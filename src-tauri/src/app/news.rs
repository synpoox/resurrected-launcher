//! News feed and outbound link commands (#80): `get_news`, `open_news_link` and
//! `open_product_page`. The frontend sends product IDs and an index only, never a
//! URL. Every `get_news` downloads `news.json` again (no cache, #80); links come
//! from the last validated reply. Product pages are bundled from
//! `src/product-pages.json`. Nothing remote is rendered: replies are plain text.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::State;

use super::operations::{invalid, locked, Launcher};
use crate::catalog::news::{self, News, MAX_NEWS_SIZE};
use crate::contract::{ErrorCode, LauncherError, ProductId, Versioned};
use crate::install::download::Expect;

/// Where the owner publishes `news.json`: one file for every product, a release
/// asset of the launcher repository (owner decision on #80).
pub const DEFAULT_NEWS_URL: &str =
    "https://github.com/synpoox/resurrected-launcher/releases/latest/download/news.json";

/// "About <product>": each product's public page, bundled, never fetched. The one
/// table, shared with the frontend (`src/products.ts` shows the button exactly for
/// these products). Forever and 09 have no public repository yet.
const PRODUCT_PAGES_JSON: &str = include_str!("../../../src/product-pages.json");

/// The project Discord, offered by the in-launcher About dialog of products without
/// a page (#135). Bundled, never fetched; passes [`news::link`] (tested).
pub const COMMUNITY_URL: &str = "https://discord.gg/4mKqKXMRy9";

/// [`PRODUCT_PAGES_JSON`] as `(product id, URL)`, sorted by id. Each URL passes
/// [`news::link`] (tested).
pub fn product_pages() -> &'static [(String, String)] {
    static PAGES: OnceLock<Vec<(String, String)>> = OnceLock::new();
    PAGES.get_or_init(|| {
        let map: std::collections::BTreeMap<String, String> =
            serde_json::from_str(PRODUCT_PAGES_JSON).expect("src/product-pages.json is valid");
        map.into_iter().collect()
    })
}

/// `get_news` reply, mirrored by the frontend's `NewsState`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NewsState {
    Ok {
        items: Vec<NewsItem>,
    },
    /// Offline, or the file was missing, too large or invalid.
    Offline,
}

/// A shown item: text only; the link stays native.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsItem {
    pub tag: String,
    pub title: String,
    /// Short description under the title (#139); omitted when the file has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub published: String,
}

/// Per product, the links of the latest-started `get_news` reply, in reply order.
/// A reply that finishes after a newer request started leaves them alone; the
/// frontend likewise shows only its latest request's reply, so an index always
/// means the same item on both sides.
#[derive(Default)]
pub(super) struct NewsHeld {
    started: HashMap<ProductId, u64>,
    links: HashMap<ProductId, Vec<String>>,
}

impl Launcher {
    /// Downloads and validates [`Launcher::news_url`]. No lock is held.
    fn fetch_news(&self) -> Result<News, LauncherError> {
        let cancel = AtomicBool::new(false);
        let (bytes, _) =
            self.downloader
                .fetch_bytes(&self.news_url, Expect::AtMost(MAX_NEWS_SIZE), &cancel)?;
        News::parse(&bytes)
            .map_err(|e| LauncherError::new(ErrorCode::MetadataInvalid, e.to_string()))
    }

    pub(super) fn news_for(&self, product: &ProductId) -> Result<NewsState, LauncherError> {
        let ticket = {
            let mut held = locked(&self.news)?;
            let n = held.started.entry(product.clone()).or_default();
            *n += 1;
            *n
        };
        let fetched = self.fetch_news();
        let mut held = locked(&self.news)?;
        let latest = held.started.get(product) == Some(&ticket);
        let Ok(file) = fetched else {
            if latest {
                held.links.remove(product);
            }
            return Ok(NewsState::Offline);
        };
        let shown = file.for_product(product);
        if latest {
            held.links.insert(
                product.clone(),
                shown.iter().map(|e| e.url.clone()).collect(),
            );
        }
        Ok(NewsState::Ok {
            items: shown
                .into_iter()
                .map(|e| NewsItem {
                    tag: e.tag.to_owned(),
                    title: e.title.clone(),
                    summary: e.summary.clone(),
                    published: e.published.clone(),
                })
                .collect(),
        })
    }

    pub(super) fn news_link(
        &self,
        product: &ProductId,
        index: usize,
    ) -> Result<String, LauncherError> {
        let url = locked(&self.news)?
            .links
            .get(product)
            .and_then(|links| links.get(index))
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "no news item {index} for {}; load news again",
                    product.0
                ))
            })?;
        // Checked again right before it leaves the launcher.
        news::link("news link", url).map_err(|e| invalid(e.to_string()))
    }
}

/// The bundled "About <product>" page, `None` when the product has none.
pub fn product_page(product: &ProductId) -> Option<&'static str> {
    product_pages()
        .iter()
        .find(|(id, _)| *id == product.0)
        .map(|(_, url)| url.as_str())
}

/// Opens a checked link off the async runtime (spawning explorer.exe blocks).
async fn open(launcher: &Arc<Launcher>, url: String) -> Result<(), LauncherError> {
    let launcher = Arc::clone(launcher);
    tauri::async_runtime::spawn_blocking(move || {
        launcher
            .desktop
            .open_link(&url)
            .map_err(|e| LauncherError::new(ErrorCode::Internal, format!("opening {url}: {e}")))
    })
    .await
    .map_err(|e| LauncherError::new(ErrorCode::Internal, e.to_string()))?
}

/// `get_news`: up to three announcements and guides for `product_id`, newest
/// first, from `news.json`. Offline, or a missing, oversized or invalid file,
/// reads `{"status": "offline"}`; it is never an error.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_news(
    launcher: State<'_, Arc<Launcher>>,
    product_id: ProductId,
) -> Result<Versioned<NewsState>, LauncherError> {
    let launcher = Arc::clone(&launcher);
    tauri::async_runtime::spawn_blocking(move || launcher.news_for(&product_id))
        .await
        .map_err(|e| LauncherError::new(ErrorCode::Internal, e.to_string()))?
        .map(Versioned::new)
}

/// `open_news_link`: opens item `index` of the last `get_news` reply for
/// `product_id` in the default browser. Takes no URL; `InvalidRequest` for an
/// index that reply did not have.
#[tauri::command(rename_all = "snake_case")]
pub async fn open_news_link(
    launcher: State<'_, Arc<Launcher>>,
    product_id: ProductId,
    index: usize,
) -> Result<(), LauncherError> {
    let url = launcher.news_link(&product_id, index)?;
    open(&launcher, url).await
}

/// `open_product_page`: opens the bundled "About <product>" page. Takes no URL;
/// `InvalidRequest` for a product without one.
#[tauri::command(rename_all = "snake_case")]
pub async fn open_product_page(
    launcher: State<'_, Arc<Launcher>>,
    product_id: ProductId,
) -> Result<(), LauncherError> {
    let url = product_page(&product_id)
        .ok_or_else(|| invalid(format!("{} has no product page", product_id.0)))?;
    open(&launcher, url.to_owned()).await
}

/// `open_community_page`: opens the bundled project Discord ([`COMMUNITY_URL`]).
/// Takes nothing.
#[tauri::command(rename_all = "snake_case")]
pub async fn open_community_page(launcher: State<'_, Arc<Launcher>>) -> Result<(), LauncherError> {
    open(&launcher, COMMUNITY_URL.to_owned()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_product_pages_pass_the_link_rule() {
        assert!(!product_pages().is_empty());
        for (id, url) in product_pages() {
            assert!(news::link(id, url.clone()).is_ok(), "{id}: {url}");
        }
        assert_eq!(product_page(&ProductId("project-forever".into())), None);
        assert_eq!(product_page(&ProductId("project-09".into())), None);
    }

    #[test]
    fn community_url_passes_the_link_rule() {
        assert!(news::link("community", COMMUNITY_URL.to_owned()).is_ok());
    }

    #[test]
    fn default_news_url_is_public_https() {
        assert!(crate::catalog::public_https("news", DEFAULT_NEWS_URL.to_owned()).is_ok());
    }
}
