//! The news commands (#80) over Tauri IPC with the mock runtime, fetching
//! `news.json` from the local TLS fixture server (`fixtures/news/`) and opening
//! links through a fake [`Desktop`]. Nothing here opens a real browser.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use resurrected_launcher_lib::app::{
    self, product_pages, CachedReleases, Desktop, Launcher, NewsItem, NewsState, COMMUNITY_URL,
};
use resurrected_launcher_lib::catalog::news::MAX_NEWS_SIZE;
use resurrected_launcher_lib::contract::{LauncherError, Versioned};
use resurrected_launcher_lib::game::SystemGameHost;
use resurrected_launcher_lib::install::download::Downloader;
use resurrected_launcher_lib::install::preflight::SystemHost;
use resurrected_launcher_lib::state::StateStore;
use serde_json::{json, Value};
use support::{FixtureServer, Reply};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};

const NEWS: &str = "/news.json";

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
        Err(std::io::Error::other("no Saved Games in tests"))
    }
    fn run_value(&self) -> std::io::Result<Option<String>> {
        Ok(None)
    }
    fn set_run_value(&self, _: Option<&str>) -> std::io::Result<()> {
        Ok(())
    }
}

struct Rig {
    server: FixtureServer,
    browser: Browser,
    webview: WebviewWindow<MockRuntime>,
    _state: tempfile::TempDir,
}

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/news")
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

impl Rig {
    /// A launcher whose `news.json` is the fixture server's [`NEWS`] path.
    fn new() -> Self {
        let server = FixtureServer::start();
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

    fn serving(reply: Reply) -> Self {
        let rig = Self::new();
        rig.server.route(NEWS, reply);
        rig
    }

    fn invoke(&self, cmd: &str, body: Value) -> Result<Value, LauncherError> {
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
        .map_err(|e| serde_json::from_value(e.clone()).unwrap_or_else(|_| panic!("{e}")))
    }

    /// The raw IPC error, for refusals before the command runs (argument types).
    fn refused(&self, cmd: &str, body: Value) -> String {
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
        .unwrap_err()
        .to_string()
    }

    fn news(&self, product: &str) -> NewsState {
        let reply = self
            .invoke("get_news", json!({ "product_id": product }))
            .unwrap();
        let v: Versioned<NewsState> = serde_json::from_value(reply).unwrap();
        v.into_current().unwrap()
    }

    fn open(&self, product: &str, index: usize) -> Result<Value, LauncherError> {
        self.invoke(
            "open_news_link",
            json!({ "product_id": product, "index": index }),
        )
    }

    fn opened(&self) -> Vec<String> {
        self.browser.0.lock().unwrap().clone()
    }

    fn fetches(&self) -> usize {
        self.server.hits().iter().filter(|h| *h == NEWS).count()
    }
}

fn titles(state: &NewsState) -> Vec<&str> {
    match state {
        NewsState::Ok { items } => items.iter().map(|i| i.title.as_str()).collect(),
        NewsState::Offline => panic!("offline"),
    }
}

fn code(e: LauncherError) -> String {
    serde_json::to_value(e.code)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_valid_file_shows_three_announcements_and_guides_newest_first() {
    let rig = Rig::serving(Reply::Body(fixture("valid.json")));
    let state = rig.news("pd2r");
    assert_eq!(
        titles(&state),
        [
            "TCP/IP co-op testing weekend",
            "Backing up your saves before an update",
            "Setting up D2RLoader",
        ]
    );
    let NewsState::Ok { items } = &state else {
        unreachable!()
    };
    assert_eq!(
        items[0],
        NewsItem {
            tag: "Announcement".into(),
            title: "TCP/IP co-op testing weekend".into(),
            summary: Some("Two days of open direct co-op games on the alpha.".into()),
            published: "2026-11-01".into(),
        }
    );
    assert_eq!(items[1].summary, None);
    // The reply is text only: no URL reaches the webview.
    let raw = rig
        .invoke("get_news", json!({ "product_id": "pd2r" }))
        .unwrap();
    assert!(!raw.to_string().contains("https://"), "{raw}");
    assert_eq!(
        titles(&rig.news("project-forever")),
        ["Legacy items: first look"]
    );
    assert_eq!(titles(&rig.news("project-09")), Vec::<&str>::new());
}

#[test]
fn links_open_by_index_from_the_last_reply_only() {
    let rig = Rig::serving(Reply::Body(fixture("valid.json")));
    // Nothing fetched yet: no link to open.
    assert_eq!(code(rig.open("pd2r", 0).unwrap_err()), "invalid_request");
    rig.news("pd2r");
    rig.open("pd2r", 1).unwrap();
    rig.open("pd2r", 0).unwrap();
    assert_eq!(
        rig.opened(),
        [
            "https://github.com/synpoox/pd2-resurrected/wiki/Backups",
            "https://discord.gg/mGpHT26Xak",
        ]
    );
    for (product, index) in [("pd2r", 3), ("project-forever", 0), ("unknown", 0)] {
        let err = rig.open(product, index).unwrap_err();
        assert_eq!(code(err), "invalid_request", "{product} {index}");
    }
    // A URL in place of an index is refused by the command's types.
    let err = rig.refused(
        "open_news_link",
        json!({ "product_id": "pd2r", "index": "https://evil.example/" }),
    );
    assert!(err.contains("invalid args"), "{err}");
    assert_eq!(rig.opened().len(), 2);
}

#[test]
fn invalid_files_read_offline_and_open_nothing() {
    let oversized = {
        let mut v = fixture("valid.json");
        v.resize(MAX_NEWS_SIZE as usize + 1, b' ');
        v
    };
    let cases = [
        ("oversized", Reply::Body(oversized)),
        ("non-https", Reply::Body(fixture("non-https.json"))),
        ("off-allowlist", Reply::Body(fixture("off-allowlist.json"))),
        ("malformed", Reply::Body(fixture("malformed.json"))),
        ("missing", Reply::Status(404)),
        ("server error", Reply::Status(500)),
        (
            "truncated",
            Reply::Truncated {
                body: fixture("valid.json"),
                after: 100,
            },
        ),
        ("redirect to http", Reply::RedirectToHttp(NEWS.into())),
    ];
    for (name, reply) in cases {
        let rig = Rig::serving(reply);
        assert_eq!(rig.news("pd2r"), NewsState::Offline, "{name}");
        assert_eq!(
            code(rig.open("pd2r", 0).unwrap_err()),
            "invalid_request",
            "{name}"
        );
        assert!(rig.opened().is_empty(), "{name}");
    }
}

#[test]
fn every_get_news_fetches_again_with_no_cache() {
    let rig = Rig::new();
    rig.server.once(NEWS, Reply::Status(503));
    rig.server.route(NEWS, Reply::Body(fixture("valid.json")));
    assert_eq!(rig.news("pd2r"), NewsState::Offline);
    // Retry.
    assert_eq!(titles(&rig.news("pd2r")).len(), 3);
    rig.news("project-forever");
    assert_eq!(rig.fetches(), 3);
    // Offline again: the earlier file is not shown, and its links are gone.
    rig.server.route(NEWS, Reply::Status(503));
    assert_eq!(rig.news("pd2r"), NewsState::Offline);
    assert_eq!(code(rig.open("pd2r", 0).unwrap_err()), "invalid_request");
}

/// Two overlapping `get_news` calls: the one started last owns the indexes, even
/// when the earlier one finishes after it.
#[test]
fn links_follow_the_latest_started_reply() {
    let item = |title: &str, url: &str| {
        serde_json::to_vec(&json!({ "schema_version": 1, "items": [{
            "product_id": "pd2r", "kind": "guide", "title": title,
            "published": "2026-11-01", "url": url,
        }]}))
        .unwrap()
    };
    let rig = Arc::new(Rig::new());
    rig.server.once(
        NEWS,
        Reply::Delayed(
            std::time::Duration::from_millis(800),
            Box::new(Reply::Body(item("Slow", "https://github.com/slow"))),
        ),
    );
    rig.server
        .route(NEWS, Reply::Body(item("Fast", "https://github.com/fast")));
    let slow = {
        let rig = Arc::clone(&rig);
        std::thread::spawn(move || rig.news("pd2r"))
    };
    while rig.fetches() == 0 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(titles(&rig.news("pd2r")), ["Fast"]);
    assert_eq!(titles(&slow.join().unwrap()), ["Slow"]);
    rig.open("pd2r", 0).unwrap();
    assert_eq!(rig.opened(), ["https://github.com/fast"]);
}

#[test]
fn product_pages_are_bundled_and_only_for_listed_products() {
    let rig = Rig::new();
    rig.invoke("open_product_page", json!({ "product_id": "pd2r" }))
        .unwrap();
    assert_eq!(rig.opened(), ["https://github.com/synpoox/pd2-resurrected"]);
    assert_eq!(
        product_pages(),
        [(
            "pd2r".to_owned(),
            "https://github.com/synpoox/pd2-resurrected".to_owned()
        )]
    );
    for product in ["project-forever", "project-09", "unknown"] {
        let err = rig
            .invoke("open_product_page", json!({ "product_id": product }))
            .unwrap_err();
        assert_eq!(code(err), "invalid_request", "{product}");
    }
    // Never fetched: the page needs no network.
    assert!(rig.server.hits().is_empty());
    assert_eq!(rig.opened().len(), 1);
}

#[test]
fn community_page_is_the_bundled_discord() {
    let rig = Rig::new();
    rig.invoke("open_community_page", json!({})).unwrap();
    assert_eq!(rig.opened(), [COMMUNITY_URL]);
    assert_eq!(COMMUNITY_URL, "https://discord.gg/4mKqKXMRy9");
    assert!(rig.server.hits().is_empty());
}
