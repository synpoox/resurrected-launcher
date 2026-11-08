//! Slice 3 commands (#60): the D2R location, readiness, catalog refresh, loader
//! guidance and Play. The frontend sends product IDs only: the location comes from
//! the native folder picker, the download page from a fixed constant.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime, State};

use super::operations::{locked, Launcher, Release, ReleaseSummary, RunningMap};
use crate::catalog::{refresh_with, Availability, Feed, MAX_FEED_SIZE};
use crate::contract::{
    ErrorCode, GameLocation, LauncherError, LoaderGuidance, OperationKind, ProductId, Readiness,
    RunningGame, Sha256Digest, Versioned, LOADER_DOWNLOAD_PAGE,
};
use crate::game::process::{running_under, wait_until_exited};
use crate::game::readiness::{self, Facts};
use crate::game::{detect, launch, loader_setup};
use crate::install::download::Expect;
use crate::install::journal::journal_path;
use crate::install::plan::product_root;
use crate::state::{InstalledRelease, Preferences};

/// Event (`Versioned<()>`) telling the UI to read `get_product_state` again:
/// startup recovery finished, a catalog refresh ended, or the game exited.
pub const STATE_EVENT: &str = "product-state-changed";

/// How often the exit watcher looks for processes under the D2R root.
const EXIT_POLL: Duration = Duration::from_secs(1);

/// The native UI the commands use. Production: [`SystemDesktop`]; tests a fake.
pub trait Desktop: Send + Sync {
    /// The native folder picker; `None` when the player cancels.
    fn pick_folder(&self) -> Option<PathBuf>;
    /// Opens [`LOADER_DOWNLOAD_PAGE`] in the default browser. No other URL.
    fn open_loader_download_page(&self) -> io::Result<()>;
}

/// The real desktop: the dialog plugin's folder picker (Rust side only; no JS
/// permission is granted) and the Windows shell.
pub struct SystemDesktop(pub AppHandle);

impl Desktop for SystemDesktop {
    fn pick_folder(&self) -> Option<PathBuf> {
        use tauri_plugin_dialog::DialogExt;
        self.0
            .dialog()
            .file()
            .set_title("Choose the Diablo II: Resurrected folder")
            .blocking_pick_folder()?
            .into_path()
            .ok()
    }

    fn open_loader_download_page(&self) -> io::Result<()> {
        // explorer.exe hands an https URL to the default browser; no shell parses it.
        let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        std::process::Command::new(Path::new(&windows).join("explorer.exe"))
            .arg(LOADER_DOWNLOAD_PAGE)
            .spawn()
            .map(drop)
    }
}

/// The last catalog refresh in this launcher run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CatalogState {
    NotRefreshed,
    Refreshing,
    Refreshed,
    /// Offline or invalid metadata. The last valid cached metadata stays in use.
    Failed {
        error: LauncherError,
    },
}

/// Reply to `get_product_state`.
///
/// Offline/no-catalog: `installed` and `recommended` are both null (nothing was
/// ever installed and no valid feed is cached), so there is no release to offer;
/// `catalog` says why. Readiness is then `not_installed` or an earlier reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductState {
    pub product_id: ProductId,
    pub installed: Option<InstalledRelease>,
    /// `None` when the feed or manifest is unavailable or invalid.
    pub recommended: Option<ReleaseSummary>,
    /// The operation running on this product, if any.
    pub running: Option<OperationKind>,
    /// The selected D2R location, `None` when none is selected or it no longer validates.
    pub location: Option<GameLocation>,
    pub readiness: Readiness,
    /// The earliest-started process under the D2R root, any product.
    pub running_game: Option<RunningGame>,
    /// Set when readiness is `loader_missing` or `loader_unsupported`.
    pub loader_guidance: Option<LoaderGuidance>,
    pub catalog: CatalogState,
}

/// What one fresh assessment found.
struct Assessed {
    record: Option<InstalledRelease>,
    location: Option<GameLocation>,
    installed: Option<Release>,
    recommended: Option<Release>,
    readiness: Readiness,
    running: Vec<RunningGame>,
    guidance: Option<LoaderGuidance>,
}

/// Refuses a D2R root that is, or lies inside, a symlink or junction (#66 review):
/// writes would land somewhere other than the folder the player chose.
pub(super) fn refuse_linked_root(d2r: &Path) -> Result<(), LauncherError> {
    let abs = std::path::absolute(d2r).unwrap_or_else(|_| d2r.to_path_buf());
    for dir in abs.ancestors() {
        if std::fs::symlink_metadata(dir).is_ok_and(|m| is_link(&m)) {
            return Err(LauncherError::new(
                ErrorCode::D2rNotFound,
                format!(
                    "{} is a link or junction. Choose the real Diablo II: Resurrected folder.",
                    dir.display()
                ),
            ));
        }
    }
    Ok(())
}

/// Same rule as `game::detect`'s (private) check: any reparse point or symlink.
fn is_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

fn internal(e: impl std::fmt::Display) -> LauncherError {
    LauncherError::new(ErrorCode::Internal, e.to_string())
}

impl Launcher {
    /// Gathers fresh [`Facts`] and assesses `product`. Guidance gets the same
    /// manifest readiness used: the installed one, else the recommended one.
    fn assess(&self, product: &ProductId) -> Result<Assessed, LauncherError> {
        let (remembered, record, installed, recommended) = {
            let mut state = locked(&self.state)?;
            let remembered = state.preferences().d2r_location.clone();
            let record = state.installed_release(product).cloned();
            let installed = match &record {
                Some(r) => Some(self.releases.installed(&mut state, r)?),
                None => None,
            };
            // An unavailable feed only hides the update offer; it never implies readiness.
            let recommended = self.releases.recommended(&mut state, product).ok();
            (remembered, record, installed, recommended)
        };
        let location = remembered.and_then(|p| detect::validate(&p, &*self.game).ok());
        let manifest = installed
            .as_ref()
            .or(recommended.as_ref())
            .map(|r| &r.manifest);
        let running = match &location {
            Some(l) => running_under(&l.path, &*self.game)?,
            None => Vec::new(),
        };
        let journal = match (&location, manifest) {
            (Some(l), Some(m)) => {
                std::fs::symlink_metadata(journal_path(&product_root(&l.path, m))).is_ok()
            }
            _ => false,
        };
        let facts = Facts {
            location: location.as_ref(),
            installed: installed.as_ref().map(|r| &r.manifest),
            recommended: recommended.as_ref().map(|r| &r.manifest),
            // Until startup recovery finished, every product waits for it.
            recovery_pending: journal || !self.recovered.load(Ordering::SeqCst),
            running: &running,
        };
        let readiness = readiness::assess(&facts, &self.hashes, &*self.game)?;
        let guidance = manifest.and_then(|m| loader_setup::guidance(&readiness, m));
        Ok(Assessed {
            record,
            location,
            installed,
            recommended,
            readiness,
            running,
            guidance,
        })
    }

    pub(super) fn state_of(&self, product: &ProductId) -> Result<ProductState, LauncherError> {
        let running = self.running().get(product).map(|(_, kind)| *kind);
        let assessed = self.assess(product)?;
        let catalog = self
            .catalog
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        Ok(ProductState {
            product_id: product.clone(),
            installed: assessed.record,
            recommended: assessed
                .recommended
                .map(|r| ReleaseSummary::of(&r.manifest)),
            running,
            location: assessed.location,
            readiness: assessed.readiness,
            running_game: assessed.running.into_iter().min_by_key(|g| g.since),
            loader_guidance: assessed.guidance,
            catalog,
        })
    }

    /// `OperationInProgress` while any operation runs, `GameRunning` while a game
    /// runs under the selected root.
    fn refuse_location_change(&self, running: &RunningMap) -> Result<(), LauncherError> {
        if let Some(product) = running.keys().next() {
            return Err(LauncherError::new(
                ErrorCode::OperationInProgress,
                format!("{} has an operation running", product.0),
            ));
        }
        let current = locked(&self.state)?.preferences().d2r_location.clone();
        match current {
            Some(d2r) => self.refuse_if_game_running(&d2r),
            None => Ok(()),
        }
    }

    fn set_location(&self, picked: &Path) -> Result<GameLocation, LauncherError> {
        // Held so no operation or Play starts while the location changes.
        let running = self.running();
        self.refuse_location_change(&running)?;
        let location = detect::validate(picked, &*self.game)?;
        refuse_linked_root(&location.path)?;
        let mut state = locked(&self.state)?;
        let prefs = Preferences {
            d2r_location: Some(location.path.clone()),
            ..state.preferences().clone()
        };
        state.set_preferences(prefs)?;
        Ok(location)
    }

    /// Play under the app operation lock: no operation may run, and two Plays
    /// cannot both pass the recheck in [`launch::play`].
    fn play<R: Runtime>(
        self: &Arc<Self>,
        app: &AppHandle<R>,
        product: &ProductId,
    ) -> Result<RunningGame, LauncherError> {
        let running = self.running();
        if let Some((other, (_, kind))) = running.iter().next() {
            return Err(LauncherError::new(
                ErrorCode::OperationInProgress,
                format!("{} is running {kind:?}", other.0),
            ));
        }
        let a = self.assess(product)?;
        let refused = |code| LauncherError::new(code, format!("not playable: {:?}", a.readiness));
        if let Some(code) = a.readiness.play_refusal() {
            return Err(refused(code));
        }
        // Playable readiness implies both; Internal only guards that invariant.
        let (Some(location), Some(installed)) = (&a.location, &a.installed) else {
            return Err(refused(ErrorCode::Internal));
        };
        let game = launch::play(location, &installed.manifest, &a.readiness, &*self.game)?;
        drop(running);
        self.watch_exit(app, location.path.clone());
        Ok(game)
    }

    /// Emits [`STATE_EVENT`] once nothing runs under `root` any more.
    fn watch_exit<R: Runtime>(self: &Arc<Self>, app: &AppHandle<R>, root: PathBuf) {
        let (launcher, app) = (Arc::clone(self), app.clone());
        std::thread::spawn(move || {
            // An enumeration failure ends the watch; the UI then reads the state.
            let _ = wait_until_exited(&root, &*launcher.game, EXIT_POLL);
            let _ = app.emit(STATE_EVENT, Versioned::new(()));
        });
    }

    /// Startup (blocking; `run` calls it on a worker thread): recovers every
    /// installed product, then enables Play, watches a game already running under
    /// the root, and refreshes the catalog in the background.
    pub fn start_up<R: Runtime>(self: &Arc<Self>, app: &AppHandle<R>) {
        let products: Vec<ProductId> = locked(&self.state)
            .map(|s| {
                s.installed_releases()
                    .iter()
                    .map(|r| r.product_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        for product in &products {
            // A failure leaves the journal; readiness then reports recovery_required.
            let _ = self.recover(product);
        }
        self.recovered.store(true, Ordering::SeqCst);
        let root = locked(&self.state)
            .ok()
            .and_then(|s| s.preferences().d2r_location.clone());
        if let Some(root) = root {
            if running_under(&root, &*self.game).is_ok_and(|g| !g.is_empty()) {
                self.watch_exit(app, root);
            }
        }
        let _ = app.emit(STATE_EVENT, Versioned::new(()));
        self.refresh_in_background(app);
    }

    /// Starts a catalog refresh unless one is running; emits [`STATE_EVENT`] when
    /// it ends.
    fn refresh_in_background<R: Runtime>(self: &Arc<Self>, app: &AppHandle<R>) {
        {
            let mut catalog = self.catalog.lock().unwrap_or_else(|e| e.into_inner());
            if *catalog == CatalogState::Refreshing {
                return;
            }
            *catalog = CatalogState::Refreshing;
        }
        let (launcher, app) = (Arc::clone(self), app.clone());
        std::thread::spawn(move || {
            let done = match launcher.refresh_catalog() {
                Ok(()) => CatalogState::Refreshed,
                Err(error) => CatalogState::Failed { error },
            };
            *launcher.catalog.lock().unwrap_or_else(|e| e.into_inner()) = done;
            let _ = app.emit(STATE_EVENT, Versioned::new(()));
        });
    }

    /// [`crate::catalog::refresh`], with every download done before State is
    /// locked so `get_product_state` never waits on the network: the feed and its
    /// manifests are fetched first, then the refresh replays them under the lock.
    fn refresh_catalog(&self) -> Result<(), LauncherError> {
        let cancel = AtomicBool::new(false);
        let fetch =
            |url: &str, expect: Expect<'_>| self.downloader.fetch_bytes(url, expect, &cancel);
        let mut fetched: HashMap<String, (Vec<u8>, Sha256Digest)> = HashMap::new();
        let feed = fetch(&self.feed_url, Expect::AtMost(MAX_FEED_SIZE))?;
        let parsed = Feed::parse(&feed.0)?;
        for product in &parsed.products {
            if let Availability::Available(p) = &product.availability {
                let expect = Expect::Exact {
                    size: p.manifest_size,
                    sha256: &p.manifest_sha256,
                };
                fetched.insert(p.manifest_url.clone(), fetch(&p.manifest_url, expect)?);
            }
        }
        fetched.insert(self.feed_url.clone(), feed);
        let mut state = locked(&self.state)?;
        // Each replayed download was fetched with the Expect this same feed gives it.
        refresh_with(&mut state, &self.feed_url, &mut |url, expect| {
            fetched.remove(url).map_or_else(|| fetch(url, expect), Ok)
        })?;
        Ok(())
    }
}

/// `get_product_state`: installed and recommended releases, the running operation,
/// the selected location, readiness, the running game, loader guidance and the
/// catalog state. Assessed fresh on every call ("Check again").
#[tauri::command(rename_all = "snake_case")]
pub async fn get_product_state(
    launcher: State<'_, Arc<Launcher>>,
    product_id: ProductId,
) -> Result<Versioned<ProductState>, LauncherError> {
    launcher.state_of(&product_id).map(Versioned::new)
}

/// `detect_game_locations`: valid D2R locations, the remembered one first, then
/// Battle.net's. Writes nothing.
#[tauri::command(rename_all = "snake_case")]
pub async fn detect_game_locations(
    launcher: State<'_, Arc<Launcher>>,
) -> Result<Versioned<Vec<GameLocation>>, LauncherError> {
    let remembered = locked(&launcher.state)?.preferences().d2r_location.clone();
    Ok(Versioned::new(detect::candidates(
        remembered.as_deref(),
        &*launcher.game,
    )))
}

/// `choose_game_location`: opens the native folder picker and remembers the
/// validated choice. `None` when the player cancels. Refused with
/// `GameRunning` while a game runs under the current location and
/// `OperationInProgress` while an operation runs; `D2rNotFound` for a folder
/// without a valid `D2R.exe` or behind a link.
#[tauri::command(rename_all = "snake_case")]
pub async fn choose_game_location(
    launcher: State<'_, Arc<Launcher>>,
) -> Result<Versioned<Option<GameLocation>>, LauncherError> {
    launcher.refuse_location_change(&launcher.running())?;
    let picker = Arc::clone(&launcher);
    let picked = tauri::async_runtime::spawn_blocking(move || picker.desktop.pick_folder())
        .await
        .map_err(internal)?;
    match picked {
        Some(path) => launcher
            .set_location(&path)
            .map(|l| Versioned::new(Some(l))),
        None => Ok(Versioned::new(None)),
    }
}

/// `refresh_catalog`: starts a background refresh of the feed and manifests and
/// returns at once; [`STATE_EVENT`] follows, and `get_product_state`'s `catalog`
/// reports the result. Also run at startup.
#[tauri::command(rename_all = "snake_case")]
pub async fn refresh_catalog<R: Runtime>(
    app: AppHandle<R>,
    launcher: State<'_, Arc<Launcher>>,
) -> Result<(), LauncherError> {
    launcher.refresh_in_background(&app);
    Ok(())
}

/// `play_product`: launches the installed release (offline single-player) and
/// returns the started loader process. Refused with readiness's reason
/// (`d2r_not_found`, `game_running`, `recovery_required`, `game_build_unsupported`,
/// `loader_not_ready`, `product_not_installed`), `operation_in_progress`, or
/// `launch_failed`. [`STATE_EVENT`] follows when the game exits.
#[tauri::command(rename_all = "snake_case")]
pub async fn play_product<R: Runtime>(
    app: AppHandle<R>,
    launcher: State<'_, Arc<Launcher>>,
    product_id: ProductId,
) -> Result<Versioned<RunningGame>, LauncherError> {
    launcher.inner().play(&app, &product_id).map(Versioned::new)
}

/// `open_loader_download_page`: opens the official D2RLoader download page
/// ([`LOADER_DOWNLOAD_PAGE`]) in the default browser. Takes no URL.
#[tauri::command(rename_all = "snake_case")]
pub async fn open_loader_download_page(
    launcher: State<'_, Arc<Launcher>>,
) -> Result<(), LauncherError> {
    launcher
        .desktop
        .open_loader_download_page()
        .map_err(|e| internal(format!("opening {LOADER_DOWNLOAD_PAGE}: {e}")))
}
