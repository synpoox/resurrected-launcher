//! The slice 5 gate (#89): the #8 slice 5 clauses (packaging and signed launcher
//! updates), one named test or recorded check per clause. The clean-machine rows
//! of the matrix in `docs/release/launcher-signing.md` are run by the owner on
//! Windows 11 from the signed gate builds; their checklist and results are in the
//! gate PR. These tests prove what that checklist relies on.
//!
//! Gate builds come from a `launcher-v<version>-gate` tag: self-update on and the
//! updater endpoint on `127.0.0.1:8789` (`src-tauri/gate/tauri.gate.conf.json`),
//! fed by `src-tauri/gate/serve-update.ps1`. The key is the release key; no test
//! here uses it, creates it or reads it. Every test key is ephemeral.
//!
//! | Clause | Tests (and recorded checks) |
//! | --- | --- |
//! | Signed artifacts | [`signed_artifacts`]: the downloaded installer's `.sig` verifies against the embedded key, and `latest.json` names that signature and version (run with `--ignored` on the artifacts); [`gate_build`]: the workflow's tag check and gate mode |
//! | Clean installs | [`gate_build`]: per-user NSIS, offline WebView2, bundled 7-Zip, no elevation in the bundle; owner checklist rows 1, 9 |
//! | Missing runtime | [`gate_build`]: the installer embeds the offline WebView2 installer; owner checklist row 2 (gap: no no-WebView2 snapshot) |
//! | Normal-user permissions | [`gate_build`]: per-user install and passive update mode; `failure_matrix::refusals` (`access_denied`); owner checklist row 3 |
//! | Upgrades | [`local_endpoint`]: a gate build's updater, fed by `serve-update.ps1`, downloads and keeps N+1; owner checklist row 4 |
//! | Bad-signature rejection | [`local_endpoint`]: tampered, wrong-key and unsigned, each rejected through `serve-update.ps1` with nothing kept; owner checklist row 5 |
//! | Interrupted update recovery | [`local_endpoint`]: the endpoint dying mid-update leaves N running and the retry succeeds; `self_update.rs` markers; owner checklist row 6 |
//! | Signing-key recovery | [`local_endpoint`]: an update signed with a key restored from an encrypted recovery copy is accepted; owner checklist row 8 with the real recovery copy |
#![cfg(windows)]

mod gate_build;
mod local_endpoint;
mod signed_artifacts;

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The repository root.
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub fn read(rel: &str) -> String {
    std::fs::read_to_string(repo().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

pub fn json(rel: &str) -> Value {
    serde_json::from_str(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// RFC 7396 merge patch, which is how `tauri build --config` merges a file over
/// `tauri.conf.json`.
pub fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(patch) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Default::default());
    }
    let map = target.as_object_mut().unwrap();
    for (k, v) in patch {
        if v.is_null() {
            map.remove(k);
        } else {
            merge_patch(map.entry(k.clone()).or_insert(Value::Null), v);
        }
    }
}

/// `tauri.conf.json` as a gate build sees it.
pub fn gate_conf() -> Value {
    let mut conf = json("src-tauri/tauri.conf.json");
    merge_patch(&mut conf, &json("src-tauri/gate/tauri.gate.conf.json"));
    conf
}
