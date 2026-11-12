//! The install operation commands (#40) and the slice 3 game commands (#60) over
//! Tauri IPC with the mock runtime, against disposable installations, the fixture
//! HTTPS server, the pinned 7-Zip, a fake [`GameHost`] (processes, versions and
//! spawning) and a fake [`Desktop`] (folder picker, browser). Nothing here touches
//! a real D2R installation or starts a real process.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use resurrected_launcher_lib::app::{
    self, CachedReleases, CatalogState, Desktop, InspectionView, Launcher, LocationView,
    ProductState, RecoveryView, Release, ReleaseNotes, Releases, StartupFailed, OUTCOME_EVENT,
    PROGRESS_EVENT, STATE_EVENT,
};
use resurrected_launcher_lib::catalog::{GameBuild, Platform, ReleaseManifest};
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
struct FixtureReleases(Mutex<Vec<Release>>, Mutex<Option<Gate>>);

/// Makes the next `installed` call announce itself, then wait to be released.
type Gate = (Sender<()>, Receiver<()>);

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
        if let Some((entered, release)) = self.1.lock().unwrap().take() {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
        let all = self.0.lock().unwrap();
        Ok(all
            .iter()
            .find(|r| r.manifest.release_id == record.release_id)
            .expect("fixture release")
            .clone())
    }

    /// Every other published release of the product, like the cached manifests.
    fn earlier(
        &self,
        _: &mut StateStore,
        product: &ProductId,
        release_id: &str,
    ) -> Vec<ReleaseManifest> {
        let all = self.0.lock().unwrap();
        all.iter()
            .map(|r| &r.manifest)
            .filter(|m| &m.product_id == product && m.release_id != release_id)
            .cloned()
            .collect()
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
/// A pid no process can have (#226): `OpenProcess` ignores a pid's low two bits,
/// so a small fake like 4242 opens whichever real process holds 4240 and yields
/// an exit code the test expects to be unknown.
const FAKE_PID: u32 = u32::MAX - 1;

type Spawned = (PathBuf, Vec<String>, PathBuf);

/// Processes, file versions and spawning, all in memory. `spawn` records the
/// command and adds a running process under the exe's folder.
#[derive(Clone, Default)]
struct FakeGame {
    /// The pid `spawn` reports instead of [`FAKE_PID`], e.g. a real child's.
    spawn_pid: Arc<Mutex<Option<u32>>>,
    processes: Arc<Mutex<Vec<RunningGame>>>,
    /// Set: listing processes fails.
    fail_listing: Arc<AtomicBool>,
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
        if self.fail_listing.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("injected listing failure"));
        }
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
        let pid = self.spawn_pid.lock().unwrap().unwrap_or(FAKE_PID);
        self.processes.lock().unwrap().push(RunningGame {
            pid,
            image: exe.to_owned(),
            since: 1_700_000_000,
        });
        Ok(pid)
    }
}

#[derive(Clone, Default)]
struct FakeDesktop {
    pick: Arc<Mutex<Option<PathBuf>>>,
    opened: Arc<AtomicUsize>,
    /// Every folder opened in "Explorer".
    folders: Arc<Mutex<Vec<PathBuf>>>,
    /// Every file shown in its folder.
    shown: Arc<Mutex<Vec<PathBuf>>>,
    /// What the save dialog returns; `None` is Cancel.
    save_to: Arc<Mutex<Option<PathBuf>>>,
    saved_games: Arc<Mutex<Option<PathBuf>>>,
    /// The launcher's Run value.
    run: Arc<Mutex<Option<String>>>,
}

impl Desktop for FakeDesktop {
    fn pick_folder(&self) -> Option<PathBuf> {
        self.pick.lock().unwrap().clone()
    }
    fn open_loader_download_page(&self) -> std::io::Result<()> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn open_link(&self, _: &str) -> std::io::Result<()> {
        Ok(())
    }
    fn open_folder(&self, dir: &Path) -> std::io::Result<()> {
        self.folders.lock().unwrap().push(dir.to_owned());
        Ok(())
    }
    fn show_in_folder(&self, file: &Path) -> std::io::Result<()> {
        self.shown.lock().unwrap().push(file.to_owned());
        Ok(())
    }
    fn pick_save_file(&self, _: &str) -> Option<PathBuf> {
        self.save_to.lock().unwrap().clone()
    }
    fn saved_games(&self) -> std::io::Result<PathBuf> {
        self.saved_games
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| std::io::Error::other("no Saved Games"))
    }
    fn run_value(&self) -> std::io::Result<Option<String>> {
        Ok(self.run.lock().unwrap().clone())
    }
    fn set_run_value(&self, value: Option<&str>) -> std::io::Result<()> {
        *self.run.lock().unwrap() = value.map(str::to_owned);
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
    fn earlier(&self, s: &mut StateStore, p: &ProductId, id: &str) -> Vec<ReleaseManifest> {
        self.0.earlier(s, p, id)
    }
}

/// A host whose second sharing check (the recheck before the first live
/// mutation, after the journal is written) panics: an interrupted first install.
#[derive(Default)]
struct CrashBeforeActivation(AtomicUsize);

impl Host for CrashBeforeActivation {
    fn free_space(&self, _: &std::path::Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }
    fn processes_using(&self, _: &[PathBuf]) -> Vec<String> {
        if self.0.fetch_add(1, Ordering::SeqCst) == 1 {
            panic!("injected crash before activation");
        }
        Vec::new()
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
        Self::tuned(host, |_| {})
    }

    /// Like [`Rig::on`], with `tune` applied to the launcher before it is mounted.
    fn tuned(host: Box<dyn Host>, tune: impl FnOnce(&mut Launcher)) -> Self {
        let server = FixtureServer::start();
        let game = DisposableGame::new();
        fake_game_files(game.d2r());
        let state_dir = tempfile::tempdir().unwrap();
        let releases = Arc::new(FixtureReleases::default());
        let (fake, desktop) = (FakeGame::default(), FakeDesktop::default());
        let mut l = launcher_on(
            state_dir.path(),
            game.d2r(),
            Box::new(Shared(releases.clone())),
            &server,
            host,
            &fake,
            &desktop,
        );
        tune(&mut l);
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
        self.publish_with(name, |_| {})
    }

    /// [`Rig::publish`], with `tweak` applied to the published manifest.
    fn publish_with(&self, name: &str, tweak: impl FnOnce(&mut ReleaseManifest)) -> FixtureRelease {
        let release = release(name);
        let mut manifest = release.manifest.clone();
        tweak(&mut manifest);
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

    /// The progress events so far.
    fn progress_events(&self) -> Vec<ProgressEvent> {
        self.progress
            .try_iter()
            .map(|v| {
                serde_json::from_value::<Versioned<ProgressEvent>>(v)
                    .unwrap()
                    .data
            })
            .collect()
    }

    /// Installs `name` through the commands; returns the product root.
    fn install(&self, name: &str) -> PathBuf {
        let r = self.publish(name);
        self.game.product_root(&r);
        assert_eq!(self.run("install", json!({}))["status"], "succeeded");
        self.root(&r)
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

    let events = rig.progress_events();
    let phases: Vec<Phase> = events.iter().map(|e| e.phase).collect();
    let bytes: Vec<_> = events.iter().filter(|e| e.bytes_done.is_some()).collect();
    let last = bytes.last().expect("byte progress while downloading");
    assert_eq!(last.phase, Phase::Downloading);
    assert_eq!(last.bytes_done, last.bytes_total, "the final count is sent");
    assert_eq!(last.bytes_total, Some(r1.manifest.archive.size));
    assert!(events.iter().all(|e| e.files_done.is_none()));
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

    assert!(!rig.state().cancelling);

    let cancel = json!({ "request": { "contract_version": CONTRACT_VERSION, "data": { "product_id": "pd2r" } } });
    rig.invoke("cancel_operation", cancel.clone()).unwrap();
    // The download waits on the delayed reply, so the cancel is still pending.
    assert!(rig.state().cancelling);
    assert_eq!(rig.outcome()["status"], "cancelled");
    assert!(rig.state().installed.is_none());
    assert_eq!(rig.state().running, None);
    assert!(!rig.state().cancelling);
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

/// Phase B of the slice 3 gate (#61): a real D2R folder's `mods\<folder>` is
/// writable, but never through a junction. Here `mods` is a junction to a
/// folder elsewhere, so the write would land outside the chosen D2R folder.
#[cfg(windows)]
#[test]
fn commands_never_write_a_product_root_reached_through_a_junction() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    let elsewhere = tempfile::tempdir().unwrap();
    let mods = rig.game.d2r().join("mods");
    fs::remove_dir_all(&mods).unwrap();
    let made = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(&mods)
        .arg(elsewhere.path())
        .output()
        .is_ok_and(|o| o.status.success());
    if !made {
        eprintln!("SKIPPED: mklink /J failed");
        return;
    }
    // A real-looking root: present, but no sentinel, through the junction.
    fs::create_dir_all(rig.root(&r1)).unwrap();
    let outcome = rig.run("install", json!({}));
    assert_eq!(outcome["error"]["code"], "product_root_not_disposable");
    assert_eq!(fs::read_dir(rig.root(&r1)).unwrap().count(), 0);
    assert_eq!(fs::read_dir(elsewhere.path()).unwrap().count(), 1);
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
    assert_eq!(state.location.unwrap().location.path, rig.game.d2r());
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

/// #153: a game started outside Play (the loader run by hand, a desktop shortcut)
/// after startup is announced, so the UI rereads state and holds Restart to
/// update; its exit is announced too.
#[test]
fn a_game_started_outside_the_launcher_is_announced_and_its_exit_too() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    rig.start_up();
    assert_eq!(rig.run("install", json!({}))["status"], "succeeded");
    while rig.states.try_recv().is_ok() {}
    assert_eq!(rig.state().readiness, Readiness::Ready);

    rig.host.processes.lock().unwrap().push(RunningGame {
        pid: FAKE_PID,
        image: rig.game.d2r().join("D2R.exe"),
        since: 1_700_000_000,
    });
    rig.state_event();
    assert_eq!(
        rig.state().readiness,
        Readiness::Running {
            since: 1_700_000_000
        }
    );

    rig.host.exit_all();
    rig.state_event();
    assert_eq!(rig.state().readiness, Readiness::Ready);
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
    assert_eq!(rig.state().location.unwrap().location.path, rig.game.d2r());

    fake_game_files(other.path());
    let chosen: Option<GameLocation> = data(choose().unwrap());
    assert_eq!(chosen.unwrap().path, other.path());
    assert_eq!(rig.state().location.unwrap().location.path, other.path());
    let detected: Vec<GameLocation> = data(rig.invoke("detect_game_locations", json!({})).unwrap());
    assert_eq!(detected.len(), 1);
    assert_eq!(detected[0].path, other.path());
}

/// #123: Settings and the product page each hold their own copy of product
/// state; only `product-state-changed` tells the one that didn't ask to reread.
/// A remembered location emits it; a cancelled or refused choice does not.
#[test]
fn a_location_change_emits_product_state_changed() {
    let rig = Rig::new();
    rig.publish(PD2R_1);
    let quiet = |what: &str| assert!(rig.states.try_recv().is_err(), "{what} emitted");
    let choose = || rig.invoke("choose_game_location", json!({}));

    choose().unwrap();
    quiet("a cancelled picker");
    let other = tempfile::tempdir().unwrap();
    *rig.desktop.pick.lock().unwrap() = Some(other.path().to_owned());
    choose().unwrap_err();
    quiet("a refused folder");

    fake_game_files(other.path());
    choose().unwrap();
    rig.state_event();
    quiet("a second event for one choice");

    rig.invoke("detect_game_locations", json!({})).unwrap();
    rig.invoke("select_game_location", json!({ "index": 0 }))
        .unwrap();
    rig.state_event();
    rig.invoke("select_game_location", json!({ "index": 9 }))
        .unwrap_err();
    quiet("an out-of-range selection");
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
    assert_eq!(rig.state().location.unwrap().location.path, rig.game.d2r());
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
    assert_eq!(rig.state().location.unwrap().location.path, rig.game.d2r());
}

#[test]
fn a_process_listing_failure_blocks_like_a_running_game() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    rig.start_up();
    assert_eq!(rig.run("install", json!({}))["status"], "succeeded");
    let repair = rig.inspect("repair", json!({})).unwrap();
    rig.host.fail_listing.store(true, Ordering::SeqCst);

    // State still answers, with a blocking reason instead of an error.
    assert_eq!(rig.state().readiness, Readiness::Running { since: 0 });
    assert_eq!(code(rig.play().unwrap_err()), "game_running");
    let start = rig.start("repair", json!({}), &repair.token);
    assert_eq!(code(start.unwrap_err()), "game_running");
    let recover = rig.invoke("recover_product", json!({ "product_id": "pd2r" }));
    assert_eq!(code(recover.unwrap_err()), "game_running");
    assert!(rig.host.spawned.lock().unwrap().is_empty());

    rig.host.fail_listing.store(false, Ordering::SeqCst);
    assert_eq!(rig.state().readiness, Readiness::Ready);
}

#[test]
fn play_and_location_changes_wait_for_a_running_recovery() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    rig.start_up();
    assert_eq!(rig.run("install", json!({}))["status"], "succeeded");

    let (entered_tx, entered) = channel();
    let (release, release_rx) = channel();
    *rig.releases.1.lock().unwrap() = Some((entered_tx, release_rx));
    let webview = rig.webview.clone();
    let recovery = std::thread::spawn(move || {
        invoke(&webview, "recover_product", json!({ "product_id": "pd2r" }))
    });
    entered.recv_timeout(Duration::from_secs(20)).unwrap();
    // Recovery holds its reservation: nothing else may start under the root.
    assert_eq!(code(rig.play().unwrap_err()), "operation_in_progress");
    let choose = rig.invoke("choose_game_location", json!({}));
    assert_eq!(code(choose.unwrap_err()), "operation_in_progress");
    let again = rig.invoke("recover_product", json!({ "product_id": "pd2r" }));
    assert_eq!(code(again.unwrap_err()), "operation_in_progress");

    release.send(()).unwrap();
    let reply = recovery.join().unwrap().unwrap();
    assert_eq!(data::<RecoveryView>(reply), RecoveryView::Nothing);
    assert!(rig.host.spawned.lock().unwrap().is_empty());
    rig.play().unwrap();
}

/// A start reserves before it reads the D2R root (#97): while it resolves, no
/// location change or Play can start, and a game started outside the launcher
/// in that window is still seen before the operation runs.
#[test]
fn a_game_started_while_an_operation_resolves_its_root_refuses_the_start() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    rig.start_up();
    assert_eq!(rig.run("install", json!({}))["status"], "succeeded");
    let repair = rig.inspect("repair", json!({})).unwrap();

    let (entered_tx, entered) = channel();
    let (release, release_rx) = channel();
    *rig.releases.1.lock().unwrap() = Some((entered_tx, release_rx));
    let webview = rig.webview.clone();
    let token = repair.token.clone();
    let start = std::thread::spawn(move || {
        invoke(
            &webview,
            "start_operation",
            json!({ "request": request("repair", json!({})), "inspection_token": token }),
        )
    });
    // The start is inside `resolve`, past reading the D2R root.
    entered.recv_timeout(Duration::from_secs(20)).unwrap();

    // Refused at once, not queued behind the read: the root cannot change and
    // no Play starts. Run on threads so a regression fails instead of hanging.
    *rig.desktop.pick.lock().unwrap() = Some(rig.game.d2r().to_owned());
    let attempt = |cmd: &'static str, body: Value| {
        let (tx, rx) = channel();
        let webview = rig.webview.clone();
        std::thread::spawn(move || tx.send(invoke(&webview, cmd, body)).unwrap());
        rx
    };
    let choose = attempt("choose_game_location", json!({}));
    let play = attempt("play_product", json!({ "product_id": "pd2r" }));
    for refused in [choose, play] {
        let reply = refused
            .recv_timeout(Duration::from_secs(5))
            .expect("refused at once");
        assert_eq!(code(reply.unwrap_err()), "operation_in_progress");
    }
    assert!(rig.host.spawned.lock().unwrap().is_empty());

    // A game started outside the launcher between the read and the check.
    rig.host
        .spawn(&rig.game.d2r().join("D2R.exe"), &[], rig.game.d2r())
        .unwrap();
    release.send(()).unwrap();
    assert_eq!(code(start.join().unwrap().unwrap_err()), "game_running");
    // The refusal freed the reservation; no operation ran.
    assert_eq!(rig.state().running, None);
    assert!(rig.outcomes.try_recv().is_err());

    rig.host.exit_all();
    assert_eq!(
        rig.start("repair", json!({}), &repair.token).unwrap(),
        Value::Null
    );
    assert_eq!(rig.outcome()["status"], "succeeded");
}

/// An interrupted first install leaves a journal but no installed record; startup
/// recovers it through the cached feed's product list.
#[test]
fn startup_recovers_an_interrupted_first_install() {
    let rig = Rig::on(Box::new(CrashBeforeActivation::default()));
    let r1 = rig.publish(PD2R_1);
    let root = rig.game.product_root(&r1);
    let outcome = rig.run("install", json!({}));
    assert_eq!(outcome["error"]["code"], "internal", "{outcome}");
    assert!(rig.state().installed.is_none());
    let journal = root.with_file_name("PD2R.resurrected-journal.json");
    assert!(journal.exists(), "no journal at {}", journal.display());
    {
        let launcher = rig.webview.state::<Arc<Launcher>>();
        let mut store = launcher.state.lock().unwrap();
        store.put_cached("feed", &feed(&[&r1])).unwrap();
    }

    rig.start_up();
    assert!(!journal.exists());
    assert_eq!(rig.state().readiness, Readiness::NotInstalled);
    // Only the sentinel is left in the product root.
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
}

/// #234: a coming-soon product has nothing to recover; startup must not try it
/// (which fails with `InvalidRequest`) nor log a failure, while an installed
/// product with a journal still recovers.
#[test]
fn startup_skips_coming_soon_products_and_still_recovers_installed_ones() {
    let rig = Rig::on(Box::new(CrashBeforeActivation::default()));
    let r1 = rig.publish(PD2R_1);
    let root = rig.game.product_root(&r1);
    let outcome = rig.run("install", json!({}));
    assert_eq!(outcome["error"]["code"], "internal", "{outcome}");
    let journal = root.with_file_name("PD2R.resurrected-journal.json");
    assert!(journal.exists());
    let mut feed: Value = serde_json::from_slice(&feed(&[&r1])).unwrap();
    for id in ["project-09", "project-forever"] {
        feed["products"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "product_id": id, "availability": "upcoming" }));
    }
    {
        let launcher = rig.webview.state::<Arc<Launcher>>();
        let mut store = launcher.state.lock().unwrap();
        store
            .put_cached("feed", &serde_json::to_vec(&feed).unwrap())
            .unwrap();
    }

    rig.start_up();
    assert!(!journal.exists(), "the installed product still recovers");
    let text = fs::read_to_string(rig._state_dir.path().join("launcher.log")).unwrap();
    assert!(!text.contains("project-09"), "{text}");
    assert!(!text.contains("project-forever"), "{text}");
    assert!(text.contains("recovery of pd2r: RolledBack"), "{text}");
    assert!(!text.contains("failed: InvalidRequest"), "{text}");
}

// ---- slice 4 command gaps (#74) ----

fn strings(v: &Value) -> Vec<String> {
    serde_json::from_value(v.clone()).unwrap()
}

/// #215: `Launcher::resolve` passes `Releases::earlier` as the history for an
/// update and a clean install over an installed release, so a skipped
/// save-breaking release counts even when the target is not itself flagged. The
/// `FixtureReleases` double returns every published manifest; the real lookup of
/// cached manifests is covered by `operations::tests::earlier_lists_other_valid_cached_manifests_of_the_product`.
#[test]
fn resolve_passes_the_release_history_to_the_save_break_check() {
    let rig = Rig::new();
    rig.install(PD2R_1);
    rig.publish_with(PD2R_2, |m| {
        m.release_date = "2099-01-02".into();
        m.saves.incompatibility_notice = Some("Saves break.".into());
    });
    let r3 = rig.publish_with(PD2R_3, |m| {
        m.release_date = "2099-01-03".into();
        m.saves.incompatibility_notice = None;
    });
    for kind in ["update", "clean_install"] {
        let insp = rig.inspect(kind, json!({})).unwrap();
        assert!(!insp.release.save_breaking, "{kind}");
        assert!(insp.breaks_saves, "{kind} must see the skipped release");
    }

    // The backend refuses the unacknowledged start and goes ahead with it.
    let insp = rig.inspect("update", json!({})).unwrap();
    rig.start("update", json!({}), &insp.token).unwrap();
    let refused = rig.outcome();
    assert_eq!(refused["error"]["code"], "save_breaking_not_acknowledged");
    assert_eq!(rig.state().installed.unwrap().release_id, PD2R_1);
    let ack = json!({ "acknowledge_save_breaking": true });
    assert_eq!(rig.run("update", ack)["status"], "succeeded");
    assert_eq!(rig.state().installed.unwrap().release_id, r3.release_id());
}

#[test]
fn update_command_needs_acknowledgement_only_for_a_save_breaking_release() {
    let rig = Rig::new();
    let root = rig.install(PD2R_1);
    let edited = edit_loot_filter(&root);
    rig.publish(PD2R_2);
    let insp = rig.inspect("update", json!({})).unwrap();
    assert_eq!(insp.save_breaking_notice, None);
    let outcome = rig.run("update", json!({}));
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(strings(&outcome["report"]["preserved"]), [LOOT_FILTER]);
    assert_eq!(fs::read(root.join(LOOT_FILTER)).unwrap(), edited);
    assert_eq!(rig.state().installed.unwrap().release_id, PD2R_2);

    let r3 = rig.publish(PD2R_3);
    let insp = rig.inspect("update", json!({})).unwrap();
    assert!(insp.save_breaking_notice.is_some());
    assert!(insp.release.save_breaking);
    rig.start("update", json!({}), &insp.token).unwrap();
    let refused = rig.outcome();
    assert_eq!(refused["error"]["code"], "save_breaking_not_acknowledged");
    assert_eq!(rig.state().installed.unwrap().release_id, PD2R_2);
    assert!(rig.server.hits().iter().all(|h| *h != r3.archive_path()));

    let ack = json!({ "acknowledge_save_breaking": true });
    let outcome = rig.run("update", ack);
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(rig.state().installed.unwrap().release_id, PD2R_3);
}

#[test]
fn repair_command_restores_a_deleted_managed_file_and_counts_files() {
    let rig = Rig::new();
    let root = rig.install(PD2R_1);
    let r1 = release(PD2R_1);
    let (rel, bytes) = r1
        .payload
        .iter()
        .find(|(rel, _)| rel.as_str() != LOOT_FILTER)
        .unwrap();
    fs::remove_file(root.join(rel)).unwrap();
    let insp = rig.inspect("repair", json!({})).unwrap();
    assert_eq!(insp.files_added, 1);
    rig.progress_events();

    let outcome = rig.run("repair", json!({}));
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(&fs::read(root.join(rel)).unwrap(), bytes);
    let files: Vec<_> = rig
        .progress_events()
        .into_iter()
        .filter(|e| e.files_done.is_some())
        .collect();
    let last = files.last().expect("file progress while Repair verifies");
    assert_eq!(last.phase, Phase::Verifying);
    assert_eq!(last.files_done, last.files_total);
    assert!(last.files_total.unwrap() >= r1.payload.len() as u64);
}

#[test]
fn uninstall_command_reports_the_files_it_left() {
    let rig = Rig::new();
    let root = rig.install(PD2R_1);
    edit_loot_filter(&root);
    add_unknown_plugin(&root);
    let outcome = rig.run("uninstall", json!({}));
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    let report = &outcome["report"];
    let remaining = strings(&report["remaining"]);
    assert!(
        remaining.iter().any(|r| r == UNKNOWN_PLUGIN),
        "{remaining:?}"
    );
    assert!(remaining.iter().any(|r| r == LOOT_FILTER), "{remaining:?}");
    assert_eq!(strings(&report["preserved"]), [LOOT_FILTER]);
    assert_eq!(report["clean_install_backups"], json!([]));
    assert!(rig.state().installed.is_none());

    // #156: the result dialog's Open folder actions still work once uninstalled.
    let saved = tempfile::tempdir().unwrap();
    *rig.desktop.saved_games.lock().unwrap() = Some(saved.path().to_owned());
    let saves = saved.path().join(r"Diablo II Resurrected\mods\PD2R");
    fs::create_dir_all(&saves).unwrap();
    let open = |folder: &str| {
        rig.invoke(
            "open_folder",
            json!({ "product_id": "pd2r", "folder": folder }),
        )
        .map(data::<bool>)
    };
    assert!(!open("saves").unwrap());
    assert!(!open("settings").unwrap(), "left files keep the folder");
    assert_eq!(
        code(open("product_plugins").unwrap_err()),
        "product_not_installed",
        "never creates plugins for a product that isn't there"
    );
    assert_eq!(*rig.desktop.folders.lock().unwrap(), [saves, root]);
}

#[test]
fn select_game_location_picks_from_the_last_detection() {
    let rig = Rig::new();
    let select = |i: usize| rig.invoke("select_game_location", json!({ "index": i }));
    assert_eq!(code(select(0).unwrap_err()), "invalid_request");

    let detected: Vec<LocationView> = data(rig.invoke("detect_game_locations", json!({})).unwrap());
    assert_eq!(detected.len(), 1);
    assert!(detected[0].installed_products.is_empty());
    rig.install(PD2R_1);
    let detected: Vec<LocationView> = data(rig.invoke("detect_game_locations", json!({})).unwrap());
    assert_eq!(detected[0].installed_products, [ProductId("pd2r".into())]);
    // The wire shape is flat, like GameLocation plus the product list.
    let raw = rig.invoke("detect_game_locations", json!({})).unwrap();
    assert!(raw["data"][0]["path"].is_string());
    assert_eq!(raw["data"][0]["installed_products"], json!(["pd2r"]));

    assert_eq!(code(select(1).unwrap_err()), "invalid_request");
    let chosen: LocationView = data(select(0).unwrap());
    assert_eq!(chosen.location.path, rig.game.d2r());

    rig.host
        .spawn(&rig.game.d2r().join("D2R.exe"), &[], rig.game.d2r())
        .unwrap();
    assert_eq!(code(select(0).unwrap_err()), "game_running");
}

#[test]
fn a_game_that_exits_soon_after_play_is_a_startup_failure_until_the_next_play() {
    let rig = Rig::new();
    rig.install(PD2R_1);
    rig.start_up();
    assert_eq!(rig.state().startup_failed, None);
    rig.play().unwrap();
    assert_eq!(rig.state().startup_failed, None, "still running");
    rig.host.exit_all();
    rig.state_event();
    // The fake pid cannot be opened as the started process, so no exit code.
    let failed = rig.state().startup_failed.unwrap();
    assert_eq!(failed, StartupFailed { exit_code: None });
    rig.play().unwrap();
    assert_eq!(rig.state().startup_failed, None);
}

#[test]
fn a_loader_that_exits_before_the_game_appears_is_not_a_startup_failure() {
    // #200: the loader hands off to D2R.exe and exits; the watch used to end the
    // moment nothing ran under the root, flagging a startup failure while the
    // game was starting (or running).
    let rig = Rig::new();
    rig.install(PD2R_1);
    rig.start_up();
    rig.play().unwrap();
    rig.host.exit_all();
    std::thread::sleep(Duration::from_millis(1500));
    let game = rig.game.d2r().join("D2R.exe");
    rig.host.processes.lock().unwrap().push(RunningGame {
        pid: 5151,
        image: game,
        since: 1_700_000_001,
    });
    // Past the poll that would have ended the old watch.
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(rig.state().startup_failed, None, "the game is running");
    assert!(rig.states.try_recv().is_err(), "the watch has not ended");
    // A game that dies right away is still a startup failure.
    rig.host.exit_all();
    rig.state_event();
    assert!(rig.state().startup_failed.is_some());
}

#[test]
fn a_crashed_loader_with_nothing_following_is_flagged_without_the_grace() {
    // The grace is for a loader that exited 0 after handing off; a non-zero exit
    // is a crash, so the watch must not wait the grace out. The exit code is only
    // known once the loader has exited, so it is decided when the root empties.
    let rig = Rig::tuned(Box::new(SystemHost), |l| {
        l.handoff_grace = Duration::from_secs(20)
    });
    rig.install(PD2R_1);
    rig.start_up();
    let mut loader = std::process::Command::new("cmd")
        .args(["/c", "ping -n 2 127.0.0.1 >nul & exit 3"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    *rig.host.spawn_pid.lock().unwrap() = Some(loader.id());
    rig.play().unwrap();
    assert_eq!(loader.wait().unwrap().code(), Some(3));
    rig.host.exit_all();
    let begun = std::time::Instant::now();
    rig.state_event();
    assert!(begun.elapsed() < Duration::from_secs(10), "no grace waited");
    assert_eq!(
        rig.state().startup_failed,
        Some(StartupFailed { exit_code: Some(3) })
    );
}

#[test]
fn a_crash_inside_the_startup_window_is_flagged_even_when_the_grace_runs_past_it() {
    // The window runs from the moment the root went empty, not from the end of
    // the grace: with a 3 s grace and a 3 s window, a game gone after about 1 s
    // is flagged (it used to be measured at about 4 s).
    let rig = Rig::tuned(Box::new(SystemHost), |l| {
        l.handoff_grace = Duration::from_secs(3);
        l.startup_window = Duration::from_secs(3);
    });
    rig.install(PD2R_1);
    rig.start_up();
    rig.play().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    rig.host.exit_all();
    rig.state_event();
    assert!(rig.state().startup_failed.is_some());
}

#[test]
fn a_successful_repair_clears_the_startup_failure() {
    // #171: after "The game closed while starting", Repair succeeded but the
    // product kept startup_failed, so the page kept Try again instead of Play.
    let rig = Rig::new();
    rig.install(PD2R_1);
    rig.start_up();
    rig.play().unwrap();
    rig.host.exit_all();
    rig.state_event();
    assert!(rig.state().startup_failed.is_some());
    assert_eq!(rig.run("repair", json!({}))["status"], "succeeded");
    assert_eq!(rig.state().startup_failed, None);
}

#[test]
fn release_notes_list_the_newer_release_then_the_installed_one() {
    let rig = Rig::new();
    let notes = || -> Vec<ReleaseNotes> {
        data(
            rig.invoke("get_release_notes", json!({ "product_id": "pd2r" }))
                .unwrap(),
        )
    };
    assert!(notes().is_empty(), "nothing published (offline)");
    rig.install(PD2R_1);
    let only = notes();
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].release_id, PD2R_1);
    assert_eq!(only[0].status, "alpha");
    assert_eq!(only[0].release_date, "2026-11-01");
    assert!(only[0]
        .notes
        .as_deref()
        .unwrap()
        .contains("Fixture release"));

    rig.publish(PD2R_2);
    let both = notes();
    let ids: Vec<&str> = both.iter().map(|n| n.release_id.as_str()).collect();
    assert_eq!(ids, [PD2R_2, PD2R_1]);
    assert_eq!(both[0].release_date, "2026-11-08");
}

#[test]
fn open_folder_builds_each_path_natively_and_creates_missing_plugins_folders() {
    let rig = Rig::new();
    let open = |product: Value, folder: &str| {
        rig.invoke(
            "open_folder",
            json!({ "product_id": product, "folder": folder }),
        )
        .map(data::<bool>)
    };
    assert_eq!(
        code(open(json!("pd2r"), "settings").unwrap_err()),
        "product_not_installed"
    );
    let root = rig.install(PD2R_1);
    let saved = tempfile::tempdir().unwrap();
    *rig.desktop.saved_games.lock().unwrap() = Some(saved.path().to_owned());

    let saves = saved.path().join(r"Diablo II Resurrected\mods\PD2R");
    assert_eq!(
        code(open(json!("pd2r"), "saves").unwrap_err()),
        "product_not_installed",
        "never played: no saves folder yet"
    );
    fs::create_dir_all(&saves).unwrap();
    assert!(!open(json!("pd2r"), "saves").unwrap());
    assert!(!open(json!("pd2r"), "settings").unwrap());
    let plugins = root.join(r"d2rloader\plugins");
    let _ = fs::remove_dir_all(&plugins);
    assert!(
        open(json!("pd2r"), "product_plugins").unwrap(),
        "reports that it created the folder"
    );
    assert!(plugins.is_dir(), "created under the guarded product root");
    assert!(
        !open(json!("pd2r"), "product_plugins").unwrap(),
        "already there"
    );

    // The D2R root has no disposable sentinel, so the guard refuses to create it.
    let global = rig.game.d2r().join(r"d2rloader\plugins");
    assert_eq!(
        code(open(Value::Null, "global_plugins").unwrap_err()),
        "access_denied"
    );
    assert!(!global.exists());
    fs::write(
        rig.game
            .d2r()
            .join(resurrected_launcher_lib::contract::DISPOSABLE_SENTINEL),
        b"",
    )
    .unwrap();
    assert!(open(Value::Null, "global_plugins").unwrap());
    assert!(global.is_dir());

    assert_eq!(
        *rig.desktop.folders.lock().unwrap(),
        [saves, root.clone(), plugins.clone(), plugins, global]
    );
    assert_eq!(
        code(open(json!("pd2r"), "global_plugins").unwrap_err()),
        "invalid_request"
    );
    assert_eq!(
        code(open(Value::Null, "saves").unwrap_err()),
        "invalid_request"
    );
    let path = json!({ "product_id": "pd2r", "folder": r"C:\Windows" });
    assert!(
        raw_invoke(&rig.webview, "open_folder", path).is_err(),
        "no paths"
    );
}

#[test]
fn saved_diagnostics_are_exactly_the_previewed_text() {
    let rig = Rig::new();
    let preview = |log: bool| {
        rig.invoke("preview_diagnostics", json!({ "include_log": log }))
            .unwrap()
    };
    let save = |log: bool| -> bool {
        data(
            rig.invoke("save_diagnostics", json!({ "include_log": log }))
                .unwrap(),
        )
    };
    let shown = rig.invoke("show_diagnostics_in_folder", json!({}));
    assert_eq!(code(shown.unwrap_err()), "invalid_request");

    let reply = preview(false);
    let text = reply["data"]["text"].as_str().unwrap().to_owned();
    assert!(text.contains("Contract version: 2"));
    assert!(text.contains("Start with Windows: off"));
    assert!(!reply["data"]["never_included"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!save(false), "cancelled dialog");

    let out = tempfile::tempdir().unwrap();
    let file = out.path().join("report.txt");
    *rig.desktop.save_to.lock().unwrap() = Some(file.clone());
    assert!(save(false));
    assert_eq!(fs::read(&file).unwrap(), text.as_bytes());
    rig.invoke("show_diagnostics_in_folder", json!({})).unwrap();
    assert_eq!(*rig.desktop.shown.lock().unwrap(), [file]);
}

/// #186: Copy log returns the redacted log tail; Open logs folder opens the state dir.
#[test]
fn copy_launcher_log_and_open_logs_folder() {
    let rig = Rig::new();
    let empty = rig.invoke("copy_launcher_log", json!({})).unwrap();
    assert_eq!(empty["data"], "");
    fs::write(
        rig._state_dir.path().join("launcher.log"),
        "started\npassword=pw\n",
    )
    .unwrap();
    let copied = rig.invoke("copy_launcher_log", json!({})).unwrap();
    assert_eq!(copied["data"], "started\npassword=<redacted>\n");
    rig.invoke("open_logs_folder", json!({})).unwrap();
    assert_eq!(
        *rig.desktop.folders.lock().unwrap(),
        [rig._state_dir.path().to_owned()]
    );
}

/// #125: a release build has no console, so startup and its failures must reach
/// `launcher.log` in the state dir, where the diagnostics report reads it.
#[test]
fn start_up_writes_the_launcher_log_that_diagnostics_includes() {
    let rig = Rig::new();
    let log = rig._state_dir.path().join("launcher.log");
    assert!(!log.exists());
    rig.start_up();
    let text = fs::read_to_string(&log).unwrap();
    assert!(
        text.contains(&format!(
            "] launcher {} started
",
            env!("CARGO_PKG_VERSION")
        )),
        "{text}"
    );
    // The offline startup refresh fails, and says so.
    assert!(text.contains("] catalog refresh failed: "), "{text}");

    let reply = rig
        .invoke("preview_diagnostics", json!({ "include_log": true }))
        .unwrap();
    let report = reply["data"]["text"].as_str().unwrap();
    let section = report
        .split(
            "[Launcher log]
",
        )
        .nth(1)
        .unwrap();
    assert!(section.contains("started"), "{report}");
}

#[test]
fn start_with_windows_is_only_the_run_value() {
    let rig = Rig::new();
    let get = || -> bool { data(rig.invoke("get_start_with_windows", json!({})).unwrap()) };
    let set = |on: bool| rig.invoke("set_start_with_windows", json!({ "enabled": on }));
    assert!(!get(), "off by default");
    set(true).unwrap();
    let exe = std::env::current_exe().unwrap();
    assert_eq!(
        rig.desktop.run.lock().unwrap().clone().unwrap(),
        format!("\"{}\" --autostart", exe.display())
    );
    assert!(get());
    set(false).unwrap();
    assert_eq!(*rig.desktop.run.lock().unwrap(), None);
    // A value left by a launcher at another path does not start this one.
    *rig.desktop.run.lock().unwrap() = Some(r#""C:\old\launcher.exe" --autostart"#.into());
    assert!(!get());
}

#[test]
fn window_controls_act_on_main_and_quit_waits_for_operations() {
    let rig = Rig::new();
    for cmd in ["minimize_window", "start_window_drag"] {
        rig.invoke(cmd, json!({}))
            .unwrap_or_else(|e| panic!("{cmd}: {e:?}"));
    }
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
    let quit = rig.invoke("quit_launcher", json!({}));
    assert_eq!(code(quit.unwrap_err()), "operation_in_progress");
    assert_eq!(rig.outcome()["status"], "succeeded");
}

/// `AppHandle::exit` only requests the exit, so after Quit nothing may start.
/// (The mock runtime ends the IPC channel on exit, so this drives Quit's gate.)
#[test]
fn nothing_starts_after_quit_is_accepted() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.product_root(&r1);
    rig.start_up();
    let insp = rig.inspect("install", json!({})).unwrap();
    let launcher = rig.webview.state::<Arc<Launcher>>();
    launcher.begin_quit().unwrap();

    let start = rig.start("install", json!({}), &insp.token).unwrap_err();
    assert_eq!(code(start), "operation_in_progress");
    assert_eq!(code(rig.play().unwrap_err()), "operation_in_progress");
    let recover = rig.invoke("recover_product", json!({ "product_id": "pd2r" }));
    assert_eq!(code(recover.unwrap_err()), "operation_in_progress");
    let choose = rig.invoke("choose_game_location", json!({}));
    assert_eq!(code(choose.unwrap_err()), "operation_in_progress");
    assert!(rig.state().installed.is_none());
}

// ---- #126: a product folder the launcher has no record of ----
//
// The UI decides between Adopt, Install and Clean install from the install
// inspection: nothing to add or replace means the files already are the
// recommended release (Adopt); conflicts mean they are some other release or
// edited managed files (Clean install); otherwise Install adds what is missing.

#[test]
fn unmanaged_install_of_the_recommended_release_inspects_as_adoptable() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    rig.game.installed(&r1);
    assert!(rig.state().installed.is_none());

    let insp = rig.inspect("install", json!({})).unwrap();
    assert!(insp.conflicts.is_empty(), "{:?}", insp.conflicts);
    assert_eq!((insp.files_added, insp.files_replaced_or_removed), (0, 0));
    assert_eq!(insp.files_kept, r1.payload.len());

    let outcome = rig.run("adopt", json!({}));
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(rig.state().installed.unwrap().release_id, PD2R_1);
}

#[test]
fn unmanaged_install_of_an_older_release_inspects_as_needing_a_clean_install() {
    // The owner's #126 clone: 0.1.0 files on disk, the feed recommends 0.2.0.
    let rig = Rig::new();
    rig.game.installed(&release(PD2R_1));
    let r2 = rig.publish(PD2R_2);

    let insp = rig.inspect("install", json!({})).unwrap();
    assert!(!insp.conflicts.is_empty(), "the old managed files differ");
    assert!(insp.files_kept > 0, "unchanged files are recognised");
    let refused = rig.run("install", json!({}));
    assert_eq!(refused["status"], "failed", "{refused}");
    assert!(rig.state().installed.is_none());

    let clean = rig.inspect("clean_install", json!({})).unwrap();
    assert!(clean.conflicts.is_empty(), "{:?}", clean.conflicts);
    let outcome = rig.run("clean_install", json!({ "consent_clean_install": true }));
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(
        outcome["report"]["clean_install_backups"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(rig.state().installed.unwrap().release_id, PD2R_2);
    // #156: Open backup opens the newest backup, never a path from the UI.
    let opened = rig.invoke(
        "open_folder",
        json!({ "product_id": "pd2r", "folder": "clean_install_backup" }),
    );
    assert!(!data::<bool>(opened.unwrap()));
    // The report's path may be verbatim (`\?\`); the opened one is the same folder.
    let backup = outcome["report"]["clean_install_backups"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let folders = rig.desktop.folders.lock().unwrap().clone();
    assert_eq!(folders.len(), 1);
    // Compare by file name: CI temp dirs differ between 8.3 short and long forms.
    assert_eq!(
        std::path::Path::new(&backup).file_name(),
        folders[0].file_name(),
        "{backup} vs {folders:?}"
    );
    // Managed files are 2's; a differing shipped default (the old loot filter)
    // is carried forward like a player edit, and its original stays in the backup.
    for (rel, bytes) in &r2.payload {
        if is_managed(&r2, rel) {
            assert_eq!(&fs::read(rig.root(&r2).join(rel)).unwrap(), bytes, "{rel}");
        }
    }
}

fn is_managed(release: &FixtureRelease, rel: &str) -> bool {
    release
        .manifest
        .ownership_of(&RelativePath::new(rel).unwrap())
        == resurrected_launcher_lib::catalog::Ownership::Managed
}

#[test]
fn unmanaged_install_missing_a_file_inspects_as_an_install_that_adds_it() {
    let rig = Rig::new();
    let r1 = rig.publish(PD2R_1);
    let root = rig.game.installed(&r1);
    let (missing, bytes) = r1
        .payload
        .iter()
        .find(|(rel, _)| is_managed(&r1, rel))
        .unwrap();
    fs::remove_file(root.join(missing)).unwrap();

    let insp = rig.inspect("install", json!({})).unwrap();
    assert!(insp.conflicts.is_empty(), "{:?}", insp.conflicts);
    assert_eq!(insp.files_added, 1);
    assert_eq!(insp.files_kept, r1.payload.len() - 1);
    let outcome = rig.run("install", json!({}));
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(&fs::read(root.join(missing)).unwrap(), bytes);
}
