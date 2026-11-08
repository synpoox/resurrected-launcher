//! The gate's rig: a [`Launcher`] mounted in the Tauri mock runtime and driven
//! only through the registered commands, as the UI drives it.
//!
//! Unlike the in-memory fakes elsewhere, [`GateHost`] lists **real** processes
//! (Toolhelp32 through [`SystemGameHost`]) and spawns **real** ones (the
//! production `CreateProcess` path), so a running game is a real
//! [`super::fake_loader`] process. Only PE ProductVersion strings are injected,
//! because the fixture `D2R.exe`/`D2RCore.dll` are fake bytes. Nothing here
//! touches a real D2R copy; [`super::phase_a`] owns that.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use resurrected_launcher_lib::app::{
    self, CatalogState, Desktop, InspectionView, Launcher, ProductState, Release, Releases,
    OUTCOME_EVENT, STATE_EVENT,
};
use resurrected_launcher_lib::catalog::{GameBuild, Platform};
use resurrected_launcher_lib::contract::{
    ErrorCode, LauncherError, ProductId, RelativePath, RunningGame, Versioned, CONTRACT_VERSION,
};
use resurrected_launcher_lib::game::{GameHost, SystemGameHost};
use resurrected_launcher_lib::install::download::Downloader;
use resurrected_launcher_lib::install::preflight::{Host, SystemHost};
use resurrected_launcher_lib::state::{InstalledRelease, Preferences, StateStore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{Listener, Manager, WebviewWindow, WebviewWindowBuilder};

use crate::fake_loader;
use crate::support::{self, DisposableGame, FixtureRelease, FixtureServer};

/// The build label the fake `D2R.exe` reports (the owner's copy's build).
pub const D2R_BUILD: &str = "3.3.93847";
/// The loader version the fake loader files report (the real manifest's pin).
pub const LOADER_VERSION: &str = "1.3.1-beta";
/// Bytes of every rig's fake `D2R.exe`; the published fingerprint is theirs.
pub const D2R_BYTES: &[u8] = b"MZ fake D2R.exe for the slice 3 gate (#61), not a game file";
/// The product every gate test plays.
pub const PRODUCT: &str = "pd2r";
/// Never resolves (`.invalid`), so every catalog refresh fails like offline.
const OFFLINE_FEED: &str = "https://feed.fixtures.invalid/feed.json";

/// (exe, args, cwd) of one spawn.
pub type Spawned = (PathBuf, Vec<String>, PathBuf);

/// Real processes and real spawning; injected ProductVersion strings by file name.
#[derive(Clone)]
pub struct GateHost {
    versions: Arc<Mutex<HashMap<String, String>>>,
    pub spawned: Arc<Mutex<Vec<Spawned>>>,
    /// Phase A: real registry and versions, and every spawn refused.
    read_only: bool,
}

impl Default for GateHost {
    fn default() -> Self {
        let versions = [
            ("D2R.EXE", D2R_BUILD),
            ("D2RLOADER.EXE", LOADER_VERSION),
            ("D2RCORE.DLL", LOADER_VERSION),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()));
        Self {
            versions: Arc::new(Mutex::new(versions.into_iter().collect())),
            spawned: Arc::default(),
            read_only: false,
        }
    }
}

impl GateHost {
    /// The real machine, except that nothing can be started: Battle.net
    /// registry, PE versions and processes are real; `spawn` always fails.
    pub fn read_only() -> Self {
        Self {
            versions: Arc::default(),
            spawned: Arc::default(),
            read_only: true,
        }
    }

    /// Makes `file_name` report `version`.
    pub fn set_version(&self, file_name: &str, version: &str) {
        self.versions
            .lock()
            .unwrap()
            .insert(file_name.to_uppercase(), version.to_owned());
    }

    /// `file_name` reports its real version resource (none for fake bytes).
    pub fn real_version(&self, file_name: &str) {
        self.versions
            .lock()
            .unwrap()
            .remove(&file_name.to_uppercase());
    }

    pub fn spawns(&self) -> Vec<Spawned> {
        self.spawned.lock().unwrap().clone()
    }
}

impl GameHost for GateHost {
    fn battle_net_install_locations(&self) -> Vec<PathBuf> {
        if self.read_only {
            return SystemGameHost.battle_net_install_locations();
        }
        // Fixture tests must not depend on whether this machine has D2R installed.
        Vec::new()
    }

    fn processes(&self) -> std::io::Result<Vec<RunningGame>> {
        SystemGameHost.processes()
    }

    fn product_version(&self, file: &Path) -> std::io::Result<Option<String>> {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().to_uppercase())
            .unwrap_or_default();
        let injected = self.versions.lock().unwrap().get(&name).cloned();
        match injected {
            Some(v) if fs::symlink_metadata(file).is_ok_and(|m| m.is_file()) => Ok(Some(v)),
            _ => SystemGameHost.product_version(file),
        }
    }

    fn spawn(&self, exe: &Path, args: &[String], cwd: &Path) -> std::io::Result<u32> {
        self.spawned
            .lock()
            .unwrap()
            .push((exe.to_owned(), args.to_vec(), cwd.to_owned()));
        if self.read_only {
            return Err(std::io::Error::other("phase A never starts anything"));
        }
        SystemGameHost.spawn(exe, args, cwd)
    }
}

/// The native folder picker and browser, in memory.
#[derive(Clone, Default)]
pub struct GateDesktop {
    pub pick: Arc<Mutex<Option<PathBuf>>>,
}

impl GateDesktop {
    pub fn will_pick(&self, path: &Path) {
        *self.pick.lock().unwrap() = Some(path.to_owned());
    }
}

impl Desktop for GateDesktop {
    fn pick_folder(&self) -> Option<PathBuf> {
        self.pick.lock().unwrap().clone()
    }
    fn open_loader_download_page(&self) -> std::io::Result<()> {
        Ok(())
    }
    fn open_link(&self, _: &str) -> std::io::Result<()> {
        Ok(())
    }
    fn open_folder(&self, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
    fn show_in_folder(&self, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
    fn pick_save_file(&self, _: &str) -> Option<PathBuf> {
        None
    }
    fn saved_games(&self) -> std::io::Result<PathBuf> {
        Err(std::io::Error::other("the gate has no Saved Games"))
    }
    fn run_value(&self) -> std::io::Result<Option<String>> {
        Ok(None)
    }
    fn set_run_value(&self, _: Option<&str>) -> std::io::Result<()> {
        Ok(())
    }
}

/// Published releases; the last one pushed per product is the recommended one.
#[derive(Clone, Default)]
pub struct GateReleases(pub Arc<Mutex<Vec<Release>>>);

impl Releases for GateReleases {
    fn recommended(&self, _: &mut StateStore, p: &ProductId) -> Result<Release, LauncherError> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|r| &r.manifest.product_id == p)
            .cloned()
            .ok_or_else(|| LauncherError::new(ErrorCode::MetadataInvalid, "nothing published"))
    }

    fn installed(
        &self,
        _: &mut StateStore,
        record: &InstalledRelease,
    ) -> Result<Release, LauncherError> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.manifest.release_id == record.release_id)
            .cloned()
            .ok_or_else(|| LauncherError::new(ErrorCode::MetadataInvalid, "not published"))
    }
}

/// The real-layout fixture (`mods/PD2Resurrected`), or a copy with another
/// `release_id` and the same archive: an update whose files are all unchanged.
pub fn real_layout(release_id: &str) -> FixtureRelease {
    let mut r = support::release(support::PD2R_REAL_LAYOUT);
    if r.manifest.release_id != release_id {
        r.manifest.release_id = release_id.to_owned();
        r.manifest.display_version = format!("real layout ({release_id})");
        r.manifest_json = format!("derived manifest {release_id}").into_bytes();
    }
    r
}

pub const REAL_1: &str = "pd2r-real-layout-1";
pub const REAL_1B: &str = "pd2r-real-layout-1b";

/// The temp dirs a launcher and its restarts share: a disposable D2R
/// installation (fake game files, the fake loader as `D2RLoader.exe`) and State.
pub struct Dirs {
    pub game: DisposableGame,
    pub state: tempfile::TempDir,
}

impl Dirs {
    pub fn new() -> Arc<Self> {
        let game = DisposableGame::new();
        fake_game_files(game.d2r());
        Arc::new(Self {
            game,
            state: tempfile::tempdir().unwrap(),
        })
    }
}

/// `D2R.exe` ([`D2R_BYTES`]), `D2RLoader.exe` (the fake loader) and `D2RCore.dll`.
pub fn fake_game_files(d2r: &Path) {
    fs::create_dir_all(d2r).unwrap();
    fs::write(d2r.join("D2R.exe"), D2R_BYTES).unwrap();
    fake_loader::install(d2r, "D2RLoader.exe");
    fs::write(d2r.join("D2RCore.dll"), b"fake D2RCore.dll").unwrap();
}

pub struct Rig {
    pub d2r: PathBuf,
    pub state_dir: PathBuf,
    pub dirs: Option<Arc<Dirs>>,
    pub server: FixtureServer,
    pub releases: GateReleases,
    pub webview: WebviewWindow<MockRuntime>,
    pub outcomes: Receiver<Value>,
    pub states: Receiver<Value>,
    pub host: GateHost,
    pub desktop: GateDesktop,
    /// How long [`Self::outcome`] waits: longer online, where a real archive
    /// downloads and a real installation is hashed.
    pub outcome_wait: Duration,
}

impl Rig {
    /// A launcher on fresh dirs, with the D2R location already chosen.
    pub fn new() -> Self {
        Self::on(Dirs::new(), Box::new(SystemHost))
    }

    /// A launcher on `dirs` whose install preflight uses `host`.
    pub fn on(dirs: Arc<Dirs>, host: Box<dyn Host>) -> Self {
        let mut rig = Self::attach(dirs.game.d2r(), dirs.state.path(), host);
        rig.dirs = Some(dirs);
        rig
    }

    /// A new launcher process on the same dirs: same State, same installation.
    /// Releases are not carried over; publish them again.
    pub fn restart(&self) -> Self {
        let dirs = self.dirs.clone().expect("owned dirs");
        Self::on(dirs, Box::new(SystemHost))
    }

    /// A launcher on dirs owned elsewhere (the child launcher process).
    pub fn attach(d2r: &Path, state_dir: &Path, host: Box<dyn Host>) -> Self {
        Self::attach_with(d2r, state_dir, host, GateHost::default(), true)
    }

    /// [`Self::attach`] with `game` as the game host; with `remember` false no
    /// D2R location is selected yet, as on a first start.
    pub fn attach_with(
        d2r: &Path,
        state_dir: &Path,
        host: Box<dyn Host>,
        game: GateHost,
        remember: bool,
    ) -> Self {
        let server = FixtureServer::start();
        let downloader = Downloader::with_tls(server.tls_config());
        Self::attach_full(d2r, state_dir, host, game, remember, server, downloader)
    }

    /// Phase B on a real copy: [`Self::attach_with`] (location remembered), but
    /// downloading over the internet with the production [`Downloader`], so a
    /// published release's real archive URL is fetched and verified.
    pub fn attach_online(d2r: &Path, state_dir: &Path, game: GateHost) -> Self {
        let server = FixtureServer::start();
        let mut rig = Self::attach_full(
            d2r,
            state_dir,
            Box::new(SystemHost),
            game,
            true,
            server,
            Downloader::new(),
        );
        rig.outcome_wait = Duration::from_secs(30 * 60);
        rig
    }

    fn attach_full(
        d2r: &Path,
        state_dir: &Path,
        host: Box<dyn Host>,
        game: GateHost,
        remember: bool,
        server: FixtureServer,
        downloader: Downloader,
    ) -> Self {
        let releases = GateReleases::default();
        let desktop = GateDesktop::default();
        let mut state = StateStore::open(state_dir);
        if remember {
            state
                .set_preferences(Preferences {
                    d2r_location: Some(d2r.to_owned()),
                    ..Preferences::default()
                })
                .unwrap();
        }
        let mut launcher = Launcher::new(
            state,
            Box::new(releases.clone()),
            downloader,
            support::seven_zip(),
            host,
            Box::new(game.clone()),
            Box::new(desktop.clone()),
        );
        launcher.feed_url = OFFLINE_FEED.into();
        let (webview, outcomes, states) = mount(launcher);
        Self {
            d2r: d2r.to_owned(),
            state_dir: state_dir.to_owned(),
            dirs: None,
            server,
            releases,
            webview,
            outcomes,
            states,
            host: game,
            desktop,
            outcome_wait: Duration::from_secs(120),
        }
    }

    pub fn launcher(&self) -> Arc<Launcher> {
        Arc::clone(self.webview.state::<Arc<Launcher>>().inner())
    }

    /// Publishes `release` as recommended, its archive on the fixture server and
    /// its build fingerprint that of the rig's `D2R.exe` as it is now.
    pub fn publish(&self, release: &FixtureRelease) -> Release {
        let exe = fs::read(self.d2r.join("D2R.exe")).unwrap_or_else(|_| D2R_BYTES.to_vec());
        let mut manifest = release.manifest.clone();
        manifest.archive = self.server.serve(release);
        manifest.compatibility.game_builds = vec![GameBuild {
            build: D2R_BUILD.into(),
            platform: Platform::WindowsX64,
            executable: RelativePath::new("D2R.exe").unwrap(),
            size: exe.len() as u64,
            sha256: support::sha256(&exe),
        }];
        let published = Release {
            manifest,
            sha256: support::sha256(&release.manifest_json),
        };
        self.releases.0.lock().unwrap().push(published.clone());
        published
    }

    /// The product root of the real layout under this rig's D2R.
    pub fn root(&self) -> PathBuf {
        self.d2r.join("mods").join("PD2Resurrected")
    }

    /// The real-layout files in place with the sentinel, adopted and Ready.
    pub fn adopted(&self) -> FixtureRelease {
        let release = real_layout(REAL_1);
        self.dirs
            .as_ref()
            .expect("owned dirs")
            .game
            .installed(&release);
        self.publish(&release);
        self.start_up();
        assert_eq!(self.run("adopt", json!({}))["status"], "succeeded");
        release
    }

    /// Startup recovery, as `run` does on its worker thread. Waits until the
    /// (offline) startup refresh ended, then drops the events so far.
    pub fn start_up(&self) {
        self.launcher().start_up(self.webview.app_handle());
        wait_until("the startup refresh", || {
            matches!(self.state().catalog, CatalogState::Failed { .. })
        });
        while self.states.recv_timeout(Duration::from_millis(200)).is_ok() {}
    }

    pub fn invoke(&self, cmd: &str, body: Value) -> Result<Value, LauncherError> {
        invoke(&self.webview, cmd, body)
    }

    pub fn state(&self) -> ProductState {
        data(
            self.invoke("get_product_state", json!({ "product_id": PRODUCT }))
                .unwrap(),
        )
    }

    pub fn play(&self) -> Result<RunningGame, LauncherError> {
        self.invoke("play_product", json!({ "product_id": PRODUCT }))
            .map(data)
    }

    pub fn inspect(&self, kind: &str, extra: Value) -> Result<InspectionView, LauncherError> {
        self.invoke(
            "inspect_operation",
            json!({ "request": request(kind, extra) }),
        )
        .map(data)
    }

    pub fn start(&self, kind: &str, extra: Value, token: &str) -> Result<Value, LauncherError> {
        self.invoke(
            "start_operation",
            json!({ "request": request(kind, extra), "inspection_token": token }),
        )
    }

    /// Inspects, starts with that inspection's token and returns the outcome.
    pub fn run(&self, kind: &str, extra: Value) -> Value {
        let insp = self.inspect(kind, extra.clone()).unwrap();
        self.start(kind, extra, &insp.token).unwrap();
        self.outcome()
    }

    pub fn outcome(&self) -> Value {
        let v = self.outcomes.recv_timeout(self.outcome_wait).unwrap();
        assert_eq!(v["contract_version"], CONTRACT_VERSION);
        v["data"].clone()
    }

    /// Waits for a `product-state-changed` event.
    pub fn state_event(&self) {
        let v = self.states.recv_timeout(Duration::from_secs(30)).unwrap();
        assert_eq!(v["contract_version"], CONTRACT_VERSION);
    }
}

fn mount(launcher: Launcher) -> (WebviewWindow<MockRuntime>, Receiver<Value>, Receiver<Value>) {
    let app = mock_builder()
        .invoke_handler(app::invoke_handler())
        .manage(Arc::new(launcher))
        .build(tauri::generate_context!())
        .unwrap();
    let (out_tx, outcomes) = channel();
    let (state_tx, states) = channel();
    app.listen_any(STATE_EVENT, move |e| {
        let _ = state_tx.send(serde_json::from_str(e.payload()).unwrap());
    });
    app.listen_any(OUTCOME_EVENT, move |e| {
        let _ = out_tx.send(serde_json::from_str(e.payload()).unwrap());
    });
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    // The app must outlive the test; the webview keeps a handle, not ownership.
    std::mem::forget(app);
    (webview, outcomes, states)
}

pub fn invoke(
    webview: &WebviewWindow<MockRuntime>,
    cmd: &str,
    body: Value,
) -> Result<Value, LauncherError> {
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
    .map_err(|e: Value| {
        serde_json::from_value(e.clone()).unwrap_or_else(|_| panic!("not a LauncherError: {e}"))
    })
}

pub fn request(kind: &str, extra: Value) -> Value {
    let mut data = json!({ "product_id": PRODUCT, "kind": kind });
    data.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    json!({ "contract_version": CONTRACT_VERSION, "data": data })
}

pub fn data<T: serde::de::DeserializeOwned>(reply: Value) -> T {
    let v: Versioned<T> = serde_json::from_value(reply).unwrap();
    v.into_current().unwrap()
}

/// The wire name of an error's code, e.g. `game_running`.
pub fn code(e: LauncherError) -> String {
    serde_json::to_value(e.code)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

pub fn wait_until(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ok() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Every file and directory under `dir`: (size, SHA-256, modified) for files,
/// `None` for directories. Links are listed, not followed.
pub type Tree = BTreeMap<String, Option<(u64, String, std::time::SystemTime)>>;

pub fn tree(dir: &Path) -> Tree {
    fn go(dir: &Path, prefix: &str, out: &mut Tree) {
        let mut entries: Vec<_> = fs::read_dir(dir).unwrap().map(|e| e.unwrap()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let rel = format!("{prefix}{}", e.file_name().to_string_lossy());
            let meta = fs::symlink_metadata(e.path()).unwrap();
            if meta.is_dir() && !meta.file_type().is_symlink() {
                out.insert(format!("{rel}/"), None);
                go(&e.path(), &format!("{rel}/"), out);
            } else {
                let bytes = fs::read(e.path()).unwrap_or_default();
                let hash = Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                out.insert(rel, Some((meta.len(), hash, meta.modified().unwrap())));
            }
        }
    }
    let mut out = Tree::new();
    if dir.exists() {
        go(dir, "", &mut out);
    }
    out
}

/// Skipped checks are loud: visible with `--show-output` and in CI logs.
pub fn skip(why: &str) {
    eprintln!(
        "SKIPPED {}: {why}",
        std::thread::current().name().unwrap_or("?")
    );
}
