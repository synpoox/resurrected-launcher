//! Install operation commands (#40): one command per user intent, built on
//! [`crate::install::ops`]. The frontend sends product IDs, operation kinds,
//! consents and the token of the inspection it showed; everything else (the D2R
//! location, release manifests, the downloader, 7-Zip) is native state.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::catalog::{Availability, Feed, ReleaseManifest};
use crate::contract::{
    Boundary, CancelRequest, ErrorCode, FaultInjector, FaultPoint, InjectedFault, LauncherError,
    NoFaults, OperationKind, OperationOutcome, OperationRequest, Phase, ProductId, ProgressEvent,
    RelativePath, Sha256Digest, Versioned,
};
use crate::install::download::Downloader;
use crate::install::extract::SevenZip;
use crate::install::ops::{self, Env};
use crate::install::plan::{self, Conflict, Inspection, Request};
use crate::install::preflight::Host;
use crate::install::transaction::Recovery;
use crate::state::{InstalledRelease, StateStore};

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
}

/// Reads the feed (`feed`) and manifests (`manifest/<product>/<release>`) from
/// State's metadata cache, re-validating them on every read.
pub struct CachedReleases;

fn metadata(why: String) -> LauncherError {
    LauncherError::new(ErrorCode::MetadataInvalid, why)
}

fn invalid(why: impl Into<String>) -> LauncherError {
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
}

/// Native state behind the commands, managed by Tauri as `Arc<Launcher>`.
pub struct Launcher {
    pub state: Mutex<StateStore>,
    pub releases: Box<dyn Releases>,
    pub downloader: Downloader,
    pub seven_zip: SevenZip,
    pub host: Box<dyn Host>,
    /// Running operations: cancel flag and kind, by product.
    running: Mutex<HashMap<ProductId, (Arc<AtomicBool>, OperationKind)>>,
}

fn locked<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, LauncherError> {
    m.lock()
        .map_err(|_| LauncherError::new(ErrorCode::Internal, "lock poisoned"))
}

/// Everything an [`ops`] request borrows, resolved from native state.
struct Resolved {
    kind: OperationKind,
    d2r: PathBuf,
    release: Release,
    installed: Option<Release>,
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
    ) -> Self {
        Self {
            state: Mutex::new(state),
            releases,
            downloader,
            seven_zip,
            host,
            running: Mutex::new(HashMap::new()),
        }
    }

    fn env<'a>(&'a self, injector: &'a dyn FaultInjector, cancel: &'a AtomicBool) -> Env<'a> {
        Env {
            downloader: &self.downloader,
            seven_zip: &self.seven_zip,
            state: &self.state,
            host: &*self.host,
            injector,
            cancel,
        }
    }

    /// The D2R location from Preferences and the manifests `req` acts on: the
    /// recommended release for install, adopt, update and clean install; the
    /// installed one for repair and uninstall.
    fn resolve(&self, req: &OperationRequest) -> Result<Resolved, LauncherError> {
        use OperationKind::*;
        let mut state = locked(&self.state)?;
        let d2r = state
            .preferences()
            .d2r_location
            .clone()
            .ok_or_else(|| invalid("no D2R location is selected"))?;
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
        Ok(Resolved {
            kind: req.kind,
            d2r,
            release,
            installed,
            acknowledge_save_breaking: req.acknowledge_save_breaking,
            consent_clean_install: req.consent_clean_install,
        })
    }

    fn inspect(&self, resolved: &Resolved) -> Result<(Inspection, String), LauncherError> {
        let req = resolved.request();
        let insp = ops::inspect(&self.env(&NoFaults, &AtomicBool::new(false)), &req)?;
        let root = plan::product_root(req.d2r, req.release);
        // Debug of Vec-only structs is deterministic within one launcher build, and
        // tokens never outlive the process, so it binds exactly what was inspected.
        let digest = Sha256::digest(format!("{root:?}|{}|{insp:?}", req.release_sha256));
        let token = digest.iter().map(|b| format!("{b:02x}")).collect();
        Ok((insp, token))
    }

    fn state_of(&self, product: &ProductId) -> Result<ProductState, LauncherError> {
        let running = locked(&self.running)?.get(product).map(|(_, kind)| *kind);
        let mut state = locked(&self.state)?;
        let installed = state.installed_release(product).cloned();
        // An unavailable feed only hides the update offer; it never implies readiness.
        let recommended = self
            .releases
            .recommended(&mut state, product)
            .ok()
            .map(|r| ReleaseSummary::of(&r.manifest));
        Ok(ProductState {
            product_id: product.clone(),
            installed,
            recommended,
            running,
        })
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
    fn of(m: &ReleaseManifest) -> Self {
        Self {
            release_id: m.release_id.clone(),
            display_version: m.display_version.clone(),
            save_breaking: m.is_save_breaking(),
        }
    }
}

/// Reply to `get_product_state`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductState {
    pub product_id: ProductId,
    pub installed: Option<InstalledRelease>,
    /// `None` when the feed or manifest is unavailable or invalid.
    pub recommended: Option<ReleaseSummary>,
    /// The operation running on this product, if any.
    pub running: Option<OperationKind>,
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
    pub preserved: Vec<RelativePath>,
    /// Clean install: files that stay only in the labelled backup.
    pub backup_only: Vec<RelativePath>,
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
            preserved: insp.preserved,
            backup_only: insp.backup_only,
            conflicts: insp
                .conflicts
                .into_iter()
                .map(|c| match c {
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
                })
                .collect(),
            save_breaking_notice: insp.save_breaking_notice,
            space_needed: insp.plan.space.total(),
            token,
        }
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

/// Turns the transaction's boundary checks into progress events; never injects.
struct Progress<F> {
    product_id: ProductId,
    kind: OperationKind,
    reached: Mutex<Option<Boundary>>,
    emit: F,
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
            FaultPoint::BeforeStep => (self.emit)(ProgressEvent {
                product_id: self.product_id.clone(),
                kind: self.kind,
                phase: phase(b),
                boundary: *reached,
                bytes_done: None,
                bytes_total: None,
            }),
            FaultPoint::AfterStep => *reached = Some(b),
            _ => {}
        }
        Ok(())
    }
}

/// `get_product_state`: installed release, recommended release, running operation.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_product_state(
    launcher: State<'_, Arc<Launcher>>,
    product_id: ProductId,
) -> Result<Versioned<ProductState>, LauncherError> {
    launcher.state_of(&product_id).map(Versioned::new)
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
    let resolved = launcher.resolve(&req)?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut running = locked(&launcher.running)?;
        if running.contains_key(&req.product_id) {
            return Err(LauncherError::new(
                ErrorCode::OperationInProgress,
                format!("{} already has an operation running", req.product_id.0),
            ));
        }
        // Checked under the running lock so two starts cannot both pass it.
        if launcher.inspect(&resolved)?.1 != inspection_token {
            return Err(LauncherError::new(
                ErrorCode::InventoryMismatch,
                "the product changed since it was inspected; inspect again",
            ));
        }
        running.insert(req.product_id.clone(), (cancel.clone(), req.kind));
    }
    let launcher = Arc::clone(&launcher);
    std::thread::spawn(move || {
        let (product_id, kind) = (req.product_id, req.kind);
        let progress = Progress {
            product_id: product_id.clone(),
            kind,
            reached: Mutex::new(None),
            emit: |e| {
                let _ = app.emit(PROGRESS_EVENT, Versioned::new(e));
            },
        };
        let result = ops::run(&launcher.env(&progress, &cancel), &resolved.request());
        if let Ok(mut running) = launcher.running.lock() {
            running.remove(&product_id);
        }
        let outcome = match result {
            Ok(_) => OperationOutcome::Succeeded { product_id, kind },
            Err(e) if e.code == ErrorCode::Cancelled => {
                OperationOutcome::Cancelled { product_id, kind }
            }
            Err(error) => OperationOutcome::Failed {
                product_id,
                kind,
                error,
            },
        };
        let _ = app.emit(OUTCOME_EVENT, Versioned::new(outcome));
    });
    Ok(())
}

/// `cancel_operation`: asks the running operation to stop. During activation it
/// rolls back safely; the outcome event reports when it has.
#[tauri::command(rename_all = "snake_case")]
pub async fn cancel_operation(
    launcher: State<'_, Arc<Launcher>>,
    request: Versioned<CancelRequest>,
) -> Result<(), LauncherError> {
    let req = request.into_current()?;
    let running = locked(&launcher.running)?;
    let (cancel, _) = running
        .get(&req.product_id)
        .ok_or_else(|| invalid(format!("{} has no operation running", req.product_id.0)))?;
    cancel.store(true, Ordering::Relaxed);
    Ok(())
}

/// `recover_product`: startup recovery before Play is enabled, and the way out of
/// [`ErrorCode::RecoveryRequired`].
#[tauri::command(rename_all = "snake_case")]
pub async fn recover_product(
    launcher: State<'_, Arc<Launcher>>,
    product_id: ProductId,
) -> Result<Versioned<RecoveryView>, LauncherError> {
    if locked(&launcher.running)?.contains_key(&product_id) {
        return Err(LauncherError::new(
            ErrorCode::OperationInProgress,
            format!("{} has an operation running", product_id.0),
        ));
    }
    let (d2r, manifest) = {
        let mut state = locked(&launcher.state)?;
        let d2r = state
            .preferences()
            .d2r_location
            .clone()
            .ok_or_else(|| invalid("no D2R location is selected"))?;
        // Any manifest of the product names its install root.
        let release = match state.installed_release(&product_id).cloned() {
            Some(record) => launcher.releases.installed(&mut state, &record)?,
            None => launcher.releases.recommended(&mut state, &product_id)?,
        };
        (d2r, release.manifest)
    };
    let cancel = AtomicBool::new(false);
    let recovery = ops::recover(&launcher.env(&NoFaults, &cancel), &d2r, &manifest)?;
    Ok(Versioned::new(match recovery {
        Recovery::Nothing => RecoveryView::Nothing,
        Recovery::RolledBack => RecoveryView::RolledBack,
        Recovery::RolledForward => RecoveryView::RolledForward,
    }))
}
