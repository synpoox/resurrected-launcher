//! The install operation commands (#40) and the slice 3 game commands (#60) over
//! Tauri IPC with the mock runtime, against disposable installations, the fixture
//! HTTPS server, the pinned 7-Zip, a fake [`GameHost`] (processes, versions and
//! spawning) and a fake [`Desktop`] (folder picker, browser). Nothing here touches
//! a real D2R installation or starts a real process.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use resurrected_launcher_lib::app::{
    self, CachedReleases, CatalogState, Desktop, InspectionView, Launcher, ProductState,
    RecoveryView, Release, Releases, OUTCOME_EVENT, PROGRESS_EVENT, STATE_EVENT,
};
use resurrected_launcher_lib::catalog::{GameBuild, Platform};
use resurrected_launcher_lib::contract::{
    ErrorCode, GameLocation, LauncherError, OperationKind, Phase, ProductId, ProgressEvent,
    Readiness, RelativePath, RunningGame, Versioned, CONTRACT_VERSION, LOADER_DOWNLOAD_PAGE,
};
use resurrected_launcher_lib::game::GameHost;
use resurrected_launcher_lib::install::download::Downloader;
use resurrected_launcher_lib::install::preflight::{Host, SystemHost};
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
        all.iter()
            .rev()
            .find(|r| &r.manifest.product_id == product)
            .cloned()
            .ok_or_else(|| {
                LauncherError::new(ErrorCode::MetadataInvalid, "nothing published (offline)")
            })
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
    states: Receiver<Value>,
    host: FakeGame,
    desktop: FakeDesktop,
}

/// The fake game files every rig's D2R root holds.
const D2R_BYTES: &[u8] = b"fake D2R.exe for #60";
const LOADER: &str = "1.3.1-beta";
const FAKE_PID: u32 = 4242;

type Spawned = (PathBuf, Vec<String>, PathBuf);

/// Processes, file versions and spawning, all in memory. `spawn` records the
/// command and adds a running process under the exe's folder.
#[derive(Clone, Default)]
struct FakeGame {
    processes: Arc<Mutex<Vec<RunningGame>>>,
    /// (exe, args, cwd) of every spawn.
    spawned: Arc<Mutex<Vec<Spawned>>>,
}

impl FakeGame {
    fn exit_all(&self) {
        self.processes.lock().unwrap().clear();
    }
}

impl GameHost for FakeGame {
    fn battle_net_install_locations(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn processes(&self) -> std::io::Result<Vec<RunningGame>> {
        Ok(self.processes.lock().unwrap().clone())
    }
    fn product_version(&self, file: &Path) -> std::io::Result<Option<String>> {
        let name = file.file_name().unwrap().to_string_lossy();
        Ok(file.is_file().then(|| {
            if name == "D2R.exe" {
                "3.3.fixture"
            } else {
                LOADER
            }
            .to_owned()
        }))
    }
    fn spawn(&self, exe: &Path, args: &[String], cwd: &Path) -> std::io::Result<u32> {
        let mut spawned = self.spawned.lock().unwrap();
        spawned.push((exe.to_owned(), args.to_vec(), cwd.to_owned()));
        self.processes.lock().unwrap().push(RunningGame {
            pid: FAKE_PID,
            image: exe.to_owned(),
            since: 1_700_000_000,
        });
        Ok(FAKE_PID)
    }
}

#[derive(Clone, Default)]
struct FakeDesktop {
    pick: Arc<Mutex<Option<PathBuf>>>,
    opened: Arc<AtomicUsize>,
}

impl Desktop for FakeDesktop {
    fn pick_folder(&self) -> Option<PathBuf> {
        self.pick.lock().unwrap().clone()
    }
    fn open_loader_download_page(&self) -> std::io::Result<()> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// Writes `D2R.exe`, `D2RLoader.exe` and `D2RCore.dll` into `d2r`.
fn fake_game_files(d2r: &Path) {
    fs::create_dir_all(d2r).unwrap();
    fs::write(d2r.join("D2R.exe"), D2R_BYTES).unwrap();
    fs::write(d2r.join("D2RLoader.exe"), b"loader").unwrap();
    fs::write(d2r.join("D2RCore.dll"), b"core").unwrap();
}

/// Never resolves (`.invalid`), so every catalog refresh fails like offline.
const OFFLINE_FEED: &str = "https://feed.fixtures.invalid/feed.json";

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

/// A host whose free-space check panics, standing in for a bug inside `ops::run`.
struct PanickingHost;

impl Host for PanickingHost {
    fn free_space(&self, _: &std::path::Path) -> std::io::Result<u64> {
        panic!("injected panic in the operation thread")
    }
    fn processes_using(&self, _: &[PathBuf]) -> Vec<String> {
        Vec::new()
    }
}

fn launcher(
    state_dir: &std::path::Path,
    d2r: &std::path::Path,
    releases: Box<dyn Releases>,
    server: &FixtureServer,
) -> Launcher {
    let (game, desktop) = (FakeGame::default(), FakeDesktop::default());
    launcher_on(
        state_dir,
        d2r,
        releases,
        server,
        Box::new(SystemHost),
        &game,
        &desktop,
    )
}

fn launcher_on(
    state_dir: &std::path::Path,
    d2r: &std::path::Path,
    releases: Box<dyn Releases>,
    server: &FixtureServer,
    host: Box<dyn Host>,
    game: &FakeGame,
    desktop: &FakeDesktop,
) -> Launcher {
    let mut state = StateStore::open(state_dir);
    state
        .set_preferences(Preferences {
            d2r_location: Some(d2r.to_owned()),
            ..Preferences::default()
        })
        .unwrap();
    let mut launcher = Launcher::new(
        state,
        releases,
        Downloader::with_tls(server.tls_config()),
        seven_zip(),
        host,
        Box::new(game.clone()),
        Box::new(desktop.clone()),
    );
    launcher.feed_url = OFFLINE_FEED.into();
    launcher
}

type Mounted = (
    WebviewWindow<MockRuntime>,
    Receiver<Value>,
    Receiver<Value>,
    Receiver<Value>,
);

fn mount(launcher: Launcher) -> Mounted {
    let app = mock_builder()
        .invoke_handler(app::invoke_handler())
        .manage(Arc::new(launcher))
        .build(tauri::generate_context!())
        .unwrap();
    let (out_tx, outcomes) = channel();
    let (prog_tx, progress) = channel();
    let (state_tx, states) = channel();
    app.listen_any(STATE_EVENT, move |e| {
        let _ = state_tx.send(serde_json::from_str(e.payload()).unwrap());
    });
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
    (webview, outcomes, progress, states)
}

impl Rig {
    fn new() -> Self {
        Self::on(Box::new(SystemHost))
    }

    fn on(host: Box<dyn Host>) -> Self {
        let server = FixtureServer::start();
        let game = DisposableGame::new();
        fake_game_files(game.d2r());
        let state_dir = tempfile::tempdir().unwrap();
        let releases = Arc::new(FixtureReleases::default());
        let (fake, desktop) = (FakeGame::default(), FakeDesktop::default());
        let l = launcher_on(
            state_dir.path(),
            game.d2r(),
            Box::new(Shared(releases.clone())),
            &server,
            host,
            &fake,
            &desktop,
        );
        let (webview, outcomes, progress, states) = mount(l);
        Self {
            game,
            server,
            _state_dir: state_dir,
            releases,
            webview,
            outcomes,
            progress,
            states,
            host: fake,
            desktop,
        }
    }

    /// Startup recovery, as `run` does on its worker thread. Waits for the
    /// (offline) startup refresh, then drops the events so far.
    fn start_up(&self) {
        let launcher = self.webview.state::<Arc<Launcher>>();
        launcher.inner().start_up(self.webview.app_handle());
        wait_until("the startup refresh", || {
            matches!(self.state().catalog, CatalogState::Failed { .. })
        });
        self.state_event();
        self.state_event();
        assert!(self.states.try_recv().is_err());
    }

    /// Waits for a `product-state-changed` event.
    fn state_event(&self) {
        let v = self.states.recv_timeout(Duration::from_secs(20)).unwrap();
        assert_eq!(v["contract_version"], CONTRACT_VERSION);
    }

    fn play(&self) -> Result<RunningGame, LauncherError> {
        let reply = self.invoke("play_product", json!({ "product_id": "pd2r" }))?;
        Ok(data(reply))
    }

    /// Publishes `name` as the recommended release, served by the fixture server.
    fn publish(&self, name: &str) -> FixtureRelease {
        let release = release(name);
        let mut manifest = release.manifest.clone();
        manifest.archive = self.server.serve(&release);
        // Verified with the rig's fake D2R.exe.
        manifest.compatibility.game_builds = vec![GameBuild {
            build: "fixture".into(),
            platform: Platform::WindowsX64,
            executable: RelativePath::new("D2R.exe").unwrap(),
            size: D2R_BYTES.len() as u64,
            sha256: sha256(D2R_BYTES),
        }];
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

    // The player reads the inspection for a while: the token must not depend on
    // the clock (the planned record's `installed_at` is in whole seconds).
    std::thread::sleep(Duration::from_millis(1100));
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
fn a_panicking_operation_frees_the_product_and_reports_failure() {
    let rig = Rig::on(Box::new(PanickingHost));
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    let outcome = rig.run("install", json!({}));
    assert_eq!(outcome["status"], "failed");
    assert_eq!(outcome["error"]["code"], "internal");
    assert_eq!(rig.state().running, None);
    assert!(rig.state().installed.is_none());
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
    let (webview, _, _, _) = mount(launcher(
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

// ---- slice 3 game commands (#60) ----

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn play_waits_for_startup_recovery_then_launches_the_installed_release() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    assert_eq!(rig.state().readiness, Readiness::RecoveryRequired);
    rig.start_up();
    let state = rig.state();
    assert_eq!(state.readiness, Readiness::NotInstalled);
    assert_eq!(state.location.unwrap().path, rig.game.d2r());
    assert_eq!(code(rig.play().unwrap_err()), "product_not_installed");

    assert_eq!(rig.run("install", json!({}))["status"], "succeeded");
    assert_eq!(rig.state().readiness, Readiness::Ready);
    let game = rig.play().unwrap();
    assert_eq!(game.pid, FAKE_PID);
    let spawned = rig.host.spawned.lock().unwrap().clone();
    assert_eq!(
        spawned,
        [(
            rig.game.d2r().join("D2RLoader.exe"),
            vec!["-mod".to_owned(), r1.manifest.mod_folder().to_owned()],
            rig.game.d2r().to_owned(),
        )]
    );
    let state = rig.state();
    assert_eq!(
        state.readiness,
        Readiness::Running {
            since: 1_700_000_000
        }
    );
    assert_eq!(state.running_game.unwrap().pid, FAKE_PID);
}

#[test]
fn a_running_game_refuses_play_operations_recovery_and_location_changes() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    rig.start_up();
    assert_eq!(rig.run("install", json!({}))["status"], "succeeded");
    let repair = rig.inspect("repair", json!({})).unwrap();
    rig.play().unwrap();

    assert_eq!(code(rig.play().unwrap_err()), "game_running");
    let refused = rig.start("repair", json!({}), &repair.token).unwrap_err();
    assert_eq!(code(refused), "game_running");
    let recover = rig.invoke("recover_product", json!({ "product_id": "pd2r" }));
    assert_eq!(code(recover.unwrap_err()), "game_running");
    *rig.desktop.pick.lock().unwrap() = Some(rig.game.d2r().to_owned());
    let choose = rig.invoke("choose_game_location", json!({}));
    assert_eq!(code(choose.unwrap_err()), "game_running");
    assert_eq!(rig.state().running, None);

    // The exit watcher reports the exit; then everything is allowed again.
    rig.host.exit_all();
    rig.state_event();
    assert_eq!(rig.state().readiness, Readiness::Ready);
    assert_eq!(rig.run("repair", json!({}))["status"], "succeeded");
}

/// A game that starts after the inspection but before the first live mutation is
/// caught by the preflight recheck, through the Launcher's host.
#[test]
fn an_operation_whose_game_starts_mid_way_fails_with_game_running() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    rig.server.route(
        &r1.archive_path(),
        Reply::Delayed(
            Duration::from_secs(1),
            Box::new(Reply::Body(r1.archive.clone())),
        ),
    );
    let insp = rig.inspect("install", json!({})).unwrap();
    rig.start("install", json!({}), &insp.token).unwrap();
    rig.host
        .spawn(&rig.game.d2r().join("D2R.exe"), &[], rig.game.d2r())
        .unwrap();
    let outcome = rig.outcome();
    assert_eq!(outcome["error"]["code"], "game_running", "{outcome}");
    assert!(rig.state().installed.is_none());
}

#[test]
fn play_is_refused_while_an_operation_runs() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    rig.start_up();
    assert_eq!(rig.run("install", json!({}))["status"], "succeeded");
    let r2 = rig.publish(PD2R_2);
    rig.server.route(
        &r2.archive_path(),
        Reply::Delayed(
            Duration::from_secs(2),
            Box::new(Reply::Body(r2.archive.clone())),
        ),
    );
    assert_eq!(rig.state().readiness, Readiness::UpdateAvailable);
    let insp = rig.inspect("update", json!({})).unwrap();
    rig.start("update", json!({}), &insp.token).unwrap();
    assert_eq!(code(rig.play().unwrap_err()), "operation_in_progress");
    let choose = rig.invoke("choose_game_location", json!({}));
    assert_eq!(code(choose.unwrap_err()), "operation_in_progress");
    assert_eq!(rig.outcome()["status"], "succeeded");
    assert!(rig.host.spawned.lock().unwrap().is_empty());
    rig.play().unwrap();
}

#[test]
fn loader_readiness_carries_guidance_from_the_assessed_manifest() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    rig.start_up();
    fs::remove_file(rig.game.d2r().join("D2RCore.dll")).unwrap();
    let state = rig.state();
    assert_eq!(state.readiness, Readiness::LoaderMissing);
    let guidance = state.loader_guidance.unwrap();
    assert_eq!(guidance.found, None);
    assert_eq!(guidance.allowed, r1.manifest.compatibility.loader_versions);
    assert_eq!(guidance.download_page, LOADER_DOWNLOAD_PAGE);

    rig.invoke("open_loader_download_page", json!({})).unwrap();
    assert_eq!(rig.desktop.opened.load(Ordering::SeqCst), 1);
    // "Check again" after the player installs the loader.
    fs::write(rig.game.d2r().join("D2RCore.dll"), b"core").unwrap();
    let state = rig.state();
    assert_eq!(state.readiness, Readiness::NotInstalled);
    assert_eq!(state.loader_guidance, None);
}

#[test]
fn choose_game_location_remembers_only_a_valid_picked_folder() {
    let rig = Rig::new();
    rig.publish(PD2R_1);
    let choose = || rig.invoke("choose_game_location", json!({}));
    // Cancelled picker: nothing changes.
    assert_eq!(data::<Option<GameLocation>>(choose().unwrap()), None);

    let other = tempfile::tempdir().unwrap();
    *rig.desktop.pick.lock().unwrap() = Some(other.path().to_owned());
    assert_eq!(code(choose().unwrap_err()), "d2r_not_found");
    assert_eq!(rig.state().location.unwrap().path, rig.game.d2r());

    fake_game_files(other.path());
    let chosen: Option<GameLocation> = data(choose().unwrap());
    assert_eq!(chosen.unwrap().path, other.path());
    assert_eq!(rig.state().location.unwrap().path, other.path());
    let detected: Vec<GameLocation> = data(rig.invoke("detect_game_locations", json!({})).unwrap());
    assert_eq!(detected.len(), 1);
    assert_eq!(detected[0].path, other.path());
}

#[cfg(windows)]
#[test]
fn choose_game_location_refuses_a_junctioned_root() {
    let rig = Rig::new();
    rig.publish(PD2R_1);
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("D2R-link");
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(rig.game.d2r())
        .output()
        .unwrap();
    assert!(made.status.success(), "{made:?}");
    *rig.desktop.pick.lock().unwrap() = Some(link);
    let err = rig.invoke("choose_game_location", json!({})).unwrap_err();
    assert_eq!(code(err), "d2r_not_found");
    assert_eq!(rig.state().location.unwrap().path, rig.game.d2r());
}

/// Never installed and no catalog: there is no release to offer, and the state
/// says the refresh failed rather than implying readiness.
#[test]
fn offline_with_no_catalog_reports_nothing_to_offer() {
    let server = FixtureServer::start();
    let game = DisposableGame::new();
    fake_game_files(game.d2r());
    let dir = tempfile::tempdir().unwrap();
    let (webview, _, _, _) = mount(launcher(
        dir.path(),
        game.d2r(),
        Box::new(CachedReleases),
        &server,
    ));
    let launcher = webview.state::<Arc<Launcher>>();
    launcher.inner().start_up(webview.app_handle());
    let state = || -> ProductState {
        data(
            invoke(
                &webview,
                "get_product_state",
                json!({ "product_id": "pd2r" }),
            )
            .unwrap(),
        )
    };
    wait_until("the startup refresh", || {
        matches!(state().catalog, CatalogState::Failed { .. })
    });
    let s = state();
    assert!(s.installed.is_none() && s.recommended.is_none());
    assert_eq!(s.readiness, Readiness::NotInstalled);
    let CatalogState::Failed { error } = s.catalog else {
        unreachable!()
    };
    assert_eq!(error.code, ErrorCode::DownloadFailed, "{error}");

    // On demand too, without blocking the command.
    invoke(&webview, "refresh_catalog", json!({})).unwrap();
    wait_until("the second refresh", || {
        matches!(state().catalog, CatalogState::Failed { .. })
    });
}

#[test]
fn game_commands_take_no_paths_or_urls() {
    let rig = Rig::new();
    rig.publish(PD2R_1);
    let picked = json!({ "path": "C:/Windows" });
    // Extra arguments are ignored by Tauri; the picker still decides.
    assert_eq!(
        data::<Option<GameLocation>>(rig.invoke("choose_game_location", picked).unwrap()),
        None
    );
    rig.invoke(
        "open_loader_download_page",
        json!({ "url": "https://evil.example" }),
    )
    .unwrap();
    assert_eq!(rig.desktop.opened.load(Ordering::SeqCst), 1);
    assert_eq!(rig.state().location.unwrap().path, rig.game.d2r());
}
