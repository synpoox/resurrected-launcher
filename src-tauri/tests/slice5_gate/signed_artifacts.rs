//! Signed artifacts: what a `launcher-release` run uploaded verifies against the
//! public key embedded in `tauri.conf.json`. Needs the downloaded artifacts, so it
//! is ignored by default. Run it with
//!
//! ```text
//! $env:LAUNCHER_GATE_ARTIFACTS = "C:\Users\<you>\Downloads\launcher-gate"
//! cargo test --test slice5_gate signed_artifacts -- --ignored --nocapture
//! ```
//!
//! Every folder under that path (and the path itself) holding a `latest.json` is
//! one workflow artifact.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde_json::Value;

use super::json;

fn decode(text: &str) -> String {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .expect("base64");
    String::from_utf8(bytes).expect("utf-8")
}

fn artifact_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::iter::once(root.to_owned())
        .chain(fs::read_dir(root).unwrap().flatten().map(|e| e.path()))
        .filter(|d| d.join("latest.json").is_file())
        .collect();
    dirs.sort();
    dirs
}

/// Checks one artifact folder; returns `(version, gate)`.
fn verify_artifact(dir: &Path, pk: &minisign::PublicKey) -> (String, bool) {
    let setups: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with("_x64-setup.exe"))
        .collect();
    assert_eq!(setups.len(), 1, "{}: {setups:?}", dir.display());
    let setup = &setups[0];
    let name = setup.file_name().unwrap().to_str().unwrap().to_owned();
    let version = name
        .strip_prefix("resurrected-launcher_")
        .and_then(|r| r.strip_suffix("_x64-setup.exe"))
        .unwrap_or_else(|| panic!("unexpected installer name {name}"))
        .to_owned();
    let bytes = fs::read(setup).unwrap();
    let sig_text = fs::read_to_string(format!("{}.sig", setup.display())).unwrap();
    let sig = minisign::SignatureBox::from_string(&decode(&sig_text)).unwrap();
    minisign::verify(pk, &sig, Cursor::new(&bytes), true, false, false)
        .unwrap_or_else(|e| panic!("{name}: signature does not verify: {e}"));
    let trusted = sig.trusted_comment().unwrap();
    assert!(
        trusted
            .split('\t')
            .any(|f| f == format!("version:{version}")),
        "{name}: signature not bound to {version}: {trusted}"
    );
    // The same bytes with one byte changed do not verify.
    let mut tampered = bytes.clone();
    let mid = tampered.len() / 2;
    tampered[mid] ^= 0xff;
    assert!(minisign::verify(pk, &sig, Cursor::new(&tampered), true, false, false).is_err());

    let latest: Value = serde_json::from_str(
        fs::read_to_string(dir.join("latest.json"))
            .unwrap()
            .trim_start_matches('\u{feff}'),
    )
    .unwrap();
    assert_eq!(latest["version"], version.as_str());
    let entry = &latest["platforms"]["windows-x86_64"];
    assert_eq!(entry["signature"], sig_text.trim());
    let url = entry["url"].as_str().unwrap();
    let gate = url.starts_with("http://127.0.0.1:8789/");
    let release = format!(
        "https://github.com/synpoox/resurrected-launcher/releases/download/launcher-v{version}/"
    );
    assert!(gate || url.starts_with(&release), "{url}");
    assert!(url.ends_with(&format!("/{name}")), "{url}");
    eprintln!(
        "{}: {name} {version} ({} bytes), signature verifies against the embedded key; {}",
        dir.display(),
        bytes.len(),
        if gate { "gate build" } else { "release build" }
    );
    (version, gate)
}

#[test]
#[ignore = "needs LAUNCHER_GATE_ARTIFACTS: the downloaded launcher-release workflow artifacts"]
fn signed_artifacts_verify_against_the_embedded_public_key() {
    let root = PathBuf::from(
        std::env::var_os("LAUNCHER_GATE_ARTIFACTS").expect("set LAUNCHER_GATE_ARTIFACTS"),
    );
    let embedded = json("src-tauri/tauri.conf.json")["plugins"]["updater"]["pubkey"]
        .as_str()
        .unwrap()
        .to_owned();
    let pk = minisign::PublicKey::from_box(
        minisign::PublicKeyBox::from_string(&decode(&embedded)).unwrap(),
    )
    .unwrap();
    let dirs = artifact_dirs(&root);
    assert!(!dirs.is_empty(), "no latest.json under {}", root.display());
    for dir in dirs {
        verify_artifact(&dir, &pk);
    }
}
