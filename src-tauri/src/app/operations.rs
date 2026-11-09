//! Install operation commands (#40): one command per user intent, built on
//! [`crate::install::ops`]. The frontend sends product IDs, operation kinds,
//! consents and the token of the inspection it showed; everything else (the D2R
//! location, release manifests, the downloader, 7-Zip) is native state.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::catalog::DEFAULT_FEED_URL;
use crate::catalog::{Availability, Feed, ReleaseManifest};
use crate::contract::{
    Boundary, CancelRequest, ErrorCode, FaultInjector, FaultPoint, GameLocation, InjectedFault,
    LauncherError, NoFaults, OperationKind, OperationOutcome, OperationRequest, OutcomeReport,
    Phase, ProductId, ProgressEvent, RelativePath, RunningGame, Sha256Digest, Versioned,
};
use crate::game::process::running_under;
use crate::game::readiness::BuildHashCache;
use crate::game::GameHost;
use crate::install::download::Downloader;
use crate::install::extract::SevenZip;
use crate::install::ops::{self, Env};
use crate::install::plan::{self, Conflict, Inspection, Request};
use crate::install::preflight::Host;
use crate::install::transaction::Recovery;
use crate::state::{InstalledRelease, StateStore};

use super::game::{CatalogState, Desktop, StartupFailed};
use super::settings::Exported;

/// Byte and file counts are emitted at most this often, plus the final one.
const COUNT_INTERVAL: Duration = Duration::from_millis(100);

/// How many recent operation errors diagnostics keep.
const RECENT_ERRORS: usize = 10;

/// Event carrying `Versioned<ProgressEvent>` while an operation runs.
pub const PROGRESS_EVENT: &str = "operation-progress";
/// Event carrying the final `Versioned<OperationOutcome>` of an operation.
pub const OUTCOME_EVENT: &str = "operation-outcome";

/// A validated release manifest and the SHA-256 of its bytes.
#[derive(Debug, Clone)]
pub struct Release {
    pub manifest: ReleaseManifest,
    pub sha256: Sha256Digest,
}

/// Where commands get release manifests. Production: [`CachedReleases`]; tests
/// supply manifests whose archives point at the fixture server.
pub trait Releases: Send + Sync {
    /// The release the feed recommends for `product`.
    fn recommended(
        &self,
        state: &mut StateStore,
        product: &ProductId,
    ) -> Result<Release, LauncherError>;
    /// The manifest of the release `record` says is installed.
    fn installed(
        &self,
        state: &mut StateStore,
        record: &InstalledRelease,
    ) -> Result<Release, LauncherError>;
    /// Every other release manifest of `product` the launcher holds, except
    /// `release_id`: clean install's evidence of what older releases shipped
    /// (#134). Unreadable or invalid ones are skipped.
    fn earlier(
        &self,
        _state: &mut StateStore,
        _product: &ProductId,
        _release_id: &str,
    ) -> Vec<ReleaseManifest> {
        Vec::new()
    }
}

/// Reads the feed (`feed`) and manifests (`manifest/<product>/<release>`) from
/// State's metadata cache, re-validating them on every read.
pub struct CachedReleases;

fn metadata(why: String) -> LauncherError {
    LauncherError::new(ErrorCode::MetadataInvalid, why)
}

pub(super) fn invalid(why: impl Into<String>) -> LauncherError {
    LauncherError::new(ErrorCode::InvalidRequest, why)
}

fn cached_manifest(
    state: &mut StateStore,
    product: &ProductId,
    release_id: &str,
    sha256: &Sha256Digest,
) -> Result<Release, LauncherError> {
    let identity = format!("manifest/{}/{release_id}", product.0);
    let cached = state
        .cached(&identity)
        .filter(|c| &c.sha256 == sha256)
        .ok_or_else(|| metadata(format!("release manifest {release_id} is not available")))?;
    let manifest = ReleaseManifest::parse(&cached.bytes)?;
    if &manifest.product_id != product || manifest.release_id != release_id {
        return Err(metadata(format!("cached manifest is not {release_id}")));
    }
    Ok(Release {
        manifest,
        sha256: cached.sha256,
    })
}

impl Releases for CachedReleases {
    fn recommended(
        &self,
        state: &mut StateStore,
        product: &ProductId,
    ) -> Result<Release, LauncherError> {
        let bytes = state
            .cached("feed")
            .ok_or_else(|| metadata("the release feed is not available".into()))?
            .bytes;
        let feed = Feed::parse(&bytes)?;
        let entry = feed.product(product).ok_or_else(|| {
            LauncherError::new(
                ErrorCode::UnknownProduct,
                format!("{} is unknown", product.0),
            )
        })?;
        let Availability::Available(pointer) = &entry.availability else {
            return Err(invalid(format!("{} is coming soon", product.0)));
        };
        let release = cached_manifest(
            state,
            product,
            &pointer.release_id,
            &pointer.manifest_sha256,
        )?;
        entry.check_manifest(&release.manifest)?;
        Ok(release)
    }

    fn installed(
        &self,
        state: &mut StateStore,
        record: &InstalledRelease,
    ) -> Result<Release, LauncherError> {
        cached_manifest(
            state,
            &record.product_id,
            &record.release_id,
            &record.manifest_sha256,
        )
    }

    fn earlier(
        &self,
        state: &mut StateStore,
        product: &ProductId,
        release_id: &str,
    ) -> Vec<ReleaseManifest> {
        let prefix = format!("manifest/{}/", product.0);
        let mut out = Vec::new();
        for identity in state.cached_identities(&prefix) {
            if identity[prefix.len()..] == *release_id {
                continue;
            }
            let Some(cached) = state.cached(&identity) else {
                continue;
            };
            match ReleaseManifest::parse(&cached.bytes) {
                Ok(m) if &m.product_id == product => out.push(m),
                _ => {}
            }
        }
        out
    }
}

/// Native state behind the commands, managed by Tauri as `Arc<Launcher>`.
pub struct Launcher {
    pub state: Mutex<StateStore>,
    pub releases: Box<dyn Releases>,
    pub downloader: Downloader,
    pub seven_zip: SevenZip,
    pub host: Box<dyn Host>,
    pub game: Box<dyn GameHost>,
    pub(super) desktop: Box<dyn Desktop>,
    /// Where `refresh_catalog` downloads the feed: [`DEFAULT_FEED_URL`] unless a
    /// test points it elsewhere.
    pub feed_url: String,
    /// Where `get_news` downloads `news.json`: [`super::news::DEFAULT_NEWS_URL`]
    /// unless a test points it elsewhere.
    pub news_url: String,
    /// The session's news file and the links of the last replies.
    pub(super) news: Mutex<super::news::NewsHeld>,
    /// Running operations and recoveries. Also the app operation lock:
    /// `start_operation` and recovery reserve under it, and `play_product` holds
    /// it from assessment to spawn.
    running: Mutex<Running>,
    pub(super) hashes: BuildHashCache,
    pub(super) catalog: Mutex<CatalogState>,
    /// Set once startup recovery finished; Play is refused before.
    pub(super) recovered: AtomicBool,
    /// Exit watchers running now. The game watcher announces a game only while
    /// none runs, so a Play (which starts its own) is not announced twice.
    pub(super) exit_watches: AtomicUsize,
    /// The last `detect_game_locations` reply, for `select_game_location`.
    pub(super) detected: Mutex<Vec<GameLocation>>,
    /// Products whose last Play ended within the startup window.
    pub(super) startup: Mutex<HashMap<ProductId, StartupFailed>>,
    /// Recent operation errors, oldest first, for diagnostics.
    pub(super) recent_errors: Mutex<Vec<LauncherError>>,
    /// The last previewed and the last saved diagnostics report.
    pub(super) exported: Mutex<Exported>,
}

/// What runs per product, behind the app operation lock.
#[derive(Default)]
pub(super) struct Running {
    /// Started operations: cancel flag and kind.
    pub(super) ops: HashMap<ProductId, (Arc<AtomicBool>, OperationKind)>,
    /// Products whose restart recovery is running.
    pub(super) recovering: HashSet<ProductId>,
    /// Set for good by Quit: `AppHandle::exit` only requests the exit, so nothing
    /// may start while the event loop winds down.
    pub(super) quitting: bool,
}

fn quitting() -> LauncherError {
    LauncherError::new(ErrorCode::OperationInProgress, "the launcher is quitting")
}

impl Running {
    /// [`ErrorCode::OperationInProgress`] while `product` has an operation or
    /// recovery running, or the launcher is quitting.
    fn refuse(&self, product: &ProductId) -> Result<(), LauncherError> {
        if self.quitting {
            return Err(quitting());
        }
        if self.ops.contains_key(product) || self.recovering.contains(product) {
            return Err(LauncherError::new(
                ErrorCode::OperationInProgress,
                format!("{} already has an operation running", product.0),
            ));
        }
        Ok(())
    }

    /// [`ErrorCode::OperationInProgress`] while any operation or recovery runs,
    /// or the launcher is quitting. The refusal names the busy product.
    pub(super) fn refuse_any(&self) -> Result<(), Refusal> {
        if self.quitting {
            return Err(Refusal {
                error: quitting(),
                product: None,
            });
        }
        match self.ops.keys().chain(&self.recovering).next() {
            Some(p) => Err(Refusal {
                error: LauncherError::new(
                    ErrorCode::OperationInProgress,
                    format!("{} has an operation or recovery running", p.0),
                ),
                product: Some(p.clone()),
            }),
            None => Ok(()),
        }
    }
}

/// Why [`Running::refuse_any`] (and so [`Launcher::begin_quit`]) refused.
#[derive(Debug)]
pub struct Refusal {
    pub error: LauncherError,
    /// The product with an operation or recovery running; `None` when the
    /// launcher is already quitting. Tray Quit asks about it (#75).
    pub product: Option<ProductId>,
}

impl From<Refusal> for LauncherError {
    fn from(r: Refusal) -> Self {
        r.error
    }
}

/// The install host plus whole-process tracking: every preflight (and its
/// recheck before the first live mutation) also refuses with `GameRunning` while
/// any process runs under the D2R root.
struct GameAware<'a> {
    host: &'a dyn Host,
    game: &'a dyn GameHost,
    d2r: &'a Path,
}

impl Host for GameAware<'_> {
    fn free_space(&self, dir: &Path) -> std::io::Result<u64> {
        self.host.free_space(dir)
    }

    fn processes_using(&self, files: &[PathBuf]) -> Vec<String> {
        let mut found = self.host.processes_using(files);
        match running_under(self.d2r, self.game) {
            Ok(games) => found.extend(games.iter().map(describe)),
            // Fail closed: an enumeration failure counts as a running game.
            Err(e) => found.push(e.message),
        }
        found
    }
}

pub(super) fn describe(g: &RunningGame) -> String {
    format!("{} (pid {}) is running", g.image.display(), g.pid)
}

pub(super) fn locked<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, LauncherError> {
    m.lock()
        .map_err(|_| LauncherError::new(ErrorCode::Internal, "lock poisoned"))
}

/// Everything an [`ops`] request borrows, resolved from native state.
struct Resolved {
    kind: OperationKind,
    d2r: PathBuf,
    release: Release,
    installed: Option<Release>,
    earlier: Vec<ReleaseManifest>,
    acknowledge_save_breaking: bool,
    consent_clean_install: bool,
}

impl Resolved {
    fn request(&self) -> Request<'_> {
        Request {
            kind: self.kind,
            d2r: &self.d2r,
            release: &self.release.manifest,
            release_sha256: self.release.sha256,
            installed: self.installed.as_ref().map(|r| &r.manifest),
            earlier: &self.earlier,
            acknowledge_save_breaking: self.acknowledge_save_breaking,
            consent_clean_install: self.consent_clean_install,
        }
    }
}

impl Launcher {
    pub fn new(
        state: StateStore,
        releases: Box<dyn Releases>,
        downloader: Downloader,
        seven_zip: SevenZip,
        host: Box<dyn Host>,
        game: Box<dyn GameHost>,
        desktop: Box<dyn Desktop>,
    ) -> Self {
        Self {
            state: Mutex::new(state),
            releases,
            downloader,
            seven_zip,
            host,
            game,
            desktop,
            feed_url: DEFAULT_FEED_URL.to_owned(),
            news_url: super::news::DEFAULT_NEWS_URL.to_owned(),
            news: Mutex::default(),
            running: Mutex::new(Running::default()),
            hashes: BuildHashCache::default(),
            catalog: Mutex::new(CatalogState::NotRefreshed),
            recovered: AtomicBool::new(false),
            exit_watches: AtomicUsize::new(0),
            detected: Mutex::default(),
            startup: Mutex::default(),
            recent_errors: Mutex::default(),
            exported: Mutex::default(),
        }
    }

    fn note_error(&self, error: &LauncherError) {
        let mut recent = self.recent_errors.lock().unwrap_or_else(|e| e.into_inner());
        recent.push(error.clone());
        let excess = recent.len().saturating_sub(RECENT_ERRORS);
        recent.drain(..excess);
    }

    fn env<'a>(
        &'a self,
        host: &'a dyn Host,
        injector: &'a dyn FaultInjector,
        cancel: &'a AtomicBool,
    ) -> Env<'a> {
        Env {
            downloader: &self.downloader,
            seven_zip: &self.seven_zip,
            state: &self.state,
            host,
            injector,
            cancel,
        }
    }

    /// The running map. Entries are only added and removed whole (removal by drop
    /// guards), so a poisoned lock is still consistent.
    pub(super) fn running(&self) -> MutexGuard<'_, Running> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The processes under `d2r`. A listing failure is [`ErrorCode::GameRunning`]:
    /// fails closed, never "nothing runs".
    pub(super) fn games_under(&self, d2r: &Path) -> Result<Vec<RunningGame>, LauncherError> {
        running_under(d2r, &*self.game).map_err(|e| {
            LauncherError::new(
                ErrorCode::GameRunning,
                format!("cannot tell whether the game is running: {}", e.message),
            )
        })
    }

    /// [`ErrorCode::GameRunning`] while any process runs under `d2r`, or when that
    /// cannot be listed.
    pub(super) fn refuse_if_game_running(&self, d2r: &Path) -> Result<(), LauncherError> {
        match self.games_under(d2r)?.first() {
            Some(g) => Err(LauncherError::new(ErrorCode::GameRunning, describe(g))),
            None => Ok(()),
        }
    }

    /// The selected D2R root, refused when it or a parent folder is a link.
    pub(super) fn d2r_root(&self, state: &StateStore) -> Result<PathBuf, LauncherError> {
        let d2r = state
            .preferences()
            .d2r_location
            .clone()
            .ok_or_else(|| invalid("no D2R location is selected"))?;
        super::game::refuse_linked_root(&d2r)?;
        Ok(d2r)
    }

    /// The D2R location from Preferences and the manifests `req` acts on: the
    /// recommended release for install, adopt, update and clean install; the
    /// installed one for repair and uninstall.
    fn resolve(&self, req: &OperationRequest) -> Result<Resolved, LauncherError> {
        use OperationKind::*;
        let mut state = locked(&self.state)?;
        let d2r = self.d2r_root(&state)?;
        let record = state.installed_release(&req.product_id).cloned();
        let installed = match &record {
            Some(r) => Some(self.releases.installed(&mut state, r)?),
            None => None,
        };
        let release = match req.kind {
            Repair | Uninstall => installed
                .clone()
                .ok_or_else(|| invalid(format!("{} is not installed", req.product_id.0)))?,
            Install | Adopt | Update | CleanInstall => {
                self.releases.recommended(&mut state, &req.product_id)?
            }
        };
        let earlier = if req.kind == CleanInstall {
            let id = &release.manifest.release_id;
            self.releases.earlier(&mut state, &req.product_id, id)
        } else {
            Vec::new()
        };
        Ok(Resolved {
            kind: req.kind,
            d2r,
            release,
            installed,
            earlier,
            acknowledge_save_breaking: req.acknowledge_save_breaking,
            consent_clean_install: req.consent_clean_install,
        })
    }

    fn inspect(&self, resolved: &Resolved) -> Result<(Inspection, String), LauncherError> {
        let req = resolved.request();
        let cancel = AtomicBool::new(false);
        let insp = ops::inspect(&self.env(&*self.host, &NoFaults, &cancel), &req)?;
        let root = plan::product_root(req.d2r, req.release);
        // Everything the plan decides, but not `plan.record`: its `installed_at` is
        // the inspection's wall-clock time. Debug of these Vec-only types is
        // deterministic within one build, and tokens never outlive the process.
        let p = &insp.plan;
        let stable = (
            (&root, req.release_sha256.to_string(), p.kind),
            (
                &p.to_backup,
                &p.to_live,
                &p.carry_forward,
                &p.prune_if_empty,
                p.space,
            ),
            (&insp.preserved, &insp.backup_only, &insp.conflicts),
            &insp.settings_reset,
            (&insp.save_breaking_notice, &insp.expect),
        );
        let digest = Sha256::digest(format!("{stable:?}"));
        let token = digest.iter().map(|b| format!("{b:02x}")).collect();
        Ok((insp, token))
    }
}

/// A release as the UI names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseSummary {
    pub release_id: String,
    pub display_version: String,
    pub save_breaking: bool,
}

impl ReleaseSummary {
    pub(super) fn of(m: &ReleaseManifest) -> Self {
        Self {
            release_id: m.release_id.clone(),
            display_version: m.display_version.clone(),
            save_breaking: m.is_save_breaking(),
        }
    }
}

/// Why an operation would pause; mirrors [`Conflict`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConflictView {
    Setting {
        file: RelativePath,
        key: String,
        required: String,
        found: Option<String>,
    },
    UnknownFile {
        path: RelativePath,
    },
}

/// Reply to `inspect_operation`: what the operation would do. `start_operation`
/// runs only with this `token`, and refuses if a fresh inspection differs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionView {
    pub product_id: ProductId,
    pub kind: OperationKind,
    pub release: ReleaseSummary,
    /// Managed files replaced or removed (moved to the recovery copy).
    pub files_replaced_or_removed: usize,
    /// Release files put in place.
    pub files_added: usize,
    /// Release files already in place and left as they are (#126): with no
    /// installed release, an existing installation the launcher did not make.
    pub files_kept: usize,
    pub preserved: Vec<RelativePath>,
    /// Clean install: files that stay only in the labelled backup.
    pub backup_only: Vec<RelativePath>,
    /// Clean install: mandatory settings an edited default in `backup_only`
    /// fails; the release's default goes live in its place (#134).
    pub settings_reset: Vec<ConflictView>,
    /// Non-empty: start is refused until resolved.
    pub conflicts: Vec<ConflictView>,
    pub save_breaking_notice: Option<String>,
    pub space_needed: u64,
    pub token: String,
}

impl InspectionView {
    fn of(req: &OperationRequest, release: &Release, insp: Inspection, token: String) -> Self {
        Self {
            product_id: req.product_id.clone(),
            kind: req.kind,
            release: ReleaseSummary::of(&release.manifest),
            files_replaced_or_removed: insp.plan.to_backup.len(),
            files_added: insp.plan.to_live.len(),
            files_kept: insp.kept,
            preserved: insp.preserved,
            backup_only: insp.backup_only,
            settings_reset: insp.settings_reset.into_iter().map(view).collect(),
            conflicts: insp.conflicts.into_iter().map(view).collect(),
            save_breaking_notice: insp.save_breaking_notice,
            space_needed: insp.plan.space.total(),
            token,
        }
    }
}

fn view(c: Conflict) -> ConflictView {
    match c {
        Conflict::Setting {
            file,
            key,
            required,
            found,
        } => ConflictView::Setting {
            file,
            key,
            required,
            found,
        },
        Conflict::UnknownFile { path } => ConflictView::UnknownFile { path },
    }
}

/// Reply to `recover_product`; mirrors [`Recovery`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryView {
    Nothing,
    RolledBack,
    RolledForward,
}

/// Turns the transaction's boundary checks and counts into progress events;
/// never injects.
struct Progress<F> {
    product_id: ProductId,
    kind: OperationKind,
    reached: Mutex<Option<Boundary>>,
    /// When a count was last emitted, for [`COUNT_INTERVAL`].
    counted: Mutex<Option<Instant>>,
    emit: F,
}

impl<F: Fn(ProgressEvent)> Progress<F> {
    fn new(product_id: ProductId, kind: OperationKind, emit: F) -> Self {
        Self {
            product_id,
            kind,
            reached: Mutex::new(None),
            counted: Mutex::new(None),
            emit,
        }
    }

    fn event(&self, phase: Phase, boundary: Option<Boundary>) -> ProgressEvent {
        ProgressEvent {
            product_id: self.product_id.clone(),
            kind: self.kind,
            phase,
            boundary,
            bytes_done: None,
            bytes_total: None,
            files_done: None,
            files_total: None,
        }
    }

    /// Emits `(done, total)` as bytes or files, rate-limited except the final one.
    fn count(&self, phase: Phase, done: u64, total: u64, files: bool) {
        let boundary = match self.reached.lock() {
            Ok(r) => *r,
            Err(_) => return,
        };
        let Ok(mut counted) = self.counted.lock() else {
            return;
        };
        let recent = counted.is_some_and(|t| t.elapsed() < COUNT_INTERVAL);
        // An unknown total (0) is rate-limited throughout.
        if recent && (total == 0 || done < total) {
            return;
        }
        *counted = Some(Instant::now());
        let mut e = self.event(phase, boundary);
        if files {
            (e.files_done, e.files_total) = (Some(done), Some(total));
        } else {
            (e.bytes_done, e.bytes_total) = (Some(done), Some(total));
        }
        (self.emit)(e)
    }
}

fn phase(b: Boundary) -> Phase {
    use Boundary::*;
    match b {
        TransactionOpened => Phase::Planning,
        DownloadComplete => Phase::Downloading,
        Staged => Phase::Extracting,
        StagedInventoryVerified | LiveVerified => Phase::Verifying,
        LiveMovedToBackup
        | CleanInstallBackupMade
        | StagedMovedToLive
        | PlayerFilesCarriedForward => Phase::Activating,
        InstalledRecordCommitted => Phase::Committing,
        CleanedUp => Phase::CleaningUp,
        RollbackStarted | NewEntriesRemovedFromLive | BackupRestoredToLive | RollbackVerified => {
            Phase::Recovering
        }
    }
}

impl<F: Fn(ProgressEvent) + Send + Sync> FaultInjector for Progress<F> {
    fn check(&self, b: Boundary, point: FaultPoint) -> Result<(), InjectedFault> {
        let Ok(mut reached) = self.reached.lock() else {
            return Ok(());
        };
        match point {
            FaultPoint::BeforeStep => (self.emit)(self.event(phase(b), *reached)),
            FaultPoint::AfterStep => *reached = Some(b),
            _ => {}
        }
        Ok(())
    }

    fn bytes(&self, done: u64, total: u64) {
        self.count(Phase::Downloading, done, total, false)
    }

    fn files(&self, done: u64, total: u64) {
        self.count(Phase::Verifying, done, total, true)
    }
}

/// `inspect_operation`: the plan the player confirms before `start_operation`.
/// Consents in the request are ignored here; `start_operation` checks them.
#[tauri::command(rename_all = "snake_case")]
pub async fn inspect_operation(
    launcher: State<'_, Arc<Launcher>>,
    request: Versioned<OperationRequest>,
) -> Result<Versioned<InspectionView>, LauncherError> {
    let req = request.into_current()?;
    let resolved = launcher.resolve(&req)?;
    let (insp, token) = launcher.inspect(&resolved)?;
    Ok(Versioned::new(InspectionView::of(
        &req,
        &resolved.release,
        insp,
        token,
    )))
}

/// `start_operation`: runs the inspected operation on a worker thread. Returns
/// once it started; progress and the outcome arrive as [`PROGRESS_EVENT`] and
/// [`OUTCOME_EVENT`]. Refused with [`ErrorCode::InventoryMismatch`] when a fresh
/// inspection no longer matches `inspection_token`, and with
/// [`ErrorCode::OperationInProgress`] while this product has one running.
#[tauri::command(rename_all = "snake_case")]
pub async fn start_operation<R: Runtime>(
    app: AppHandle<R>,
    launcher: State<'_, Arc<Launcher>>,
    request: Versioned<OperationRequest>,
    inspection_token: String,
) -> Result<(), LauncherError> {
    let req = request.into_current()?;
    let cancel = Arc::new(AtomicBool::new(false));
    // Reserved under the app operation lock before anything is read, so two
    // starts cannot both pass and no Play or location change starts from here on.
    let reservation = {
        let mut running = launcher.running();
        running.refuse(&req.product_id)?;
        running
            .ops
            .insert(req.product_id.clone(), (cancel.clone(), req.kind));
        Reservation {
            launcher: Arc::clone(&launcher),
            product_id: req.product_id.clone(),
        }
    };
    // While reserved the selected D2R root cannot change (#97), so the root
    // resolved and checked here is the one the operation runs on.
    let resolved = launcher.resolve(&req)?;
    launcher.refuse_if_game_running(&resolved.d2r)?;
    match launcher.inspect(&resolved) {
        Ok((_, token)) if token == inspection_token => {}
        Ok(_) => {
            return Err(LauncherError::new(
                ErrorCode::InventoryMismatch,
                "the product changed since it was inspected; inspect again",
            ))
        }
        Err(e) => return Err(e),
    }
    let launcher = Arc::clone(&launcher);
    let mut finish = Finish {
        app,
        reservation: Some(reservation),
        product_id: req.product_id,
        kind: req.kind,
        outcome: None,
    };
    std::thread::spawn(move || {
        let (product_id, kind) = (finish.product_id.clone(), finish.kind);
        let app = finish.app.clone();
        let host = GameAware {
            host: &*launcher.host,
            game: &*launcher.game,
            d2r: &resolved.d2r,
        };
        let progress = Progress::new(product_id.clone(), kind, move |e| {
            let _ = app.emit(PROGRESS_EVENT, Versioned::new(e));
        });
        let result = ops::run(
            &launcher.env(&host, &progress, &cancel),
            &resolved.request(),
        );
        finish.outcome = Some(match result {
            Ok(r) => OperationOutcome::Succeeded {
                product_id,
                kind,
                report: OutcomeReport {
                    preserved: r.preserved,
                    remaining: r.remaining,
                    clean_install_backups: r.clean_install_backups,
                },
            },
            Err(e) if e.code == ErrorCode::Cancelled => {
                OperationOutcome::Cancelled { product_id, kind }
            }
            Err(error) => {
                launcher.note_error(&error);
                OperationOutcome::Failed {
                    product_id,
                    kind,
                    error,
                }
            }
        });
    });
    Ok(())
}

/// Frees a product's `running` entry when dropped, so a panic or error between
/// reserving and starting never leaves it reserved (#53 item 6).
struct Reservation {
    launcher: Arc<Launcher>,
    product_id: ProductId,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.launcher.running().ops.remove(&self.product_id);
    }
}

/// Ends a started operation even if it panics: frees the product, then emits the
/// outcome (`Failed` with `Internal` when none was recorded).
struct Finish<R: Runtime> {
    app: AppHandle<R>,
    reservation: Option<Reservation>,
    product_id: ProductId,
    kind: OperationKind,
    outcome: Option<OperationOutcome>,
}

impl<R: Runtime> Drop for Finish<R> {
    fn drop(&mut self) {
        // Freed before the outcome, so state read on the outcome is not running.
        drop(self.reservation.take());
        let outcome = self
            .outcome
            .take()
            .unwrap_or_else(|| OperationOutcome::Failed {
                product_id: self.product_id.clone(),
                kind: self.kind,
                error: LauncherError::new(
                    ErrorCode::Internal,
                    "the operation stopped unexpectedly",
                ),
            });
        let _ = self.app.emit(OUTCOME_EVENT, Versioned::new(outcome));
    }
}

/// `cancel_operation`: asks the running operation to stop. During activation it
/// rolls back safely; the outcome event reports when it has.
#[tauri::command(rename_all = "snake_case")]
pub async fn cancel_operation(
    launcher: State<'_, Arc<Launcher>>,
    request: Versioned<CancelRequest>,
) -> Result<(), LauncherError> {
    let req = request.into_current()?;
    let running = launcher.running();
    let (cancel, _) = running
        .ops
        .get(&req.product_id)
        .ok_or_else(|| invalid(format!("{} has no operation running", req.product_id.0)))?;
    cancel.store(true, Ordering::Relaxed);
    Ok(())
}

/// `recover_product`: startup recovery before Play is enabled, and the way out of
/// [`ErrorCode::RecoveryRequired`]. Refused with [`ErrorCode::GameRunning`] while
/// any process runs under the D2R root.
#[tauri::command(rename_all = "snake_case")]
pub async fn recover_product(
    launcher: State<'_, Arc<Launcher>>,
    product_id: ProductId,
) -> Result<Versioned<RecoveryView>, LauncherError> {
    launcher.recover(&product_id).map(Versioned::new)
}

impl Launcher {
    /// Reserves `product_id` as recovering under the app operation lock (so no
    /// Play, location change or operation on it starts meanwhile), then recovers.
    pub(super) fn recover(&self, product_id: &ProductId) -> Result<RecoveryView, LauncherError> {
        let d2r = {
            let mut running = self.running();
            running.refuse(product_id)?;
            let d2r = self.d2r_root(&*locked(&self.state)?)?;
            self.refuse_if_game_running(&d2r)?;
            running.recovering.insert(product_id.clone());
            d2r
        };
        let _reserved = Recovering(self, product_id);
        // Any manifest of the product names its install root.
        let manifest = self.manifest_of(product_id)?.manifest;
        let cancel = AtomicBool::new(false);
        let host = GameAware {
            host: &*self.host,
            game: &*self.game,
            d2r: &d2r,
        };
        let env = self.env(&host, &NoFaults, &cancel);
        Ok(match ops::recover(&env, &d2r, &manifest)? {
            Recovery::Nothing => RecoveryView::Nothing,
            Recovery::RolledBack => RecoveryView::RolledBack,
            Recovery::RolledForward => RecoveryView::RolledForward,
        })
    }
}

/// Frees a product's recovery reservation when dropped, even on a panic.
struct Recovering<'a>(&'a Launcher, &'a ProductId);

impl Drop for Recovering<'_> {
    fn drop(&mut self) {
        self.0.running().recovering.remove(self.1);
    }
}

#[cfg(test)]
mod earlier_tests {
    use super::*;

    fn fixture(rel: &str) -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/catalog")
            .join(rel);
        std::fs::read(p).unwrap()
    }

    /// #134: clean install's evidence is every other cached manifest of the product,
    /// never the target itself, another product's, or bytes that fail validation.
    #[test]
    fn earlier_lists_other_valid_cached_manifests_of_the_product() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = StateStore::open(dir.path());
        let pd2r = ProductId("pd2r".into());
        let old = fixture("real/pd2r-0.1.0-alpha/release-manifest.json");
        let target = fixture("valid/release-manifest.json");
        state
            .put_cached("manifest/pd2r/pd2r-0.1.0-alpha", &old)
            .unwrap();
        state
            .put_cached("manifest/pd2r/pd2r-0.4.0-alpha.1", &target)
            .unwrap();
        state.put_cached("manifest/pd2r/broken", b"{}").unwrap();
        state
            .put_cached("manifest/pd2rx/pd2r-0.1.0-alpha", &old)
            .unwrap();
        state.put_cached("feed", b"{}").unwrap();

        let earlier = CachedReleases.earlier(&mut state, &pd2r, "pd2r-0.4.0-alpha.1");

        let ids: Vec<_> = earlier.iter().map(|m| m.release_id.as_str()).collect();
        assert_eq!(ids, ["pd2r-0.1.0-alpha"]);
    }
}
