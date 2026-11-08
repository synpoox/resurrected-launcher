//! Slice 3 commands (#60): the D2R location, readiness, catalog refresh, loader
//! guidance and Play. The frontend sends product IDs only: the location comes from
//! the native folder picker, the download page from a fixed constant.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime, State};

use super::operations::{invalid, locked, Launcher, Release, ReleaseSummary, Running};
use super::winapi::{self, ProcessHandle};
use crate::catalog::{self, Feed, FEED_IDENTITY};
use crate::contract::{
    ErrorCode, GameLocation, LauncherError, LoaderGuidance, OperationKind, ProductId, Readiness,
    RunningGame, Versioned, LOADER_DOWNLOAD_PAGE,
};
use crate::game::process::wait_until_exited;
use crate::game::readiness::{self, Facts};
use crate::game::{detect, launch, loader_setup};
use crate::install::journal::refuse_pending_journal;
use crate::install::plan::product_root;
use crate::state::{InstalledRelease, Preferences};

/// Event (`Versioned<()>`) telling the UI to read `get_product_state` again:
/// startup recovery finished, a catalog refresh ended, the game exited, or the
/// D2R location changed (#123: every view of product state rereads it, not only
/// the one that asked for the change).
pub const STATE_EVENT: &str = "product-state-changed";

/// How often the exit watcher looks for processes under the D2R root.
const EXIT_POLL: Duration = Duration::from_secs(1);

/// A game that exits this soon after Play failed to start.
/// shortcut: a fixed threshold; make it configurable only if real crashes show otherwise.
const STARTUP_WINDOW: Duration = Duration::from_secs(30);

/// The native UI the commands use. Production: [`SystemDesktop`]; tests a fake.
pub trait Desktop: Send + Sync {
    /// The native folder picker; `None` when the player cancels.
    fn pick_folder(&self) -> Option<PathBuf>;
    /// Opens [`LOADER_DOWNLOAD_PAGE`] in the default browser. No other URL.
    fn open_loader_download_page(&self) -> io::Result<()>;
    /// Opens an `https` URL in the default browser: a link from a validated
    /// `news.json` or a bundled product page, never one from the frontend.
    fn open_link(&self, url: &str) -> io::Result<()>;
    /// Opens `dir` in Explorer. Paths are built natively only.
    fn open_folder(&self, dir: &Path) -> io::Result<()>;
    /// Opens Explorer with `file` selected.
    fn show_in_folder(&self, file: &Path) -> io::Result<()>;
    /// The native save dialog for a diagnostics report; `None` when cancelled.
    fn pick_save_file(&self, default_name: &str) -> Option<PathBuf>;
    /// The `Saved Games` known folder.
    fn saved_games(&self) -> io::Result<PathBuf>;
    /// The launcher's value under the per-user Run key, `None` when absent.
    fn run_value(&self) -> io::Result<Option<String>>;
    /// Writes the launcher's Run value, or deletes it for `None`.
    fn set_run_value(&self, value: Option<&str>) -> io::Result<()>;
}

fn explorer() -> std::process::Command {
    let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    std::process::Command::new(Path::new(&windows).join("explorer.exe"))
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
        explorer().arg(LOADER_DOWNLOAD_PAGE).spawn().map(drop)
    }

    fn open_link(&self, url: &str) -> io::Result<()> {
        // As above: explorer.exe hands the URL to the browser; no shell parses it.
        explorer().arg(url).spawn().map(drop)
    }

    fn open_folder(&self, dir: &Path) -> io::Result<()> {
        explorer().arg(dir).spawn().map(drop)
    }

    fn show_in_folder(&self, file: &Path) -> io::Result<()> {
        let mut cmd = explorer();
        // Explorer wants `/select,"<path>"` verbatim; Windows paths cannot hold `"`.
        #[cfg(windows)]
        std::os::windows::process::CommandExt::raw_arg(
            &mut cmd,
            format!("/select,\"{}\"", file.display()),
        );
        cmd.spawn().map(drop)
    }

    fn pick_save_file(&self, default_name: &str) -> Option<PathBuf> {
        use tauri_plugin_dialog::DialogExt;
        self.0
            .dialog()
            .file()
            .set_title("Save diagnostics")
            .set_file_name(default_name)
            .add_filter("Text", &["txt"])
            .blocking_save_file()?
            .into_path()
            .ok()
    }

    fn saved_games(&self) -> io::Result<PathBuf> {
        winapi::saved_games()
    }

    fn run_value(&self) -> io::Result<Option<String>> {
        winapi::hkcu_value(winapi::RUN_KEY, &self.0.config().identifier)
    }

    fn set_run_value(&self, value: Option<&str>) -> io::Result<()> {
        winapi::set_hkcu_value(winapi::RUN_KEY, &self.0.config().identifier, value)
    }
}

/// A [`GameLocation`] as the commands reply with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationView {
    #[serde(flatten)]
    pub location: GameLocation,
    /// Products whose install folder exists under this root.
    pub installed_products: Vec<ProductId>,
}

/// The tracked game exited within [`STARTUP_WINDOW`] of Play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupFailed {
    /// From `GetExitCodeProcess` when the started process could be opened.
    pub exit_code: Option<i32>,
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
    /// A cancel was requested and the running operation has not ended yet.
    pub cancelling: bool,
    /// The selected D2R location, `None` when none is selected or it no longer validates.
    pub location: Option<LocationView>,
    pub readiness: Readiness,
    /// The earliest-started process under the D2R root, any product.
    pub running_game: Option<RunningGame>,
    /// Set when readiness is `loader_missing` or `loader_unsupported`.
    pub loader_guidance: Option<LoaderGuidance>,
    pub catalog: CatalogState,
    /// The last Play's game exited within 30 s; the next Play clears it.
    pub startup_failed: Option<StartupFailed>,
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
        if std::fs::symlink_metadata(dir).is_ok_and(|m| detect::is_link(&m)) {
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
        // A remembered folder later replaced by a link is refused like a picked one (#95).
        let location = remembered
            .and_then(|p| detect::validate(&p, &*self.game).ok())
            .filter(|l| refuse_linked_root(&l.path).is_ok());
        let manifest = installed
            .as_ref()
            .or(recommended.as_ref())
            .map(|r| &r.manifest);
        // A listing failure blocks like a running game (below), never "not running".
        let (running, listing_failed) = match &location {
            Some(l) => match self.games_under(&l.path) {
                Ok(games) => (games, false),
                Err(_) => (Vec::new(), true),
            },
            None => (Vec::new(), false),
        };
        // Fails closed like Play's own recheck: an uncheckable journal is pending.
        let journal = match (&location, manifest) {
            (Some(l), Some(m)) => refuse_pending_journal(&product_root(&l.path, m)).is_err(),
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
        let readiness = match readiness::assess(&facts, &self.hashes, &*self.game) {
            Ok(Readiness::D2rMissing) => Readiness::D2rMissing,
            // shortcut: `since: 0` when the processes could not be listed at all.
            Ok(_) if listing_failed => Readiness::Running { since: 0 },
            Ok(r) => r,
            // D2R.exe could not be hashed: its build is unverified, so not playable.
            Err(_) => Readiness::BuildUnsupported {
                found: location
                    .as_ref()
                    .map(|l| l.build.clone())
                    .unwrap_or_default(),
            },
        };
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

    /// Every product State knows: installed ones, then the cached feed's.
    pub fn known_products(&self) -> Vec<ProductId> {
        let mut products: Vec<ProductId> = Vec::new();
        if let Ok(mut s) = locked(&self.state) {
            products.extend(s.installed_releases().iter().map(|r| r.product_id.clone()));
            let feed = s
                .cached(FEED_IDENTITY)
                .and_then(|c| Feed::parse(&c.bytes).ok());
            for p in feed.into_iter().flat_map(|f| f.products) {
                if !products.contains(&p.product_id) {
                    products.push(p.product_id);
                }
            }
        }
        products
    }

    /// `location` with the known products whose install folder exists under it.
    pub(super) fn view(&self, location: GameLocation) -> LocationView {
        let installed_products = self
            .known_products()
            .into_iter()
            .filter(|p| {
                self.manifest_of(p)
                    .is_ok_and(|r| product_root(&location.path, &r.manifest).is_dir())
            })
            .collect();
        LocationView {
            location,
            installed_products,
        }
    }

    /// The installed release's manifest, else the recommended one.
    pub(super) fn manifest_of(&self, product: &ProductId) -> Result<Release, LauncherError> {
        let mut state = locked(&self.state)?;
        match state.installed_release(product).cloned() {
            Some(record) => self.releases.installed(&mut state, &record),
            None => self.releases.recommended(&mut state, product),
        }
    }

    pub(super) fn state_of(&self, product: &ProductId) -> Result<ProductState, LauncherError> {
        let (running, cancelling) = match self.running().ops.get(product) {
            Some((cancel, kind)) => (Some(*kind), cancel.load(Ordering::Relaxed)),
            None => (None, false),
        };
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
            cancelling,
            location: assessed.location.map(|l| self.view(l)),
            readiness: assessed.readiness,
            running_game: assessed.running.into_iter().min_by_key(|g| g.since),
            loader_guidance: assessed.guidance,
            catalog,
            startup_failed: locked(&self.startup)?.get(product).copied(),
        })
    }

    /// `OperationInProgress` while any operation or recovery runs, `GameRunning`
    /// while a game runs under the selected root.
    fn refuse_location_change(&self, running: &Running) -> Result<(), LauncherError> {
        running.refuse_any()?;
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
        self.refuse_if_game_running(&location.path)?;
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
        running.refuse_any()?;
        let a = self.assess(product)?;
        let refused = |code| LauncherError::new(code, format!("not playable: {:?}", a.readiness));
        if let Some(code) = a.readiness.play_refusal() {
            return Err(refused(code));
        }
        // Playable readiness implies both; Internal only guards that invariant.
        let (Some(location), Some(installed)) = (&a.location, &a.installed) else {
            return Err(refused(ErrorCode::Internal));
        };
        let game = launch::play(location, &installed.manifest, &a.readiness, &*self.game).map_err(
            |e| match e.code {
                // play's only Internal is a process-listing failure: fail closed.
                ErrorCode::Internal => LauncherError::new(ErrorCode::GameRunning, e.message),
                _ => e,
            },
        )?;
        drop(running);
        // Opened at once, so the exit code is still readable after the exit.
        let tracked = Tracked {
            product: product.clone(),
            started: Instant::now(),
            process: ProcessHandle::open(game.pid),
        };
        locked(&self.startup)?.remove(product);
        self.watch_exit(app, location.path.clone(), Some(tracked));
        Ok(game)
    }

    /// Emits [`STATE_EVENT`] once nothing runs under `root` any more. A `tracked`
    /// Play that ended within [`STARTUP_WINDOW`] is recorded as a startup failure.
    fn watch_exit<R: Runtime>(
        self: &Arc<Self>,
        app: &AppHandle<R>,
        root: PathBuf,
        tracked: Option<Tracked>,
    ) {
        let (launcher, app) = (Arc::clone(self), app.clone());
        std::thread::spawn(move || {
            // An enumeration failure ends the watch; the UI then reads the state.
            let exited = wait_until_exited(&root, &*launcher.game, EXIT_POLL).is_ok();
            if let Some(t) = tracked.filter(|t| exited && t.started.elapsed() < STARTUP_WINDOW) {
                let failed = StartupFailed {
                    exit_code: t.process.and_then(|p| p.exit_code()),
                };
                if let Ok(mut startup) = locked(&launcher.startup) {
                    startup.insert(t.product, failed);
                }
            }
            let _ = app.emit(STATE_EVENT, Versioned::new(()));
        });
    }

    /// Startup (blocking; `run` calls it on a worker thread): recovers every
    /// installed product and every product of the cached feed (an interrupted
    /// first install has a journal but no record), then enables Play, watches a
    /// game already running under the root, and refreshes the catalog in the
    /// background.
    pub fn start_up<R: Runtime>(self: &Arc<Self>, app: &AppHandle<R>) {
        for product in &self.known_products() {
            // A failure leaves the journal; readiness then reports recovery_required.
            let _ = self.recover(product);
        }
        self.recovered.store(true, Ordering::SeqCst);
        let root = locked(&self.state)
            .ok()
            .and_then(|s| s.preferences().d2r_location.clone());
        if let Some(root) = root {
            if self.games_under(&root).is_ok_and(|g| !g.is_empty()) {
                self.watch_exit(app, root, None);
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

    /// [`crate::catalog::refresh`] with its downloads ([`catalog::fetch`]) done
    /// before State is locked, so `get_product_state` never waits on the network;
    /// only [`catalog::apply`] runs under the lock.
    fn refresh_catalog(&self) -> Result<(), LauncherError> {
        let cancel = AtomicBool::new(false);
        let fetched = catalog::fetch(
            &self.feed_url,
            &mut |url, expect| self.downloader.fetch_bytes(url, expect, &cancel),
            &mut |identity| {
                let mut state = locked(&self.state).ok()?;
                state.cached(identity).map(|c| c.sha256)
            },
        )?;
        catalog::apply(&mut *locked(&self.state)?, fetched)?;
        Ok(())
    }
}

/// A Play whose game the exit watcher times against [`STARTUP_WINDOW`].
pub(super) struct Tracked {
    product: ProductId,
    started: Instant,
    process: Option<ProcessHandle>,
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
/// Battle.net's. Writes nothing; `select_game_location` picks from this reply.
#[tauri::command(rename_all = "snake_case")]
pub async fn detect_game_locations(
    launcher: State<'_, Arc<Launcher>>,
) -> Result<Versioned<Vec<LocationView>>, LauncherError> {
    let remembered = locked(&launcher.state)?.preferences().d2r_location.clone();
    let found = detect::candidates(remembered.as_deref(), &*launcher.game);
    *locked(&launcher.detected)? = found.clone();
    Ok(Versioned::new(
        found.into_iter().map(|l| launcher.view(l)).collect(),
    ))
}

/// `select_game_location`: remembers entry `index` of the latest
/// `detect_game_locations` reply, validated again. `InvalidRequest` for an index
/// out of range (or before any detection); otherwise the same refusals as
/// `choose_game_location`, including `GameRunning` under either location.
/// [`STATE_EVENT`] follows a remembered choice.
#[tauri::command(rename_all = "snake_case")]
pub async fn select_game_location<R: Runtime>(
    app: AppHandle<R>,
    launcher: State<'_, Arc<Launcher>>,
    index: usize,
) -> Result<Versioned<LocationView>, LauncherError> {
    let picked = locked(&launcher.detected)?
        .get(index)
        .map(|l| l.path.clone())
        .ok_or_else(|| invalid(format!("no detected location {index}; detect again")))?;
    let location = launcher.set_location(&picked)?;
    location_changed(&app);
    Ok(Versioned::new(launcher.view(location)))
}

/// A remembered location changed: tells every view to reread product state.
fn location_changed<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit(STATE_EVENT, Versioned::new(()));
}

/// `choose_game_location`: opens the native folder picker and remembers the
/// validated choice. `None` when the player cancels. Refused with
/// `GameRunning` while a game runs under the current location and
/// `OperationInProgress` while an operation runs; `D2rNotFound` for a folder
/// without a valid `D2R.exe` or behind a link. [`STATE_EVENT`] follows a
/// remembered choice.
#[tauri::command(rename_all = "snake_case")]
pub async fn choose_game_location<R: Runtime>(
    app: AppHandle<R>,
    launcher: State<'_, Arc<Launcher>>,
) -> Result<Versioned<Option<LocationView>>, LauncherError> {
    launcher.refuse_location_change(&launcher.running())?;
    let picker = Arc::clone(&launcher);
    let picked = tauri::async_runtime::spawn_blocking(move || picker.desktop.pick_folder())
        .await
        .map_err(internal)?;
    let Some(path) = picked else {
        return Ok(Versioned::new(None));
    };
    let location = launcher.set_location(&path)?;
    location_changed(&app);
    Ok(Versioned::new(Some(launcher.view(location))))
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
