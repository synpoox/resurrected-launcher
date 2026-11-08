//! Launcher self-update (#88) through `tauri-plugin-updater`, against the local
//! HTTPS fixture server.
//!
//! Every test signs with an ephemeral minisign key generated in a temp dir during
//! the test. No test uses the release key, and no test key is committed. The test
//! app's updater config (key, endpoint, the fixture server's self-signed cert) is
//! set on its context, the same place the real one comes from (`tauri.conf.json`).
//! The installer never runs: a recording [`UpdateHost`] stands in for it.
//! Nothing here touches a real D2R installation or the real launcher's data.
#![cfg(windows)]

mod support;

use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use resurrected_launcher_lib::app::{
    self, Desktop, Launcher, Release, Releases, SelfUpdateStatus, SelfUpdater, UpdateHost,
    OUTCOME_EVENT, RELEASE_PAGE, SELF_UPDATE_EVENT,
};
use resurrected_launcher_lib::catalog::{GameBuild, Platform};
use resurrected_launcher_lib::contract::{
    ErrorCode, LauncherError, ProductId, RelativePath, RunningGame, CONTRACT_VERSION,
};
use resurrected_launcher_lib::game::{GameHost, SystemGameHost};
use resurrected_launcher_lib::install::download::Downloader;
use resurrected_launcher_lib::install::preflight::SystemHost;
use resurrected_launcher_lib::state::{InstalledRelease, Preferences, StateStore};
use serde_json::{json, Value};
use support::{DisposableGame, FixtureServer, Reply};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, WebviewWindow, WebviewWindowBuilder};

const NEXT: &str = "99.0.0";
const LATEST: &str = "/launcher/latest.json";
const SETUP: &str = "/launcher/setup.exe";
const INSTALLER: &[u8] = b"fake NSIS installer for #88; never runs";

// ---- ephemeral signing keys ----

/// A minisign key pair written to a fresh temp dir, like `tauri signer generate`.
struct TestKey {
    pk: minisign::PublicKey,
    sk: minisign::SecretKey,
    _dir: tempfile::TempDir,
}

fn b64(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text)
}

impl TestKey {
    fn generate() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let pk_file = fs::File::create(dir.path().join("test.key.pub")).unwrap();
        let sk_file = fs::File::create(dir.path().join("test.key")).unwrap();
        let pair =
            minisign::KeyPair::generate_and_write_unencrypted_keypair(pk_file, sk_file).unwrap();
        Self {
            pk: pair.pk,
            sk: pair.sk,
            _dir: dir,
        }
    }

    /// The `plugins.updater.pubkey` value: base64 of the `.pub` file.
    fn pubkey(&self) -> String {
        b64(&self.pk.to_box().unwrap().to_string())
    }

    /// The `.sig` contents Tauri writes, base64 encoded, signed for `version`.
    fn sign(&self, bytes: &[u8], version: &str) -> String {
        let trusted = format!("timestamp:0\tfile:setup.exe\tversion:{version}");
        let sig = minisign::sign(
            Some(&self.pk),
            &self.sk,
            Cursor::new(bytes),
            Some(&trusted),
            None,
        )
        .unwrap();
        b64(&sig.to_string())
    }
}

fn latest_json(server: &FixtureServer, signature: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "version": NEXT,
        "notes": format!("Launcher {NEXT}"),
        "pub_date": "2026-11-01T00:00:00Z",
        "platforms": {
            "windows-x86_64": { "signature": signature, "url": server.url(SETUP) }
        }
    }))
    .unwrap()
}

/// Serves `installer` with `signature` as the latest release.
fn publish(server: &FixtureServer, installer: &[u8], signature: &str) {
    server.route(LATEST, Reply::Body(latest_json(server, signature)));
    server.route(SETUP, Reply::Body(installer.to_vec()));
}

// ---- fakes ----

/// `(version, installer bytes)` for each install the launcher started.
type Installs = Vec<(String, Vec<u8>)>;

#[derive(Default, Clone)]
struct Recorder {
    installs: Arc<Mutex<Installs>>,
    pages: Arc<Mutex<Vec<String>>>,
}

impl UpdateHost for Recorder {
    fn install(
        &self,
        update: &tauri_plugin_updater::Update,
        bytes: &[u8],
    ) -> Result<(), tauri_plugin_updater::Error> {
        self.installs
            .lock()
            .unwrap()
            .push((update.version.clone(), bytes.to_vec()));
        Ok(())
    }

    fn open_page(&self, url: &str) -> io::Result<()> {
        self.pages.lock().unwrap().push(url.to_owned());
        Ok(())
    }
}

impl Recorder {
    fn installs(&self) -> Installs {
        self.installs.lock().unwrap().clone()
    }
}

/// The real process list, plus any game a test says is running.
#[derive(Default, Clone)]
struct Game(Arc<Mutex<Vec<RunningGame>>>);

impl GameHost for Game {
    fn battle_net_install_locations(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn processes(&self) -> io::Result<Vec<RunningGame>> {
        let mut all = SystemGameHost.processes()?;
        all.extend(self.0.lock().unwrap().iter().cloned());
        Ok(all)
    }
    fn product_version(&self, file: &Path) -> io::Result<Option<String>> {
        SystemGameHost.product_version(file)
    }
    fn spawn(&self, _: &Path, _: &[String], _: &Path) -> io::Result<u32> {
        Err(io::Error::other("no spawning in self-update tests"))
    }
}

struct NoDesktop;

impl Desktop for NoDesktop {
    fn pick_folder(&self) -> Option<PathBuf> {
        None
    }
    fn open_loader_download_page(&self) -> io::Result<()> {
        Ok(())
    }
    fn open_link(&self, _: &str) -> io::Result<()> {
        Ok(())
    }
    fn open_folder(&self, _: &Path) -> io::Result<()> {
        Ok(())
    }
    fn show_in_folder(&self, _: &Path) -> io::Result<()> {
        Ok(())
    }
    fn pick_save_file(&self, _: &str) -> Option<PathBuf> {
        None
    }
    fn saved_games(&self) -> io::Result<PathBuf> {
        Err(io::Error::other("no Saved Games in tests"))
    }
    fn run_value(&self) -> io::Result<Option<String>> {
        Ok(None)
    }
    fn set_run_value(&self, _: Option<&str>) -> io::Result<()> {
        Ok(())
    }
}

/// The one recommended (and installed) release.
struct Published(Release);

impl Releases for Published {
    fn recommended(&self, _: &mut StateStore, _: &ProductId) -> Result<Release, LauncherError> {
        Ok(self.0.clone())
    }
    fn installed(
        &self,
        _: &mut StateStore,
        _: &InstalledRelease,
    ) -> Result<Release, LauncherError> {
        Ok(self.0.clone())
    }
}

// ---- the mock app ----

struct Rig {
    app: App<MockRuntime>,
    webview: WebviewWindow<MockRuntime>,
    updater: Arc<SelfUpdater>,
    launcher: Arc<Launcher>,
    host: Recorder,
    game: Game,
    phases: Receiver<Value>,
    /// `<app local data>\launcher-updates`.
    dir: PathBuf,
    d2r: DisposableGame,
    _data: tempfile::TempDir,
}

struct Setup {
    releases: Box<dyn Releases>,
    downloader: Downloader,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            releases: Box::new(app::CachedReleases),
            downloader: Downloader::new(),
        }
    }
}

impl Rig {
    fn new(server: &FixtureServer, key: &TestKey) -> Self {
        Self::with(server, key, DisposableGame::new(), Setup::default())
    }

    fn with(server: &FixtureServer, key: &TestKey, d2r: DisposableGame, setup: Setup) -> Self {
        fs::create_dir_all(d2r.d2r()).unwrap();
        let data = tempfile::tempdir().unwrap();
        let mut state = StateStore::open(&data.path().join("state"));
        state
            .set_preferences(Preferences {
                d2r_location: Some(d2r.d2r().to_owned()),
                ..Preferences::default()
            })
            .unwrap();
        let game = Game::default();
        let mut launcher = Launcher::new(
            state,
            setup.releases,
            setup.downloader,
            support::seven_zip(),
            Box::new(SystemHost),
            Box::new(game.clone()),
            Box::new(NoDesktop),
        );
        launcher.feed_url = "https://feed.fixtures.invalid/feed.json".into();
        let launcher = Arc::new(launcher);
        let dir = data.path().join(app::UPDATE_DIR);
        let host = Recorder::default();
        let updater = Arc::new(SelfUpdater::new(
            dir.clone(),
            env!("CARGO_PKG_VERSION").into(),
            Box::new(host.clone()),
        ));

        let mut context = tauri::generate_context!();
        context.config_mut().plugins.0.insert(
            "updater".into(),
            json!({
                "pubkey": key.pubkey(),
                "endpoints": [server.url(LATEST)],
                // The fixture server's certificate is self-signed.
                "dangerousAcceptInvalidCerts": true,
            }),
        );
        let app = mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .invoke_handler(app::invoke_handler())
            .manage(Arc::clone(&launcher))
            .manage(Arc::clone(&updater))
            .build(context)
            .unwrap();
        let (tx, phases) = channel();
        app.listen_any(SELF_UPDATE_EVENT, move |e| {
            let _ = tx.send(serde_json::from_str(e.payload()).unwrap());
        });
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        Self {
            app,
            webview,
            updater,
            launcher,
            host,
            game,
            phases,
            dir,
            d2r,
            _data: data,
        }
    }

    fn check(&self, auto: bool) -> Result<bool, LauncherError> {
        tauri::async_runtime::block_on(self.updater.check(self.app.handle(), auto))
    }

    fn download(&self) -> Result<(), LauncherError> {
        tauri::async_runtime::block_on(self.updater.download(self.app.handle()))
    }

    fn restart(&self) -> Result<(), LauncherError> {
        self.updater
            .restart_to_update(self.app.handle(), &self.launcher)
    }

    fn status(&self) -> SelfUpdateStatus {
        self.updater.status()
    }

    /// Checks (automatic downloads off) and downloads to Ready.
    fn ready(&self) {
        assert!(!self.check(false).unwrap());
        self.download().unwrap();
        assert!(matches!(self.status(), SelfUpdateStatus::Ready { .. }));
    }

    /// Files under the update dir.
    fn kept(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut names: Vec<_> = entries
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn invoke(&self, cmd: &str, body: Value) -> Result<Value, Value> {
        get_ipc_response(
            &self.webview,
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
        .map(|b| b.deserialize::<Value>().unwrap())
    }

    /// Phase events until (and including) the first whose phase is `phase`.
    fn phases_until(&self, phase: &str) -> Vec<Value> {
        let mut seen = Vec::new();
        loop {
            let event = self
                .phases
                .recv_timeout(Duration::from_secs(30))
                .unwrap_or_else(|_| panic!("no {phase} event; saw {seen:?}"));
            assert_eq!(event["contract_version"], CONTRACT_VERSION);
            let done = event["data"]["phase"] == phase;
            seen.push(event["data"].clone());
            if done {
                return seen;
            }
        }
    }
}

fn current() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

fn ready_status() -> SelfUpdateStatus {
    SelfUpdateStatus::Ready {
        current: current(),
        next: NEXT.into(),
    }
}

fn rejected_status() -> SelfUpdateStatus {
    SelfUpdateStatus::SignatureRejected {
        current: current(),
        next: NEXT.into(),
    }
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out.sort();
    out
}

// ---- trust: what tauri.conf.json carries ----

/// The key posted on #86 and the endpoint decided there, and nothing that weakens them.
#[test]
fn tauri_conf_trusts_only_the_key_and_endpoint_from_86() {
    let text = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json")).unwrap();
    let conf: Value = serde_json::from_str(&text).unwrap();
    let updater = &conf["plugins"]["updater"];
    assert_eq!(
        updater["pubkey"],
        "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IENENDdDN0NBNEY0MDc0RDMKUldUVGRFQlB5c2RIemN1WTd1UFJ5ZFhTQ1RSZ3ZJRWJ6R24wOXpEZWo3emtIcnR4WGxiOGQxYmUK"
    );
    assert_eq!(
        updater["endpoints"],
        json!([
            "https://github.com/synpoox/resurrected-launcher/releases/latest/download/latest.json"
        ])
    );
    let keys: Vec<&str> = updater
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["endpoints", "pubkey", "windows"], "{updater}");
    assert_eq!(updater["windows"], json!({ "installMode": "passive" }));
    assert_eq!(conf["bundle"]["createUpdaterArtifacts"], true);
    // The release workflow no longer needs its temporary override.
    let release = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../.github/workflows/release.yml"
    ))
    .unwrap();
    assert!(
        !release.contains("--config"),
        "release.yml still overrides the config"
    );
    assert!(
        !release.contains("VITE_SELF_UPDATE"),
        "self-update must stay off in release builds"
    );
}

/// The real config starts the plugin, and its updater uses exactly that config.
#[test]
fn the_shipped_updater_config_starts_the_plugin() {
    use tauri_plugin_updater::UpdaterExt;
    let app = mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(tauri::generate_context!())
        .unwrap();
    app.updater().expect("the shipped updater config is valid");
}

/// The plugin's own commands are never granted to the webview: only the narrow
/// launcher commands reach the updater.
#[test]
fn updater_plugin_commands_are_refused_by_the_acl() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    let rig = Rig::new(&server, &key);
    for cmd in [
        "plugin:updater|check",
        "plugin:updater|download",
        "plugin:updater|install",
        "plugin:updater|download_and_install",
    ] {
        let err = rig.invoke(cmd, json!({})).unwrap_err();
        assert!(err.to_string().contains("not allowed"), "{cmd}: {err}");
    }
}

// ---- the happy path ----

#[test]
fn a_signed_update_downloads_into_app_data_and_installs_on_restart() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    publish(&server, INSTALLER, &key.sign(INSTALLER, NEXT));
    let rig = Rig::new(&server, &key);
    let d2r_before = files_under(rig.d2r.d2r());

    assert!(!rig.check(false).unwrap());
    assert_eq!(
        rig.status(),
        SelfUpdateStatus::Available {
            current: current(),
            next: NEXT.into()
        }
    );
    assert!(rig.host.installs().is_empty(), "nothing installs on check");

    rig.download().unwrap();
    let seen = rig.phases_until("ready");
    assert!(seen.iter().any(|s| s["phase"] == "downloading"), "{seen:?}");
    assert_eq!(rig.status(), ready_status());
    // Kept in the launcher's app data, and nothing written under D2R.
    let kept = rig.dir.join(format!("launcher-{NEXT}-setup.exe"));
    assert_eq!(fs::read(&kept).unwrap(), INSTALLER);
    assert_eq!(rig.kept(), [format!("launcher-{NEXT}-setup.exe")]);
    assert_eq!(files_under(rig.d2r.d2r()), d2r_before);
    assert!(
        rig.host.installs().is_empty(),
        "nothing installs on download"
    );

    rig.restart().unwrap();
    assert_eq!(rig.host.installs(), [(NEXT.to_owned(), INSTALLER.to_vec())]);
    assert_eq!(
        rig.status(),
        SelfUpdateStatus::Restarting {
            current: current(),
            next: NEXT.into()
        }
    );
    // No operation may start while the installer takes over.
    assert!(rig.launcher.begin_quit().is_err());

    // The next launcher, now version NEXT, reports Updated and drops the spent installer.
    let after = SelfUpdater::new(rig.dir.clone(), NEXT.into(), Box::new(Recorder::default()));
    assert_eq!(
        after.status(),
        SelfUpdateStatus::Updated {
            current: NEXT.into()
        }
    );
    assert!(!kept.exists());
}

/// A restart that did not install (the installer was cancelled) reports up to date.
#[test]
fn a_restart_that_did_not_install_is_not_reported_as_updated() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    publish(&server, INSTALLER, &key.sign(INSTALLER, NEXT));
    let rig = Rig::new(&server, &key);
    rig.ready();
    rig.restart().unwrap();
    let after = SelfUpdater::new(rig.dir.clone(), current(), Box::new(Recorder::default()));
    assert_eq!(
        after.status(),
        SelfUpdateStatus::UpToDate { current: current() }
    );
}

#[test]
fn nothing_newer_is_up_to_date() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    let mut body: Value = serde_json::from_slice(&latest_json(&server, "")).unwrap();
    body["version"] = json!(current());
    server.route(LATEST, Reply::Body(serde_json::to_vec(&body).unwrap()));
    let rig = Rig::new(&server, &key);
    assert!(!rig.check(true).unwrap());
    assert_eq!(
        rig.status(),
        SelfUpdateStatus::UpToDate { current: current() }
    );
    assert!(server.hits().iter().all(|h| h != SETUP));
}

/// Over IPC: with automatic downloads on, Check finds the update and downloads it
/// to Ready in the background; the preference round-trips through Preferences.
#[test]
fn check_over_ipc_downloads_automatically_when_the_preference_is_on() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    publish(&server, INSTALLER, &key.sign(INSTALLER, NEXT));
    let rig = Rig::new(&server, &key);
    let auto = rig.invoke("get_launcher_auto_download", json!({})).unwrap();
    assert_eq!(
        auto,
        json!({ "contract_version": CONTRACT_VERSION, "data": true })
    );

    rig.invoke("check_for_launcher_update", json!({})).unwrap();
    rig.phases_until("ready");
    let status = rig.invoke("get_launcher_update_status", json!({})).unwrap();
    assert_eq!(
        status["data"],
        json!({ "phase": "ready", "current": current(), "next": NEXT })
    );

    rig.invoke("set_launcher_auto_download", json!({ "enabled": false }))
        .unwrap();
    let auto = rig.invoke("get_launcher_auto_download", json!({})).unwrap();
    assert_eq!(auto["data"], false);
    assert!(
        !rig.launcher
            .state
            .lock()
            .unwrap()
            .preferences()
            .download_launcher_updates_automatically
    );

    rig.invoke("open_launcher_whats_new", json!({})).unwrap();
    assert_eq!(
        *rig.host.pages.lock().unwrap(),
        [format!("{RELEASE_PAGE}{NEXT}")]
    );
    assert!(rig.host.installs().is_empty());
}

#[test]
fn check_over_ipc_with_automatic_downloads_off_only_announces() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    publish(&server, INSTALLER, &key.sign(INSTALLER, NEXT));
    let rig = Rig::new(&server, &key);
    rig.invoke("set_launcher_auto_download", json!({ "enabled": false }))
        .unwrap();
    rig.invoke("check_for_launcher_update", json!({})).unwrap();
    rig.phases_until("available");
    assert!(
        server.hits().iter().all(|h| h != SETUP),
        "nothing downloads"
    );

    rig.invoke("download_launcher_update", json!({})).unwrap();
    rig.phases_until("ready");
}

// ---- signature rejected ----

/// Bad signature, tampered installer, unsigned artifact, wrong key, and a signature
/// for another version: each ends in signature rejected, with nothing kept or installed.
#[test]
fn every_bad_signature_is_rejected_and_nothing_installs() {
    let key = TestKey::generate();
    let other = TestKey::generate();
    let good = key.sign(INSTALLER, NEXT);
    let mut flipped = base64::engine::general_purpose::STANDARD
        .decode(&good)
        .unwrap();
    // Corrupt one character of the signature line itself.
    let at = String::from_utf8_lossy(&flipped).find("\n").unwrap() + 5;
    flipped[at] = if flipped[at] == b'A' { b'B' } else { b'A' };
    let corrupt = base64::engine::general_purpose::STANDARD.encode(&flipped);
    let mut tampered = INSTALLER.to_vec();
    tampered[0] ^= 0xff;

    let cases: [(&str, &[u8], String); 6] = [
        ("bad signature", INSTALLER, corrupt),
        (
            "not a signature",
            INSTALLER,
            b64("not a minisign signature"),
        ),
        ("tampered installer", &tampered, good.clone()),
        ("unsigned artifact", INSTALLER, String::new()),
        ("wrong key", INSTALLER, other.sign(INSTALLER, NEXT)),
        (
            "signed for another version",
            INSTALLER,
            key.sign(INSTALLER, "98.0.0"),
        ),
    ];
    for (case, installer, signature) in cases {
        let server = FixtureServer::start();
        publish(&server, installer, &signature);
        let rig = Rig::new(&server, &key);
        assert!(!rig.check(false).unwrap(), "{case}");
        let err = rig.download().unwrap_err();
        assert_eq!(err.code, ErrorCode::VerificationFailed, "{case}: {err:?}");
        assert_eq!(rig.status(), rejected_status(), "{case}");
        assert_eq!(
            rig.phases_until("signature_rejected").last().unwrap()["phase"],
            "signature_rejected"
        );
        assert!(rig.kept().is_empty(), "{case}: kept {:?}", rig.kept());
        let refused = rig.restart().unwrap_err();
        assert_eq!(refused.code, ErrorCode::InvalidRequest, "{case}");
        assert!(rig.host.installs().is_empty(), "{case}: installed");
        assert!(
            rig.launcher.begin_quit().is_ok(),
            "{case}: quit stays possible"
        );
    }
}

/// An installer changed on disk after it was verified is caught before it runs.
#[test]
fn a_kept_installer_tampered_after_download_is_rejected_at_restart() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    publish(&server, INSTALLER, &key.sign(INSTALLER, NEXT));
    let rig = Rig::new(&server, &key);
    rig.ready();
    let kept = rig.dir.join(format!("launcher-{NEXT}-setup.exe"));
    fs::write(&kept, b"swapped after verification").unwrap();

    let err = rig.restart().unwrap_err();
    assert_eq!(err.code, ErrorCode::VerificationFailed);
    assert_eq!(rig.status(), rejected_status());
    assert!(rig.host.installs().is_empty());
    assert!(!kept.exists(), "the tampered file is deleted");
    assert!(
        rig.launcher.begin_quit().is_ok(),
        "the restart gate is released"
    );
}

/// A verified installer kept from an earlier download is reused after checking it again.
#[test]
fn a_kept_installer_is_verified_again_and_reused() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    publish(&server, INSTALLER, &key.sign(INSTALLER, NEXT));
    let rig = Rig::new(&server, &key);
    rig.ready();
    let downloads = || server.hits().iter().filter(|h| *h == SETUP).count();
    assert_eq!(downloads(), 1);

    let again = Rig::with(&server, &key, DisposableGame::new(), Setup::default());
    fs::create_dir_all(&again.dir).unwrap();
    fs::copy(
        rig.dir.join(format!("launcher-{NEXT}-setup.exe")),
        again.dir.join(format!("launcher-{NEXT}-setup.exe")),
    )
    .unwrap();
    again.ready();
    assert_eq!(downloads(), 1, "reused without downloading");
}

// ---- download failures ----

#[test]
fn a_failed_download_keeps_the_current_launcher_and_can_be_retried() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    publish(&server, INSTALLER, &key.sign(INSTALLER, NEXT));
    server.once(SETUP, Reply::Status(500));
    let rig = Rig::new(&server, &key);
    assert!(!rig.check(false).unwrap());
    let err = rig.download().unwrap_err();
    assert_eq!(err.code, ErrorCode::DownloadFailed);
    assert_eq!(
        rig.status(),
        SelfUpdateStatus::DownloadFailed {
            current: current(),
            next: NEXT.into()
        }
    );
    assert!(rig.kept().is_empty());
    // Try again.
    rig.download().unwrap();
    assert_eq!(rig.status(), ready_status());
}

#[test]
fn an_unreachable_endpoint_fails_the_check_and_changes_nothing() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    server.route(LATEST, Reply::Status(503));
    let rig = Rig::new(&server, &key);
    let err = rig.check(true).unwrap_err();
    assert_eq!(err.code, ErrorCode::DownloadFailed);
    assert_eq!(
        rig.status(),
        SelfUpdateStatus::UpToDate { current: current() }
    );
}

// ---- held restart ----

#[test]
fn restart_waits_while_a_game_runs() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    publish(&server, INSTALLER, &key.sign(INSTALLER, NEXT));
    let rig = Rig::new(&server, &key);
    rig.ready();
    rig.game.0.lock().unwrap().push(RunningGame {
        pid: 4242,
        image: rig.d2r.d2r().join("D2R.exe"),
        since: 0,
    });

    let err = rig.restart().unwrap_err();
    assert_eq!(err.code, ErrorCode::GameRunning);
    let over_ipc = rig
        .invoke("restart_to_update_launcher", json!({}))
        .unwrap_err();
    assert_eq!(over_ipc["code"], "game_running");
    assert_eq!(rig.status(), ready_status());
    assert!(rig.host.installs().is_empty());
    assert!(rig.launcher.begin_quit().is_ok(), "held, not quitting");
}

#[test]
fn restart_proceeds_once_the_game_has_exited() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    publish(&server, INSTALLER, &key.sign(INSTALLER, NEXT));
    let rig = Rig::new(&server, &key);
    rig.ready();
    rig.game.0.lock().unwrap().push(RunningGame {
        pid: 4242,
        image: rig.d2r.d2r().join("D2R.exe"),
        since: 0,
    });
    assert!(rig.restart().is_err());
    rig.game.0.lock().unwrap().clear();
    rig.restart().unwrap();
    assert_eq!(rig.host.installs().len(), 1);
}

/// Restart to update waits while a product install is in progress, and never runs
/// during its install transaction; afterwards it proceeds.
#[test]
fn restart_waits_while_a_product_operation_runs() {
    const D2R_BYTES: &[u8] = b"fake D2R.exe for #88";
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    publish(&server, INSTALLER, &key.sign(INSTALLER, NEXT));
    let game = DisposableGame::new();
    fs::create_dir_all(game.d2r()).unwrap();
    fs::write(game.d2r().join("D2R.exe"), D2R_BYTES).unwrap();
    let r1 = support::release(support::PD2R_1);
    let mut manifest = r1.manifest.clone();
    manifest.archive = server.serve(&r1);
    manifest.compatibility.game_builds = vec![GameBuild {
        build: "fixture".into(),
        platform: Platform::WindowsX64,
        executable: RelativePath::new("D2R.exe").unwrap(),
        size: D2R_BYTES.len() as u64,
        sha256: support::sha256(D2R_BYTES),
    }];
    // Holds the install in its download while Restart to update is chosen.
    server.route(
        &r1.archive_path(),
        Reply::Delayed(
            Duration::from_secs(2),
            Box::new(Reply::Body(r1.archive.clone())),
        ),
    );
    game.product_root(&r1);
    let release = Release {
        manifest,
        sha256: support::sha256(&r1.manifest_json),
    };
    let setup = Setup {
        releases: Box::new(Published(release)),
        downloader: Downloader::with_tls(server.tls_config()),
    };
    let rig = Rig::with(&server, &key, game, setup);
    rig.ready();
    let (tx, outcomes) = channel();
    rig.app.listen_any(OUTCOME_EVENT, move |e| {
        let _ = tx.send(serde_json::from_str::<Value>(e.payload()).unwrap());
    });
    let request = json!({
        "contract_version": CONTRACT_VERSION,
        "data": { "product_id": "pd2r", "kind": "install" },
    });
    let plan = rig
        .invoke("inspect_operation", json!({ "request": request }))
        .unwrap();
    rig.invoke(
        "start_operation",
        json!({ "request": request, "inspection_token": plan["data"]["token"] }),
    )
    .unwrap();

    let err = rig.restart().unwrap_err();
    assert_eq!(err.code, ErrorCode::OperationInProgress, "{err:?}");
    assert_eq!(rig.status(), ready_status());
    assert!(rig.host.installs().is_empty());

    let outcome = outcomes.recv_timeout(Duration::from_secs(60)).unwrap();
    assert_eq!(outcome["data"]["status"], "succeeded", "{outcome}");
    rig.restart().unwrap();
    assert_eq!(rig.host.installs().len(), 1);
    // And now no product operation can start until the installer has taken over.
    let refused = rig
        .invoke(
            "inspect_operation",
            json!({ "request": json!({
            "contract_version": CONTRACT_VERSION,
            "data": { "product_id": "pd2r", "kind": "repair" },
        }) }),
        )
        .and_then(|plan| {
            rig.invoke(
                "start_operation",
                json!({ "request": {
                    "contract_version": CONTRACT_VERSION,
                    "data": { "product_id": "pd2r", "kind": "repair" },
                }, "inspection_token": plan["data"]["token"] }),
            )
        })
        .unwrap_err();
    assert_eq!(refused["code"], "operation_in_progress", "{refused}");
}

/// Two downloads at once: the second is refused, and the update downloads once.
#[test]
fn a_second_download_while_one_runs_is_refused() {
    let (server, key) = (FixtureServer::start(), TestKey::generate());
    server.route(
        LATEST,
        Reply::Body(latest_json(&server, &key.sign(INSTALLER, NEXT))),
    );
    server.route(
        SETUP,
        Reply::Delayed(
            Duration::from_millis(500),
            Box::new(Reply::Body(INSTALLER.to_vec())),
        ),
    );
    let rig = Rig::new(&server, &key);
    assert!(!rig.check(false).unwrap());
    let (updater, app) = (Arc::clone(&rig.updater), rig.app.handle().clone());
    let first = std::thread::spawn(move || tauri::async_runtime::block_on(updater.download(&app)));
    // The first has claimed the download and is waiting on the server.
    rig.phases_until("downloading");
    let second = rig.download();
    let first = first.join().unwrap();
    first.unwrap();
    assert_eq!(second.unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(rig.status(), ready_status());
    assert_eq!(server.hits().iter().filter(|h| *h == SETUP).count(), 1);
}
