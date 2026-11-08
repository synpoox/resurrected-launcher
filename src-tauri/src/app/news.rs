//! News feed and outbound link commands (#80): `get_news`, `open_news_link` and
//! `open_product_page`. The frontend sends product IDs and an index only, never a
//! URL. News links come from the last validated `news.json` reply; product pages
//! are bundled here. Nothing remote is rendered: replies are plain text.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

/// A fetched file is reused for this long within the session; there is no cache
/// on disk, so a launcher started offline shows "News needs a connection".
const NEWS_FRESH: Duration = Duration::from_secs(10 * 60);

/// "About <product>": the product's public page, bundled, never fetched. Only
/// products listed here show the button (`src/products.ts` `aboutPage` mirrors it;
/// `src/products.test.ts` checks). Forever and 09 have no public repository yet.
pub const PRODUCT_PAGES: &[(&str, &str)] =
    &[("pd2r", "https://github.com/synpoox/pd2-resurrected")];

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
    pub published: String,
}

/// The session's news: the last validated file and, per product, the links of the
/// last `get_news` reply in reply order.
#[derive(Default)]
pub(super) struct NewsHeld {
    file: Option<(Instant, Arc<News>)>,
    links: HashMap<ProductId, Vec<String>>,
}

impl Launcher {
    /// The held file while fresh, else a new download from [`Launcher::news_url`].
    fn news(&self) -> Result<Arc<News>, LauncherError> {
        if let Some((at, file)) = &locked(&self.news)?.file {
            if at.elapsed() < NEWS_FRESH {
                return Ok(Arc::clone(file));
            }
        }
        // Downloaded without the lock held.
        let cancel = AtomicBool::new(false);
        let (bytes, _) =
            self.downloader
                .fetch_bytes(&self.news_url, Expect::AtMost(MAX_NEWS_SIZE), &cancel)?;
        let file = Arc::new(
            News::parse(&bytes)
                .map_err(|e| LauncherError::new(ErrorCode::MetadataInvalid, e.to_string()))?,
        );
        locked(&self.news)?.file = Some((Instant::now(), Arc::clone(&file)));
        Ok(file)
    }

    pub(super) fn news_for(&self, product: &ProductId) -> Result<NewsState, LauncherError> {
        let file = match self.news() {
            Ok(file) => file,
            Err(_) => {
                let mut held = locked(&self.news)?;
                held.file = None;
                held.links.remove(product);
                return Ok(NewsState::Offline);
            }
        };
        let shown = file.for_product(product);
        locked(&self.news)?.links.insert(
            product.clone(),
            shown.iter().map(|e| e.url.clone()).collect(),
        );
        Ok(NewsState::Ok {
            items: shown
                .into_iter()
                .map(|e| NewsItem {
                    tag: e.tag.to_owned(),
                    title: e.title.clone(),
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
    PRODUCT_PAGES
        .iter()
        .find(|(id, _)| *id == product.0)
        .map(|(_, url)| *url)
}

fn open_failed(url: &str, e: std::io::Error) -> LauncherError {
    LauncherError::new(ErrorCode::Internal, format!("opening {url}: {e}"))
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
    launcher
        .desktop
        .open_link(&url)
        .map_err(|e| open_failed(&url, e))
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
    launcher
        .desktop
        .open_link(url)
        .map_err(|e| open_failed(url, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_product_pages_pass_the_link_rule() {
        for (id, url) in PRODUCT_PAGES {
            assert!(news::link(id, (*url).to_owned()).is_ok(), "{id}: {url}");
        }
        assert_eq!(product_page(&ProductId("project-forever".into())), None);
        assert_eq!(product_page(&ProductId("project-09".into())), None);
    }

    #[test]
    fn default_news_url_is_public_https() {
        assert!(crate::catalog::public_https("news", DEFAULT_NEWS_URL.to_owned()).is_ok());
    }
}
