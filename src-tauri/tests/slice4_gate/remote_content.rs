//! Clause "No remote content native privilege": capabilities grant only the
//! allowlisted commands, and only to `main` on the app's own origin; the CSP
//! allows no remote `connect-src` or `img-src`; news reaches the webview as text
//! and its links open only by index, through the native allowlist; a news file
//! with a non-https or off-allowlist URL is refused whole and opens nothing.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use resurrected_launcher_lib::app::{self, CachedReleases, Desktop, Launcher, NewsItem, NewsState};
use resurrected_launcher_lib::contract::{LauncherError, Versioned};
use resurrected_launcher_lib::game::SystemGameHost;
use resurrected_launcher_lib::install::download::Downloader;
use resurrected_launcher_lib::install::preflight::SystemHost;
use resurrected_launcher_lib::state::StateStore;
use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};

use crate::rig::code;
use crate::support::{self, FixtureServer, Reply};

fn root_file(rel: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)).unwrap()
}

/// The command allowlist as build.rs declares it to the ACL.
fn allowlisted() -> Vec<String> {
    let build = root_file("build.rs");
    let rest = &build[build.find(".commands(").unwrap()..];
    let list = &rest[rest.find('[').unwrap() + 1..rest.find(']').unwrap()];
    list.split(',')
        .map(|c| c.trim().trim_matches('"').to_owned())
        .filter(|c| !c.is_empty())
        .collect()
}

#[test]
fn capabilities_grant_only_the_allowlisted_commands_to_main() {
    let commands = allowlisted();
    assert!(commands.len() > 20, "{commands:?}");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
    let mut granted = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let cap: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let name = path.display().to_string();
        assert_eq!(cap["windows"], json!(["main"]), "{name}: windows");
        for key in ["remote", "webviews", "platforms"] {
            assert!(cap.get(key).is_none(), "{name}: `{key}` is set");
        }
        if let Some(local) = cap.get("local") {
            assert_eq!(local, &json!(true), "{name}: local");
        }
        for p in cap["permissions"].as_array().unwrap() {
            let id = p
                .as_str()
                .unwrap_or_else(|| panic!("{name}: scoped permission {p}"));
            granted.push(id.to_owned());
        }
    }
    let mut expected: Vec<String> = commands
        .iter()
        .map(|c| format!("allow-{}", c.replace('_', "-")))
        .collect();
    // Subscribing to the launcher's events; never emitting.
    expected.extend([
        "core:event:allow-listen".into(),
        "core:event:allow-unlisten".into(),
    ]);
    assert_eq!(granted, expected);
    // No inline capabilities and no global Tauri object for scripts.
    let conf: Value = serde_json::from_str(&root_file("tauri.conf.json")).unwrap();
    let inline = &conf["app"]["security"]["capabilities"];
    assert!(inline.is_null() || inline == &json!([]), "{inline}");
    assert!(conf["app"]["withGlobalTauri"].is_null());
    assert!(conf["app"]["security"]["dangerousDisableAssetCspModification"].is_null());
}

#[test]
fn the_csp_allows_no_remote_connect_src_or_img_src() {
    let conf: Value = serde_json::from_str(&root_file("tauri.conf.json")).unwrap();
    let csp = conf["app"]["security"]["csp"].as_str().unwrap();
    let directive = |name: &str| -> Option<Vec<&str>> {
        csp.split(';').map(str::trim).find_map(|d| {
            let mut parts = d.split_whitespace();
            (parts.next() == Some(name)).then(|| parts.collect())
        })
    };
    assert_eq!(directive("default-src").unwrap(), ["'self'"]);
    assert_eq!(directive("img-src").unwrap(), ["'self'"]);
    assert_eq!(
        directive("connect-src").unwrap(),
        ["ipc:", "http://ipc.localhost"]
    );
    for name in [
        "script-src",
        "frame-src",
        "object-src",
        "media-src",
        "font-src",
    ] {
        assert!(
            directive(name).is_none(),
            "{name} falls back to default-src"
        );
    }
    for token in csp.split_whitespace().map(|t| t.trim_end_matches(';')) {
        let wild = [
            "*",
            "https:",
            "http:",
            "data:",
            "blob:",
            "'unsafe-inline'",
            "'unsafe-eval'",
        ];
        assert!(!wild.contains(&token), "CSP allows {token}: {csp}");
        if token.contains("://") {
            assert_eq!(
                token, "http://ipc.localhost",
                "a remote source in the CSP: {csp}"
            );
        }
    }
}

fn call(webview: &WebviewWindow<MockRuntime>, cmd: &str, origin: &str) -> Result<(), String> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: origin.parse().unwrap(),
            body: InvokeBody::Json(json!({})),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// No command is reachable from another window or from a remote page loaded in
/// `main`: the ACL refuses before any command code runs.
#[test]
fn no_command_from_another_window_or_a_remote_origin() {
    let app = mock_builder()
        .invoke_handler(app::invoke_handler())
        .build(tauri::generate_context!())
        .unwrap();
    let main = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let other = WebviewWindowBuilder::new(&app, "news", Default::default())
        .build()
        .unwrap();
    assert!(call(&main, "ping", "http://tauri.localhost").is_ok());
    for cmd in allowlisted() {
        let e = call(&other, &cmd, "http://tauri.localhost").unwrap_err();
        assert!(e.contains("not allowed"), "{cmd} from another window: {e}");
        for origin in [
            "https://github.com",
            "https://evil.example",
            "http://127.0.0.1:1420",
        ] {
            let e = call(&main, &cmd, origin).unwrap_err();
            assert!(e.contains("not allowed"), "{cmd} from {origin}: {e}");
        }
    }
}

/// The system browser, in memory.
#[derive(Clone, Default)]
struct Browser(Arc<Mutex<Vec<String>>>);

impl Desktop for Browser {
    fn pick_folder(&self) -> Option<PathBuf> {
        None
    }
    fn open_loader_download_page(&self) -> std::io::Result<()> {
        Ok(())
    }
    fn open_link(&self, url: &str) -> std::io::Result<()> {
        self.0.lock().unwrap().push(url.to_owned());
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
        Err(std::io::Error::other("no Saved Games in the gate"))
    }
    fn run_value(&self) -> std::io::Result<Option<String>> {
        Ok(None)
    }
    fn set_run_value(&self, _: Option<&str>) -> std::io::Result<()> {
        Ok(())
    }
}

struct News {
    server: FixtureServer,
    browser: Browser,
    webview: WebviewWindow<MockRuntime>,
    _state: tempfile::TempDir,
}

const NEWS: &str = "/news.json";

impl News {
    fn serving(file: &Value) -> Self {
        let server = FixtureServer::start();
        server.route(NEWS, Reply::Body(serde_json::to_vec(file).unwrap()));
        let state = tempfile::tempdir().unwrap();
        let browser = Browser::default();
        let mut launcher = Launcher::new(
            StateStore::open(state.path()),
            Box::new(CachedReleases),
            Downloader::with_tls(server.tls_config()),
            support::seven_zip(),
            Box::new(SystemHost),
            Box::new(SystemGameHost),
            Box::new(browser.clone()),
        );
        launcher.feed_url = "https://feed.fixtures.invalid/feed.json".into();
        launcher.news_url = server.url(NEWS);
        let app = mock_builder()
            .invoke_handler(app::invoke_handler())
            .manage(Arc::new(launcher))
            .build(tauri::generate_context!())
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        std::mem::forget(app);
        Self {
            server,
            browser,
            webview,
            _state: state,
        }
    }

    fn news(&self) -> (NewsState, Value) {
        let raw =
            crate::rig::invoke(&self.webview, "get_news", json!({ "product_id": "pd2r" })).unwrap();
        let v: Versioned<NewsState> = serde_json::from_value(raw.clone()).unwrap();
        (v.into_current().unwrap(), raw)
    }

    fn open(&self, body: Value) -> Result<Value, LauncherError> {
        crate::rig::invoke(&self.webview, "open_news_link", body)
    }

    fn opened(&self) -> Vec<String> {
        self.browser.0.lock().unwrap().clone()
    }
}

fn file(items: Vec<Value>) -> Value {
    json!({ "schema_version": 1, "items": items })
}

fn item(title: &str, url: &str) -> Value {
    json!({ "product_id": "pd2r", "kind": "guide", "title": title, "published": "2026-11-01", "url": url })
}

const MARKUP: &str =
    r#"<img src="https://evil.example/x.png" onerror="alert(1)"> <script>x()</script>"#;

#[test]
fn news_reaches_the_webview_as_text_and_links_open_only_by_index() {
    let rig = News::serving(&file(vec![
        item(MARKUP, "https://github.com/synpoox/pd2-resurrected/wiki"),
        item("Second", "https://discord.gg/mGpHT26Xak"),
    ]));
    let (state, raw) = rig.news();
    let NewsState::Ok { items } = state else {
        panic!("offline: {raw}")
    };
    assert_eq!(items.len(), 2);
    let marked: &NewsItem = items.iter().find(|i| i.title == MARKUP).unwrap();
    assert_eq!(
        marked.title, MARKUP,
        "title passed through as text, unchanged"
    );
    // The webview gets tag, title and date: no URL, image or markup field.
    let text = raw.to_string();
    assert!(
        !text.contains("github.com") && !text.contains("discord.gg"),
        "{text}"
    );
    for v in raw["data"]["items"].as_array().unwrap() {
        let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["published", "tag", "title"], "{v}");
    }

    // By index, from this reply, through the native allowlist.
    let index = |title: &str| items.iter().position(|i| i.title == title).unwrap();
    rig.open(json!({ "product_id": "pd2r", "index": index("Second") }))
        .unwrap();
    assert_eq!(rig.opened(), ["https://discord.gg/mGpHT26Xak"]);
    // An index past the list, a URL, or a string index is refused; nothing opens.
    let past = rig
        .open(json!({ "product_id": "pd2r", "index": 9 }))
        .unwrap_err();
    assert_eq!(code(past), "invalid_request");
    for body in [
        json!({ "product_id": "pd2r", "index": "https://evil.example" }),
        json!({ "product_id": "pd2r", "url": "https://evil.example" }),
        json!({ "product_id": "pd2r", "index": -1 }),
    ] {
        let e = get_ipc_response(
            &rig.webview,
            InvokeRequest {
                cmd: "open_news_link".into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "http://tauri.localhost".parse().unwrap(),
                body: InvokeBody::Json(body.clone()),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        );
        assert!(e.is_err(), "{body} was accepted");
    }
    assert_eq!(rig.opened().len(), 1);
    assert!(
        rig.server.hits().iter().all(|h| h == NEWS),
        "only news.json fetched"
    );
}

#[test]
fn a_non_https_or_off_allowlist_news_url_is_refused() {
    let bad = [
        "http://github.com/synpoox/pd2-resurrected",
        "https://evil.example/x",
        "https://github.com.evil.example/x",
        "https://evil.example@github.com/x",
        "https://github.com:8443/x",
        "https://140.82.112.3/x",
        "https://github.com/a/../b",
        "https://github.com/a,b",
        "javascript:alert(1)",
        "file:///C:/Windows/System32/calc.exe",
        "ms-settings:privacy",
        "//github.com/x",
    ];
    for url in bad {
        let rig = News::serving(&file(vec![
            item("Fine", "https://github.com/synpoox/pd2-resurrected"),
            item("Bad", url),
        ]));
        let (state, raw) = rig.news();
        assert_eq!(state, NewsState::Offline, "{url}: {raw}");
        assert_eq!(
            code(
                rig.open(json!({ "product_id": "pd2r", "index": 0 }))
                    .unwrap_err()
            ),
            "invalid_request",
            "{url}"
        );
        assert_eq!(
            code(
                rig.open(json!({ "product_id": "pd2r", "index": 1 }))
                    .unwrap_err()
            ),
            "invalid_request",
            "{url}"
        );
        assert!(rig.opened().is_empty(), "{url}");
    }
}
