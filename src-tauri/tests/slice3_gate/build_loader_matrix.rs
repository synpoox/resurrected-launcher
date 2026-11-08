//! Clause 1: the game build and loader matrix. Each row is assessed by
//! `get_product_state` and then tried with `play_product`; a refused row never
//! spawns anything.

use std::fs;
use std::path::Path;

use resurrected_launcher_lib::contract::{Readiness, LOADER_DOWNLOAD_PAGE};
use serde_json::json;

use crate::fake_loader::FakeLoader;
use crate::rig::*;

/// Play is refused with `want` and nothing was spawned.
fn play_refused(rig: &Rig, want: &str) {
    assert_eq!(code(rig.play().unwrap_err()), want);
    assert!(rig.host.spawns().is_empty(), "{:?}", rig.host.spawns());
}

#[test]
fn build_loader_matrix_supported_build_and_loader_are_ready_and_play() {
    let rig = Rig::new();
    rig.adopted();
    let state = rig.state();
    assert_eq!(state.readiness, Readiness::Ready);
    let location = state.location.unwrap();
    assert_eq!(location.build, D2R_BUILD);
    assert_eq!(state.loader_guidance, None);
    let game = rig.play().unwrap();
    let loader = FakeLoader::adopt(&game.image);
    assert_eq!(loader.pid(), game.pid);
    loader.stop();
}

/// The build label alone never makes a build supported: the fingerprint is the
/// executable's size and SHA-256.
#[test]
fn build_loader_matrix_same_size_but_different_hash_is_build_unsupported() {
    let rig = Rig::new();
    rig.adopted();
    let exe = rig.d2r.join("D2R.exe");
    let mut bytes = fs::read(&exe).unwrap();
    *bytes.last_mut().unwrap() ^= 0xff;
    fs::write(&exe, &bytes).unwrap();
    assert_eq!(fs::metadata(&exe).unwrap().len(), D2R_BYTES.len() as u64);

    let state = rig.state();
    assert_eq!(
        state.readiness,
        Readiness::BuildUnsupported {
            found: D2R_BUILD.into()
        }
    );
    play_refused(&rig, "game_build_unsupported");
}

#[test]
fn build_loader_matrix_missing_game_or_loader_file_refuses_play() {
    let rig = Rig::new();
    rig.adopted();
    let aside = |name: &str| {
        let (from, to) = (rig.d2r.join(name), rig.d2r.join(format!("{name}.aside")));
        fs::rename(&from, &to).unwrap();
        move || fs::rename(&to, &from).unwrap()
    };

    let back = aside("D2R.exe");
    let state = rig.state();
    assert_eq!(state.readiness, Readiness::D2rMissing);
    assert_eq!(state.location, None);
    play_refused(&rig, "d2r_not_found");
    back();

    for loader in ["D2RLoader.exe", "D2RCore.dll"] {
        let back = aside(loader);
        let state = rig.state();
        assert_eq!(state.readiness, Readiness::LoaderMissing, "{loader}");
        let guidance = state.loader_guidance.unwrap();
        assert_eq!(guidance.found, None);
        assert_eq!(guidance.allowed, [LOADER_VERSION]);
        assert_eq!(guidance.download_page, LOADER_DOWNLOAD_PAGE);
        play_refused(&rig, "loader_not_ready");
        back();
    }
    assert_eq!(rig.state().readiness, Readiness::Ready);
}

fn symlink_file(target: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_file(target, link).is_ok()
}

fn junction(link: &Path, target: &Path) -> bool {
    std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// A link is never the game: a linked `D2R.exe` or loader file, and a
/// junctioned D2R folder chosen in the picker.
#[test]
fn build_loader_matrix_linked_game_or_loader_refuses_play() {
    let rig = Rig::new();
    rig.adopted();
    let elsewhere = rig.d2r.parent().unwrap().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();

    // Linked files, each pointing at a byte-identical real file.
    for (name, refusal, want) in [
        ("D2R.exe", "d2r_not_found", Readiness::D2rMissing),
        (
            "D2RLoader.exe",
            "loader_not_ready",
            Readiness::LoaderMissing,
        ),
        ("D2RCore.dll", "loader_not_ready", Readiness::LoaderMissing),
    ] {
        let live = rig.d2r.join(name);
        let real = elsewhere.join(name);
        fs::rename(&live, &real).unwrap();
        if !symlink_file(&real, &live) {
            skip("this account cannot create symlinks");
            fs::rename(&real, &live).unwrap();
            break;
        }
        assert_eq!(rig.state().readiness, want, "{name}");
        play_refused(&rig, refusal);
        fs::remove_file(&live).unwrap();
        fs::rename(&real, &live).unwrap();
    }
    assert_eq!(rig.state().readiness, Readiness::Ready);

    // A junction chosen in the folder picker is refused and not remembered.
    let jn = rig.d2r.parent().unwrap().join("D2R junction");
    assert!(junction(&jn, &rig.d2r), "mklink /J failed");
    rig.desktop.will_pick(&jn);
    let picked = rig.invoke("choose_game_location", json!({}));
    assert_eq!(code(picked.unwrap_err()), "d2r_not_found");
    assert_eq!(rig.state().location.unwrap().path, rig.d2r);
    fs::remove_dir(&jn).unwrap();

    let _ = fs::remove_file(rig.d2r.join("D2RLoader.exe.started"));
}

/// The remembered D2R folder later replaced by a junction to the same files.
/// Readiness and Play refuse it like writes do (`refuse_linked_root`).
#[test]
fn build_loader_matrix_junctioned_remembered_folder_refuses_play() {
    let rig = Rig::new();
    rig.adopted();
    let moved = rig
        .d2r
        .parent()
        .unwrap()
        .join("Diablo II Resurrected (moved)");
    fs::rename(&rig.d2r, &moved).unwrap();
    assert!(junction(&rig.d2r, &moved), "mklink /J failed");
    let played = rig.play();
    let spawned = rig.host.spawns();
    if let Ok(game) = &played {
        FakeLoader::adopt(&game.image).stop();
    }
    let readiness = rig.state().readiness;
    fs::remove_dir(&rig.d2r).unwrap();
    fs::rename(&moved, &rig.d2r).unwrap();
    assert_eq!(code(played.unwrap_err()), "d2r_not_found", "{spawned:?}");
    assert_eq!(readiness, Readiness::D2rMissing);
}

#[test]
fn build_loader_matrix_unsupported_loader_version_refuses_play_with_guidance() {
    let rig = Rig::new();
    rig.adopted();
    for (file, found) in [("D2RLoader.exe", "1.2.0"), ("D2RCore.dll", "1.4.0")] {
        rig.host.set_version(file, found);
        let state = rig.state();
        assert_eq!(
            state.readiness,
            Readiness::LoaderUnsupported {
                found: found.into(),
                allowed: vec![LOADER_VERSION.into()]
            },
            "{file}"
        );
        let guidance = state.loader_guidance.unwrap();
        assert_eq!(guidance.found.as_deref(), Some(found));
        assert_eq!(guidance.download_page, LOADER_DOWNLOAD_PAGE);
        play_refused(&rig, "loader_not_ready");
        rig.host.set_version(file, LOADER_VERSION);
    }
    // Both disallowed: D2RLoader.exe is reported first.
    rig.host.set_version("D2RLoader.exe", "0.9.0");
    rig.host.set_version("D2RCore.dll", "0.8.0");
    assert!(matches!(
        rig.state().readiness,
        Readiness::LoaderUnsupported { found, .. } if found == "0.9.0"
    ));
}

/// The supported row once more with a real PE as `D2R.exe` and no injected
/// version: the build label comes from the real version resource through the
/// production FFI, and support still comes only from the fingerprint.
#[test]
fn build_loader_matrix_reads_the_build_label_from_a_real_version_resource() {
    let notepad = Path::new(r"C:\Windows\System32\notepad.exe");
    if !notepad.is_file() {
        skip("no notepad.exe to stand in for a real PE");
        return;
    }
    let dirs = Dirs::new();
    fs::copy(notepad, dirs.game.d2r().join("D2R.exe")).unwrap();
    let rig = Rig::on(
        dirs,
        Box::new(resurrected_launcher_lib::install::preflight::SystemHost),
    );
    rig.host.real_version("D2R.exe");
    rig.adopted();
    let state = rig.state();
    assert_eq!(state.readiness, Readiness::Ready);
    let build = state.location.unwrap().build;
    assert!(build.starts_with("10.0."), "{build}");
}
