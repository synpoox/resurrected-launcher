//! The install operation commands (#40) over Tauri IPC with the mock runtime,
//! against disposable installations, the fixture HTTPS server and the pinned
//! 7-Zip. Nothing here touches a real D2R installation.

mod support;

use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use resurrected_launcher_lib::app::{
    self, CachedReleases, InspectionView, Launcher, ProductState, RecoveryView, Release, Releases,
    OUTCOME_EVENT, PROGRESS_EVENT,
};
use resurrected_launcher_lib::contract::{
    LauncherError, OperationKind, Phase, ProductId, ProgressEvent, Versioned, CONTRACT_VERSION,
};
use resurrected_launcher_lib::install::download::Downloader;
use resurrected_launcher_lib::install::preflight::SystemHost;
use resurrected_launcher_lib::state::{InstalledRelease, Preferences, StateStore};
use serde_json::{json, Value};
use support::*;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{Listener, Manager, WebviewWindow, WebviewWindowBuilder};

/// Fixture releases handed to the commands; the last one added per product is
/// the recommended release.
#[derive(Default)]
struct FixtureReleases(Mutex<Vec<Release>>);

impl Releases for FixtureReleases {
    fn recommended(
        &self,
        _: &mut StateStore,
        product: &ProductId,
    ) -> Result<Release, LauncherError> {
        let all = self.0.lock().unwrap();
        Ok(all
            .iter()
            .rev()
            .find(|r| &r.manifest.product_id == product)
            .expect("fixture product")
            .clone())
    }

    fn installed(
        &self,
        _: &mut StateStore,
        record: &InstalledRelease,
    ) -> Result<Release, LauncherError> {
        let all = self.0.lock().unwrap();
        Ok(all
            .iter()
            .find(|r| r.manifest.release_id == record.release_id)
            .expect("fixture release")
            .clone())
    }
}

struct Rig {
    game: DisposableGame,
    server: FixtureServer,
    _state_dir: tempfile::TempDir,
    releases: Arc<FixtureReleases>,
    webview: WebviewWindow<MockRuntime>,
    outcomes: Receiver<Value>,
    progress: Receiver<Value>,
}

/// Lets the launcher own a `Box<dyn Releases>` while the test keeps adding.
struct Shared(Arc<FixtureReleases>);

impl Releases for Shared {
    fn recommended(&self, s: &mut StateStore, p: &ProductId) -> Result<Release, LauncherError> {
        self.0.recommended(s, p)
    }
    fn installed(
        &self,
        s: &mut StateStore,
        r: &InstalledRelease,
    ) -> Result<Release, LauncherError> {
        self.0.installed(s, r)
    }
}

fn launcher(
    state_dir: &std::path::Path,
    d2r: &std::path::Path,
    releases: Box<dyn Releases>,
    server: &FixtureServer,
) -> Launcher {
    let mut state = StateStore::open(state_dir);
    state
        .set_preferences(Preferences {
            d2r_location: Some(d2r.to_owned()),
            ..Preferences::default()
        })
        .unwrap();
    Launcher::new(
        state,
        releases,
        Downloader::with_tls(server.tls_config()),
        seven_zip(),
        Box::new(SystemHost),
    )
}

fn mount(launcher: Launcher) -> (WebviewWindow<MockRuntime>, Receiver<Value>, Receiver<Value>) {
    let app = mock_builder()
        .invoke_handler(app::invoke_handler())
        .manage(Arc::new(launcher))
        .build(tauri::generate_context!())
        .unwrap();
    let (out_tx, outcomes) = channel();
    let (prog_tx, progress) = channel();
    app.listen_any(OUTCOME_EVENT, move |e| {
        let _ = out_tx.send(serde_json::from_str(e.payload()).unwrap());
    });
    app.listen_any(PROGRESS_EVENT, move |e| {
        let _ = prog_tx.send(serde_json::from_str(e.payload()).unwrap());
    });
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    // The app must outlive the test; the webview keeps a handle, not ownership.
    std::mem::forget(app);
    (webview, outcomes, progress)
}

impl Rig {
    fn new() -> Self {
        let server = FixtureServer::start();
        let game = DisposableGame::new();
        let state_dir = tempfile::tempdir().unwrap();
        let releases = Arc::new(FixtureReleases::default());
        let l = launcher(
            state_dir.path(),
            game.d2r(),
            Box::new(Shared(releases.clone())),
            &server,
        );
        let (webview, outcomes, progress) = mount(l);
        Self {
            game,
            server,
            _state_dir: state_dir,
            releases,
            webview,
            outcomes,
            progress,
        }
    }

    /// Publishes `name` as the recommended release, served by the fixture server.
    fn publish(&self, name: &str) -> FixtureRelease {
        let release = release(name);
        let mut manifest = release.manifest.clone();
        manifest.archive = self.server.serve(&release);
        self.releases.0.lock().unwrap().push(Release {
            manifest,
            sha256: sha256(&release.manifest_json),
        });
        release
    }

    fn invoke(&self, cmd: &str, body: Value) -> Result<Value, LauncherError> {
        invoke(&self.webview, cmd, body)
    }

    fn inspect(&self, kind: &str, extra: Value) -> Result<InspectionView, LauncherError> {
        let reply = self.invoke(
            "inspect_operation",
            json!({ "request": request(kind, extra) }),
        )?;
        Ok(data(reply))
    }

    fn start(&self, kind: &str, extra: Value, token: &str) -> Result<Value, LauncherError> {
        self.invoke(
            "start_operation",
            json!({ "request": request(kind, extra), "inspection_token": token }),
        )
    }

    /// Inspects, starts with the inspection's token and waits for the outcome.
    fn run(&self, kind: &str, extra: Value) -> Value {
        let insp = self.inspect(kind, extra.clone()).unwrap();
        self.start(kind, extra, &insp.token).unwrap();
        self.outcome()
    }

    fn outcome(&self) -> Value {
        let v = self.outcomes.recv_timeout(Duration::from_secs(60)).unwrap();
        assert_eq!(v["contract_version"], CONTRACT_VERSION);
        v["data"].clone()
    }

    fn state(&self) -> ProductState {
        data(
            self.invoke("get_product_state", json!({ "product_id": "pd2r" }))
                .unwrap(),
        )
    }

    fn root(&self, release: &FixtureRelease) -> PathBuf {
        self.game.d2r().join(release.manifest.install_root.as_str())
    }
}

/// Rejections of arguments Tauri cannot deserialize are plain strings; every
/// rejection by a command is a `LauncherError`.
fn raw_invoke(
    webview: &WebviewWindow<MockRuntime>,
    cmd: &str,
    body: Value,
) -> Result<Value, Value> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|b| b.deserialize().unwrap())
}

fn invoke(
    webview: &WebviewWindow<MockRuntime>,
    cmd: &str,
    body: Value,
) -> Result<Value, LauncherError> {
    raw_invoke(webview, cmd, body).map_err(|e| {
        serde_json::from_value(e.clone()).unwrap_or_else(|_| panic!("not a LauncherError: {e}"))
    })
}

fn request(kind: &str, extra: Value) -> Value {
    let mut data = json!({ "product_id": "pd2r", "kind": kind });
    data.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    json!({ "contract_version": CONTRACT_VERSION, "data": data })
}

fn data<T: serde::de::DeserializeOwned>(reply: Value) -> T {
    let v: Versioned<T> = serde_json::from_value(reply).unwrap();
    v.into_current().unwrap()
}

fn code(e: LauncherError) -> String {
    serde_json::to_value(e.code)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn install_command_installs_the_recommended_release_with_progress_events() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    let insp = rig.inspect("install", json!({})).unwrap();
    assert_eq!(insp.release.release_id, PD2R_1);
    assert!(insp.conflicts.is_empty());
    assert!(insp.files_added > 0);

    rig.start("install", json!({}), &insp.token).unwrap();
    assert_eq!(rig.outcome()["status"], "succeeded");
    let state = rig.state();
    assert_eq!(state.installed.unwrap().release_id, PD2R_1);
    assert_eq!(state.running, None);
    assert_eq!(state.recommended.unwrap().release_id, PD2R_1);

    let phases: Vec<Phase> = rig
        .progress
        .try_iter()
        .map(|v| {
            serde_json::from_value::<Versioned<ProgressEvent>>(v)
                .unwrap()
                .data
                .phase
        })
        .collect();
    for p in [
        Phase::Planning,
        Phase::Downloading,
        Phase::Extracting,
        Phase::Activating,
        Phase::Committing,
        Phase::CleaningUp,
    ] {
        assert!(phases.contains(&p), "{p:?} missing from {phases:?}");
    }
    for (rel, bytes) in &r1.payload {
        assert_eq!(&fs::read(rig.root(&r1).join(rel)).unwrap(), bytes, "{rel}");
    }
}

#[test]
fn uninstall_command_refuses_a_plan_that_changed_since_the_player_saw_it() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    assert_eq!(rig.run("install", json!({}))["status"], "succeeded");

    let seen = rig.inspect("uninstall", json!({})).unwrap();
    assert!(seen.preserved.is_empty());
    // The player edits the loot filter after confirming: uninstall now keeps it.
    let edited = edit_loot_filter(&rig.root(&r1));
    let err = rig.start("uninstall", json!({}), &seen.token).unwrap_err();
    assert_eq!(code(err), "inventory_mismatch");
    assert_eq!(rig.state().installed.unwrap().release_id, PD2R_1);

    let fresh = rig.inspect("uninstall", json!({})).unwrap();
    assert_ne!(fresh.token, seen.token);
    assert_eq!(fresh.preserved.len(), 1);
    rig.start("uninstall", json!({}), &fresh.token).unwrap();
    assert_eq!(rig.outcome()["status"], "succeeded");
    assert!(rig.state().installed.is_none());
    assert_eq!(fs::read(rig.root(&r1).join(LOOT_FILTER)).unwrap(), edited);
}

#[test]
fn start_command_refuses_a_token_from_another_inspection() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    for token in ["", "not-a-token"] {
        assert_eq!(
            code(rig.start("install", json!({}), token).unwrap_err()),
            "inventory_mismatch"
        );
    }
    // A clean install token does not start an install.
    let clean = rig.inspect("clean_install", json!({})).unwrap();
    assert_eq!(
        code(rig.start("install", json!({}), &clean.token).unwrap_err()),
        "inventory_mismatch"
    );
    assert!(rig.state().installed.is_none());
}

#[test]
fn clean_install_command_needs_consent_as_well_as_the_inspection() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    let outcome = rig.run("clean_install", json!({}));
    assert_eq!(outcome["status"], "failed");
    assert_eq!(outcome["error"]["code"], "clean_install_consent_required");
    let outcome = rig.run("clean_install", json!({ "consent_clean_install": true }));
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(rig.state().installed.unwrap().release_id, PD2R_1);
}

#[test]
fn cancel_command_stops_a_download_and_keeps_the_previous_installation() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    rig.server.route(
        &r1.archive_path(),
        Reply::Delayed(
            Duration::from_secs(2),
            Box::new(Reply::Body(r1.archive.clone())),
        ),
    );
    let insp = rig.inspect("install", json!({})).unwrap();
    rig.start("install", json!({}), &insp.token).unwrap();
    assert_eq!(rig.state().running, Some(OperationKind::Install));
    // One operation per product at a time.
    let again = rig.start("install", json!({}), &insp.token).unwrap_err();
    assert_eq!(code(again), "operation_in_progress");

    let cancel = json!({ "request": { "contract_version": CONTRACT_VERSION, "data": { "product_id": "pd2r" } } });
    rig.invoke("cancel_operation", cancel.clone()).unwrap();
    assert_eq!(rig.outcome()["status"], "cancelled");
    assert!(rig.state().installed.is_none());
    assert_eq!(rig.state().running, None);
    // Only the sentinel is left in the product root.
    assert_eq!(fs::read_dir(rig.root(&r1)).unwrap().count(), 1);
    assert_eq!(
        code(rig.invoke("cancel_operation", cancel).unwrap_err()),
        "invalid_request"
    );
}

#[test]
fn commands_never_write_a_product_root_without_the_disposable_sentinel() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    // A real-looking root: present, but no sentinel.
    fs::create_dir_all(rig.root(&r1)).unwrap();
    let outcome = rig.run("install", json!({}));
    assert_eq!(outcome["error"]["code"], "product_root_not_disposable");
    assert_eq!(fs::read_dir(rig.root(&r1)).unwrap().count(), 0);
    assert!(rig.state().installed.is_none());
}

#[test]
fn commands_refuse_paths_urls_and_other_contract_versions() {
    let rig = Rig::new();
    rig.publish(PD2R_1);
    for extra in [json!({ "path": "C:/x" }), json!({ "url": "https://x" })] {
        let body = json!({ "request": request("install", extra) });
        let err = raw_invoke(&rig.webview, "inspect_operation", body).unwrap_err();
        assert!(err.as_str().unwrap().contains("unknown field"), "{err}");
    }
    let old = json!({ "request": { "contract_version": CONTRACT_VERSION + 1, "data": { "product_id": "pd2r", "kind": "install" } } });
    let err = rig.invoke("inspect_operation", old).unwrap_err();
    assert_eq!(code(err), "unsupported_contract_version");
    // Repair of a product that is not installed is not a valid intent.
    assert_eq!(
        code(rig.inspect("repair", json!({})).unwrap_err()),
        "invalid_request"
    );
}

#[test]
fn recover_command_reports_nothing_to_recover_on_a_clean_installation() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    let reply = rig
        .invoke("recover_product", json!({ "product_id": "pd2r" }))
        .unwrap();
    assert_eq!(data::<RecoveryView>(reply), RecoveryView::Nothing);
}

#[test]
fn cached_release_metadata_drives_inspection_and_fails_closed_when_missing() {
    let server = FixtureServer::start();
    let game = DisposableGame::new();
    let r1 = release(PD2R_1);
    game.product_root(&r1);
    let dir = tempfile::tempdir().unwrap();
    let (webview, _, _) = mount(launcher(
        dir.path(),
        game.d2r(),
        Box::new(CachedReleases),
        &server,
    ));
    let inspect = |product: &str| {
        let req = json!({ "contract_version": CONTRACT_VERSION, "data": { "product_id": product, "kind": "install" } });
        invoke(&webview, "inspect_operation", json!({ "request": req }))
    };
    assert_eq!(code(inspect("pd2r").unwrap_err()), "metadata_invalid");

    {
        // Seed the cache the way the Catalog would after validating fetched bytes.
        let state = webview.state::<Arc<Launcher>>();
        let mut store = state.state.lock().unwrap();
        store.put_cached("feed", &feed(&[&r1])).unwrap();
        store
            .put_cached(&format!("manifest/pd2r/{PD2R_1}"), &r1.manifest_json)
            .unwrap();
    }
    let insp: InspectionView = data(inspect("pd2r").unwrap());
    assert_eq!(insp.release.release_id, PD2R_1);
    assert_eq!(
        code(inspect("not-a-product").unwrap_err()),
        "unknown_product"
    );
}
