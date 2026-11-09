//! The gate build's update path, end to end on this machine: the gate updater
//! config (loopback endpoint) fed by the real `src-tauri/gate/serve-update.ps1`,
//! the same script and scenarios the owner runs against the signed gate builds.
//! Keys are ephemeral, made in a temp dir. The installer never runs: a recording
//! [`UpdateHost`] stands in for it.

use std::fs;
use std::io::{self, BufRead, BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use base64::Engine;
use resurrected_launcher_lib::app::{SelfUpdateStatus, SelfUpdater, UpdateHost, UPDATE_DIR};
use resurrected_launcher_lib::contract::ErrorCode;
use serde_json::json;
use tauri::test::{mock_builder, MockRuntime};
use tauri::App;

use super::{gate_conf, repo};

const NEXT: &str = "99.0.0";
const NAME: &str = "resurrected-launcher_99.0.0_x64-setup.exe";

fn b64(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text)
}

/// Fake installer bytes; large enough that a flipped middle byte is a real change.
fn installer() -> Vec<u8> {
    (0..256 * 1024u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect()
}

struct Key {
    pk: minisign::PublicKey,
    sk: minisign::SecretKey,
}

impl Key {
    fn generate() -> Self {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        Self {
            pk: pair.pk,
            sk: pair.sk,
        }
    }

    fn pubkey(&self) -> String {
        b64(&self.pk.to_box().unwrap().to_string())
    }

    /// A `.sig` file's contents, as `tauri signer sign --app-version` writes them.
    fn sign(&self, bytes: &[u8]) -> String {
        let trusted = format!("timestamp:0\tfile:{NAME}\tversion:{NEXT}");
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

/// `serve-update.ps1`, stopped on drop.
struct Server {
    child: Child,
    port: u16,
}

enum Sig<'a> {
    File(&'a Path),
    Unsigned,
}

impl Server {
    fn start(installer: &Path, sig: Sig, tamper: bool, port: u16) -> Self {
        let script = repo().join("src-tauri/gate/serve-update.ps1");
        let mut cmd = Command::new("powershell.exe");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .arg("-Installer")
        .arg(installer)
        .args(["-Version", NEXT, "-Port", &port.to_string()]);
        match sig {
            Sig::File(p) => {
                cmd.arg("-Signature").arg(p);
            }
            Sig::Unsigned => {
                cmd.arg("-Unsigned");
            }
        }
        if tamper {
            cmd.arg("-Tamper");
        }
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        let port = line
            .strip_prefix("Serving http://127.0.0.1:")
            .and_then(|r| r.split('/').next())
            .and_then(|p| p.parse().ok())
            .unwrap_or_else(|| panic!("serve-update.ps1 did not start: {line:?}"));
        // Keep draining the request log so the script never blocks on a full pipe.
        std::thread::spawn(move || for _ in out.lines() {});
        Self { child, port }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default, Clone)]
struct Recorder(Arc<Mutex<Vec<Vec<u8>>>>);

impl UpdateHost for Recorder {
    fn install(
        &self,
        _: &tauri_plugin_updater::Update,
        bytes: &[u8],
        _: bool,
    ) -> Result<(), tauri_plugin_updater::Error> {
        self.0.lock().unwrap().push(bytes.to_vec());
        Ok(())
    }
    fn open_page(&self, _: &str) -> io::Result<()> {
        Ok(())
    }
}

/// Launcher N built in gate mode: the merged gate updater config, with the test
/// key in place of the release key and the server's port in place of 8789.
struct Launcher {
    app: App<MockRuntime>,
    updater: SelfUpdater,
    dir: PathBuf,
    _data: tempfile::TempDir,
}

impl Launcher {
    fn new(pubkey: &str, port: u16) -> Self {
        let mut config = gate_conf()["plugins"]["updater"].clone();
        assert_eq!(config["endpoints"][0], "http://127.0.0.1:8789/latest.json");
        config["endpoints"] = json!([format!("http://127.0.0.1:{port}/latest.json")]);
        config["pubkey"] = json!(pubkey);
        let mut context = tauri::generate_context!();
        context
            .config_mut()
            .plugins
            .0
            .insert("updater".into(), config);
        let app = mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join(UPDATE_DIR);
        let updater = SelfUpdater::new(
            dir.clone(),
            env!("CARGO_PKG_VERSION").into(),
            Box::new(Recorder::default()),
        );
        Self {
            app,
            updater,
            dir,
            _data: data,
        }
    }

    fn check(&self) -> Result<bool, resurrected_launcher_lib::contract::LauncherError> {
        tauri::async_runtime::block_on(self.updater.check(self.app.handle(), false))
    }

    fn download(&self) -> Result<(), resurrected_launcher_lib::contract::LauncherError> {
        tauri::async_runtime::block_on(self.updater.download(self.app.handle()))
    }

    fn kept(&self) -> Vec<PathBuf> {
        fs::read_dir(&self.dir)
            .map(|d| d.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    }
}

fn status(phase: &str) -> SelfUpdateStatus {
    let current = env!("CARGO_PKG_VERSION").to_owned();
    let next = NEXT.to_owned();
    match phase {
        "ready" => SelfUpdateStatus::Ready { current, next },
        "rejected" => SelfUpdateStatus::SignatureRejected { current, next },
        "failed" => SelfUpdateStatus::DownloadFailed { current, next },
        _ => unreachable!(),
    }
}

/// The served installer and its signature, written like the workflow artifact.
struct Files {
    dir: tempfile::TempDir,
}

impl Files {
    fn new(bytes: &[u8], sig: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(NAME), bytes).unwrap();
        fs::write(dir.path().join(format!("{NAME}.sig")), sig).unwrap();
        Self { dir }
    }
    fn installer(&self) -> PathBuf {
        self.dir.path().join(NAME)
    }
    fn sig(&self) -> PathBuf {
        self.dir.path().join(format!("{NAME}.sig"))
    }
}

fn assert_ready(launcher: &Launcher, bytes: &[u8]) {
    assert!(!launcher.check().unwrap());
    launcher.download().unwrap();
    assert_eq!(launcher.updater.status(), status("ready"));
    let kept = launcher.kept();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(fs::read(&kept[0]).unwrap(), bytes);
}

fn assert_rejected(case: &str, launcher: &Launcher) {
    assert!(!launcher.check().unwrap(), "{case}");
    let err = launcher.download().unwrap_err();
    assert_eq!(err.code, ErrorCode::VerificationFailed, "{case}: {err:?}");
    assert_eq!(launcher.updater.status(), status("rejected"), "{case}");
    assert!(
        launcher.kept().is_empty(),
        "{case}: kept {:?}",
        launcher.kept()
    );
}

/// Upgrades: N (a gate build) finds N+1 on the loopback endpoint, downloads it
/// and keeps the verified installer for Restart to update.
#[test]
fn a_gate_build_updates_from_the_local_endpoint() {
    let (key, bytes) = (Key::generate(), installer());
    let files = Files::new(&bytes, &key.sign(&bytes));
    let server = Server::start(&files.installer(), Sig::File(&files.sig()), false, 0);
    assert_ready(&Launcher::new(&key.pubkey(), server.port), &bytes);
}

/// Bad-signature rejection, through the owner's script: a tampered installer, a
/// wrong-key signature and an unsigned artifact. Each ends in signature rejected
/// with nothing kept, and N keeps running (the status still names it as current).
#[test]
fn tampered_wrong_key_and_unsigned_updates_are_rejected_from_the_local_endpoint() {
    let (key, other, bytes) = (Key::generate(), Key::generate(), installer());
    let good = Files::new(&bytes, &key.sign(&bytes));
    let wrong = Files::new(&bytes, &other.sign(&bytes));
    for (case, files, sig, tamper) in [
        ("tampered installer", &good, true, true),
        ("wrong key", &wrong, true, false),
        ("unsigned artifact", &good, false, false),
    ] {
        let sig_path = files.sig();
        let sig = if sig {
            Sig::File(&sig_path)
        } else {
            Sig::Unsigned
        };
        let server = Server::start(&files.installer(), sig, tamper, 0);
        assert_rejected(case, &Launcher::new(&key.pubkey(), server.port));
    }
    // The same files untampered are accepted, so the script's -Tamper is the cause.
    let server = Server::start(&good.installer(), Sig::File(&good.sig()), false, 0);
    assert_ready(&Launcher::new(&key.pubkey(), server.port), &bytes);
}

/// Interrupted update: the endpoint dies after the check found N+1. The download
/// fails, nothing is kept, N keeps running, and once the endpoint is back the
/// retry downloads and verifies N+1.
#[test]
fn an_update_interrupted_by_a_dead_endpoint_keeps_n_and_retries() {
    let (key, bytes) = (Key::generate(), installer());
    let files = Files::new(&bytes, &key.sign(&bytes));
    let server = Server::start(&files.installer(), Sig::File(&files.sig()), false, 0);
    let port = server.port;
    let launcher = Launcher::new(&key.pubkey(), port);
    assert!(!launcher.check().unwrap());
    drop(server);
    let err = launcher.download().unwrap_err();
    assert_eq!(err.code, ErrorCode::DownloadFailed, "{err:?}");
    assert_eq!(launcher.updater.status(), status("failed"));
    assert!(launcher.kept().is_empty());
    let _back = Server::start(&files.installer(), Sig::File(&files.sig()), false, port);
    assert_ready(&launcher, &bytes);
}

/// Signing-key recovery: the key restored from its encrypted recovery copy (with
/// its password) signs an update that N, which trusts the original public key,
/// accepts. The wrong password restores nothing.
#[test]
fn an_update_signed_with_the_key_restored_from_the_recovery_copy_is_accepted() {
    let vault = tempfile::tempdir().unwrap();
    let recovery = vault.path().join("resurrected-launcher.key");
    let password = "correct horse battery staple".to_owned();
    let pair = minisign::KeyPair::generate_and_write_encrypted_keypair(
        fs::File::create(vault.path().join("resurrected-launcher.key.pub")).unwrap(),
        fs::File::create(&recovery).unwrap(),
        None,
        Some(password.clone()),
    )
    .unwrap();
    // N trusts only the public key, like tauri.conf.json.
    let trusted = b64(&pair.pk.to_box().unwrap().to_string());
    // The working copy is gone; only the recovery copy remains.
    let minisign::KeyPair { pk, .. } = pair;

    assert!(minisign::SecretKey::from_file(&recovery, Some("wrong".into())).is_err());
    let restored = Key {
        pk,
        sk: minisign::SecretKey::from_file(&recovery, Some(password)).unwrap(),
    };
    let bytes = installer();
    let files = Files::new(&bytes, &restored.sign(&bytes));
    let server = Server::start(&files.installer(), Sig::File(&files.sig()), false, 0);
    assert_ready(&Launcher::new(&trusted, server.port), &bytes);
}
