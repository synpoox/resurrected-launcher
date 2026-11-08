//! Clause "Progress, errors and retry", natively: real downloads from the
//! fixture server with injected faults, cancels sent over IPC at the moment the
//! operation reports a phase, and a real game process that exits at startup.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use resurrected_launcher_lib::app::PROGRESS_EVENT;
use resurrected_launcher_lib::contract::{OperationKind, Phase, ProgressEvent, Readiness};
use resurrected_launcher_lib::lifecycle::tray::{self, Item};
use serde_json::{json, Value};
use tauri::{Listener, Manager};

use crate::helper::{self, QUICK_EXIT_CODE};
use crate::rig::*;
use crate::support::{self, FixtureRelease, Reply};

/// `rig` with `name` published and its (empty, disposable) product root ready.
fn ready_to_install(rig: &Rig, name: &str) -> (FixtureRelease, PathBuf) {
    let release = support::release(name);
    rig.publish(&release);
    let root = rig.dirs.as_ref().unwrap().game.product_root(&release);
    rig.start_up();
    (release, root)
}

/// The product root's files by content (size, hash), ignoring times.
fn content(root: &Path) -> Vec<(String, Option<(u64, String)>)> {
    tree(root)
        .into_iter()
        .map(|(k, v)| (k, v.map(|(size, hash, _)| (size, hash))))
        .collect()
}

/// Only the sentinel: nothing of the release was left behind.
fn untouched(root: &Path) {
    let names: Vec<_> = content(root).into_iter().map(|(k, _)| k).collect();
    assert_eq!(names, [".resurrected-disposable"], "{}", root.display());
}

fn no_journal(rig: &Rig) {
    let mods = rig.d2r.join("mods");
    if let Ok(entries) = fs::read_dir(&mods) {
        for e in entries {
            let name = e.unwrap().file_name().to_string_lossy().into_owned();
            assert!(!name.ends_with(".resurrected-journal.json"), "{name}");
        }
    }
    let recovered = rig
        .invoke("recover_product", json!({ "product_id": PRODUCT }))
        .unwrap();
    assert_eq!(recovered["data"], "nothing");
}

fn tray_items(view: &tray::ProductView) -> Vec<String> {
    view.items
        .iter()
        .map(|i| match i {
            Item::Action { id, label } => format!("{label} [{id}]"),
            Item::Disabled(label) => format!("({label})"),
        })
        .collect()
}

#[test]
fn an_injected_download_failure_shows_an_error_and_try_again_succeeds() {
    let rig = Rig::new();
    let (r1, root) = ready_to_install(&rig, support::PD2R_1);
    let half = r1.archive.len() / 2;
    rig.server.once(
        &r1.archive_path(),
        Reply::Reset {
            body: r1.archive.clone(),
            after: half,
        },
    );

    let failed = rig.run("install", json!({}));
    assert_eq!(failed["status"], "failed", "{failed}");
    assert_eq!(failed["error"]["code"], "download_failed", "{failed}");
    assert!(failed["error"]["message"].as_str().unwrap().len() > 3);
    untouched(&root);
    let state = rig.state();
    assert_eq!(state.installed, None);
    assert_eq!(state.running, None);
    assert_eq!(state.readiness, Readiness::NotInstalled);
    // The tray shows the error and offers Try again for the failed install.
    let view = tray::view(&state, None, Some(OperationKind::Install));
    assert_eq!(view.status, "PD2R: Something went wrong");
    assert_eq!(
        tray_items(&view),
        ["Try again [retry:pd2r]", "Show what happened [show:pd2r]"]
    );

    // Try again: the same install, which now downloads in full.
    let ok = rig.run("install", json!({}));
    assert_eq!(ok["status"], "succeeded", "{ok}");
    let state = rig.state();
    assert_eq!(
        state.installed.unwrap().release_id,
        r1.manifest.release_id.as_str()
    );
    for (rel, bytes) in &r1.payload {
        let path = rel.split('/').fold(root.clone(), |p, c| p.join(c));
        assert_eq!(&fs::read(&path).unwrap(), bytes, "{rel}");
    }
    no_journal(&rig);
}

#[test]
fn byte_progress_reaches_100_percent() {
    let rig = Rig::new();
    let (r1, _root) = ready_to_install(&rig, support::PD2R_1);
    let events = helper::progress(&rig);
    assert_eq!(rig.run("install", json!({}))["status"], "succeeded");
    let all: Vec<ProgressEvent> = events.try_iter().collect();
    let bytes: Vec<_> = all
        .iter()
        .filter_map(|e| Some((e.bytes_done?, e.bytes_total?)))
        .collect();
    let total = r1.archive.len() as u64;
    assert!(!bytes.is_empty(), "no byte progress: {all:?}");
    assert!(bytes.iter().all(|&(_, t)| t == total), "{bytes:?}");
    assert!(bytes.windows(2).all(|w| w[0].0 <= w[1].0), "{bytes:?}");
    assert_eq!(
        *bytes.last().unwrap(),
        (total, total),
        "the download ends at 100%"
    );
    // The tray's percentage reads 100 on the last byte event.
    let last = all.iter().rev().find(|e| e.bytes_done.is_some()).unwrap();
    let mut state = rig.state();
    state.running = Some(OperationKind::Install);
    assert_eq!(
        tray::view(&state, Some(last), None).status,
        format!("PD2R: Downloading {} · 100%", r1.manifest.display_version)
    );
}

/// Cancels the running operation over IPC from inside the first progress event
/// that `when` accepts, so the cancel lands while the operation is in that phase.
fn cancel_when(
    rig: &Rig,
    when: impl Fn(&ProgressEvent) -> bool + Send + 'static,
) -> Arc<AtomicBool> {
    let webview = rig.webview.clone();
    let sent = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&sent);
    rig.webview.app_handle().listen_any(PROGRESS_EVENT, move |e| {
        let v: Value = serde_json::from_str(e.payload()).unwrap();
        let p: ProgressEvent = serde_json::from_value(v["data"].clone()).unwrap();
        if when(&p) && !flag.swap(true, Ordering::SeqCst) {
            invoke(
                &webview,
                "cancel_operation",
                json!({ "request": { "contract_version": resurrected_launcher_lib::contract::CONTRACT_VERSION, "data": { "product_id": PRODUCT } } }),
            )
            .expect("cancel accepted while running");
        }
    });
    sent
}

#[test]
fn cancel_during_downloading_ends_safely() {
    let rig = Rig::new();
    let (r1, root) = ready_to_install(&rig, support::PD2R_1);
    // The archive arrives slowly, so the cancel lands mid-download.
    rig.server.route(
        &r1.archive_path(),
        Reply::Delayed(
            Duration::from_millis(1500),
            Box::new(Reply::Body(r1.archive.clone())),
        ),
    );
    let sent = cancel_when(&rig, |p| p.phase == Phase::Downloading);
    let outcome = rig.run("install", json!({}));
    assert!(
        sent.load(Ordering::SeqCst),
        "the download phase was reported"
    );
    assert_eq!(outcome["status"], "cancelled", "{outcome}");
    untouched(&root);
    let state = rig.state();
    assert_eq!((state.installed, state.running), (None, None));
    assert!(!state.cancelling);
    no_journal(&rig);
}

#[test]
fn cancel_during_applying_ends_safely() {
    let rig = Rig::new();
    let (r1, root) = ready_to_install(&rig, support::PD2R_1);
    assert_eq!(rig.run("install", json!({}))["status"], "succeeded");
    let before = content(&root);
    let r2 = support::release(support::PD2R_2);
    rig.publish(&r2);
    assert_eq!(rig.state().readiness, Readiness::UpdateAvailable);

    // `applying` in the window: activating, committing, cleaning up (journey.ts).
    let sent = cancel_when(&rig, |p| p.phase == Phase::Activating);
    let outcome = rig.run("update", json!({}));
    assert!(sent.load(Ordering::SeqCst), "activation was reported");
    let installed = rig.state().installed.unwrap().release_id;
    match outcome["status"].as_str().unwrap() {
        // Before the new files were verified live: rolled back to r1 exactly.
        "cancelled" => {
            assert_eq!(installed, r1.manifest.release_id.as_str());
            assert_eq!(content(&root), before);
        }
        // Past the point of no return the verified update commits.
        "succeeded" => assert_eq!(installed, r2.manifest.release_id.as_str()),
        other => panic!("cancel during applying ended {other}: {outcome}"),
    }
    let state = rig.state();
    assert_eq!(state.running, None);
    assert!(!state.cancelling);
    no_journal(&rig);
}

#[test]
fn a_game_that_exits_at_startup_reports_its_exit_code() {
    let rig = Rig::new();
    rig.adopted();
    // The loader the player starts fails at once with a known code.
    fs::copy(helper::quick_exit(), rig.d2r.join("D2RLoader.exe")).unwrap();
    let game = rig.play().unwrap();
    rig.state_event();
    let state = rig.state();
    let failed = state.startup_failed.unwrap_or_else(|| panic!("{state:?}"));
    assert_eq!(failed.exit_code, Some(QUICK_EXIT_CODE), "pid {}", game.pid);
    assert!(!helper::alive(game.pid));
    // The tray says the game didn't start and offers Try again.
    let view = tray::view(&state, None, None);
    assert_eq!(view.status, "PD2R: The game didn't start");
    assert_eq!(
        tray_items(&view),
        ["Try again [retry:pd2r]", "Show what happened [show:pd2r]"]
    );
    // Try again plays again; a new Play clears the failure.
    let again = rig.play().unwrap();
    assert_eq!(rig.state().startup_failed, None);
    rig.state_event();
    assert!(!helper::alive(again.pid));
}
