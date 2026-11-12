//! What the signed workflow builds: a release launcher with self-update on and
//! the shipped config, or (on a `launcher-v<version>-gate` tag) a gate build whose
//! only difference is a loopback updater endpoint. And what
//! every build installs: per user, no elevation, offline WebView2, bundled 7-Zip.

use serde_json::{json, Value};
use tauri::test::mock_builder;
use tauri_plugin_updater::UpdaterExt;

use super::{gate_conf, json, read};

const GATE_ENDPOINT: &str = "http://127.0.0.1:8789/latest.json";

/// The gate file sets the endpoint and the flag that permits plain HTTP on it,
/// nothing else: the key, the install mode and the rest come from `tauri.conf.json`.
#[test]
fn the_gate_config_changes_only_the_endpoint_never_the_key() {
    let gate = json("src-tauri/gate/tauri.gate.conf.json");
    assert_eq!(
        gate,
        json!({ "plugins": { "updater": {
            "endpoints": [GATE_ENDPOINT],
            "dangerousInsecureTransportProtocol": true
        } } })
    );
    let base = json("src-tauri/tauri.conf.json");
    let merged = gate_conf();
    let (b, m) = (&base["plugins"]["updater"], &merged["plugins"]["updater"]);
    assert_eq!(
        m["pubkey"], b["pubkey"],
        "a gate build trusts the release key only"
    );
    assert_eq!(m["windows"], b["windows"]);
    assert_eq!(m["endpoints"], json!([GATE_ENDPOINT]));
    // Everything outside the updater endpoint is the release config.
    let mut rest = merged.clone();
    rest["plugins"]["updater"]["endpoints"] = b["endpoints"].clone();
    rest["plugins"]["updater"]
        .as_object_mut()
        .unwrap()
        .remove("dangerousInsecureTransportProtocol");
    assert_eq!(rest, base);
}

fn app_with_updater(updater: Value) -> Result<(), String> {
    let mut context = tauri::generate_context!();
    context
        .config_mut()
        .plugins
        .0
        .insert("updater".into(), updater);
    let app = mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .map_err(|e| e.to_string())?;
    app.updater().map(drop).map_err(|e| e.to_string())
}

/// The merged gate config starts the updater. (Release builds refuse plain HTTP
/// without `dangerousInsecureTransportProtocol`; debug builds only warn, so the
/// signed gate build itself is the proof that the flag is enough.)
#[test]
fn the_gate_config_starts_the_updater() {
    let updater = gate_conf()["plugins"]["updater"].clone();
    app_with_updater(updater).expect("the gate build's updater config is valid");
}

/// Every build sets `VITE_SELF_UPDATE=1` (#157). The workflow turns gate mode on
/// only for `launcher-v<version>-gate`, and only then merges the gate config. A
/// release tag (`launcher-v<version>`) builds with the shipped config.
#[test]
fn release_builds_have_self_update_on_and_only_gate_tags_change_the_build() {
    let yml = read(".github/workflows/release.yml").replace("\r\n", "\n");
    let lines: Vec<&str> = yml.lines().map(str::trim).collect();
    let has = |l: &str| lines.contains(&l);

    // The only way gate mode turns on.
    assert!(has(
        r#"$gate = $env:GITHUB_REF_TYPE -eq 'tag' -and $env:GITHUB_REF_NAME -eq "launcher-v$version-gate""#
    ));
    assert!(has(
        r#"if ($env:GITHUB_REF_TYPE -eq 'tag' -and -not $gate -and $env:GITHUB_REF_NAME -ne "launcher-v$version") {"#
    ));
    // The flag is "1" (on) for every build, gate or not; flag.ts turns on only for "1".
    let flag: Vec<_> = lines
        .iter()
        .filter(|l| l.starts_with("VITE_SELF_UPDATE:"))
        .collect();
    assert_eq!(flag, [&"VITE_SELF_UPDATE: '1'"]);
    // --config only in the gate branch, and only with the gate file.
    let configs: Vec<_> = lines.iter().filter(|l| l.contains("--config")).collect();
    assert_eq!(
        configs,
        [
            &r#"npx tauri build --ci --config "$env:GITHUB_WORKSPACE/src-tauri/gate/tauri.gate.conf.json""#
        ]
    );
    let at = lines.iter().position(|l| l.contains("--config")).unwrap();
    assert_eq!(lines[at - 1], "if ($env:GATE -eq 'true') {");
    assert_eq!(
        lines[at + 1..at + 4],
        ["} else {", "npx tauri build --ci", "}"]
    );
    // The local download URL and the artifact name follow the same switch.
    assert!(has(
        r#"if ($env:GATE -eq 'true') { $url = "http://127.0.0.1:8789/$name" }"#
    ));
    assert!(has(
        "name: launcher-${{ steps.version.outputs.version }}${{ steps.version.outputs.gate == 'true' && '-gate' || '' }}-unpublished"
    ));
    // The only release step is the draft job (#238): a draft, never published,
    // never tagged. Comments are ignored; every other line is checked.
    for word in ["softprops", "create-release", "git tag", "git push"] {
        assert!(!yml.contains(word), "release.yml mentions {word}");
    }
    let raw: Vec<&str> = yml.lines().collect();
    let start = raw
        .iter()
        .position(|l| *l == "  draft-release:")
        .expect("release.yml has a draft-release job");
    let end = raw[start + 1..]
        .iter()
        .position(|l| l.starts_with("  ") && !l.starts_with("   ") && !l.trim().starts_with('#'))
        .map_or(raw.len(), |i| start + 1 + i);
    let code = |l: &&str| !l.trim().starts_with('#');
    let in_job = |i: usize| i > start && i < end;
    let mentions = |word: &str| -> Vec<usize> {
        (0..raw.len())
            .filter(|&i| code(&raw[i]) && raw[i].contains(word))
            .collect()
    };
    // Every `gh release` is inside the draft-release job.
    let gh = mentions("gh release");
    assert!(!gh.is_empty());
    assert!(
        gh.iter().all(|&i| in_job(i)),
        "gh release outside draft-release"
    );
    // The only create is a --draft --verify-tag; nothing publishes or edits.
    let creates: Vec<&str> = gh
        .iter()
        .map(|&i| raw[i])
        .filter(|l| l.contains("gh release create"))
        .collect();
    assert_eq!(creates.len(), 1);
    assert!(creates[0].contains(" --draft ") && creates[0].contains(" --verify-tag"));
    for l in gh.iter().map(|&i| raw[i]) {
        assert!(
            l.contains("gh release create") || l.contains("gh release view"),
            "unexpected gh release use: {l}"
        );
    }
    for word in ["--latest", "gh release edit", "--draft=false"] {
        assert!(!yml.contains(word), "release.yml mentions {word}");
    }
    // Read-only by default; write access is the draft job's alone.
    assert!(has("contents: read"));
    let writes = mentions("contents: write");
    assert_eq!(writes.len(), 1, "contents: write appears once");
    assert!(in_job(writes[0]), "contents: write outside draft-release");
    assert!(raw[..start].contains(&"  contents: read"));
}

/// Every build installs per user (no UAC), runs as invoker, carries the offline
/// WebView2 installer and the pinned 7-Zip, and updates in passive mode.
#[test]
fn the_installer_is_per_user_with_offline_webview2_and_bundled_7zip() {
    let conf = json("src-tauri/tauri.conf.json");
    let bundle = &conf["bundle"];
    assert_eq!(bundle["targets"], json!(["nsis"]));
    assert_eq!(bundle["windows"]["nsis"]["installMode"], "currentUser");
    assert_eq!(
        bundle["windows"]["webviewInstallMode"],
        json!({ "type": "offlineInstaller" })
    );
    assert_eq!(bundle["resources"], json!(["resources/7zip/*"]));
    assert!(super::repo()
        .join("src-tauri/resources/7zip/7za.exe")
        .is_file());
    assert_eq!(
        conf["plugins"]["updater"]["windows"]["installMode"],
        "passive"
    );
    let manifest = read("src-tauri/windows-app-manifest.xml");
    assert!(manifest.contains(r#"<requestedExecutionLevel level="asInvoker" uiAccess="false" />"#));
    for word in ["requireAdministrator", "highestAvailable"] {
        assert!(!manifest.contains(word));
    }
}
