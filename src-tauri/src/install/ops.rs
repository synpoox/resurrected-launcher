//! The operation interface (#37): install, adopt, repair, update, uninstall and
//! clean install, plus restart recovery. The UI commands (#40) and the tests call
//! only [`inspect`], [`run`] and [`recover`]; files move only through
//! [`transaction::run`] and [`transaction::recover`].
//!
//! [`run`] follows the #8 pipeline: plan ([`super::plan`]), download/verify,
//! confined staging, staging inventory plus a fresh preservation and conflict
//! check, journaled activation, live verification, record commit, cleanup.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use super::download::Downloader;
use super::extract::{extract, verify_staging, SevenZip};
use super::journal::{refuse_pending_journal, Paths};
use super::lock::OperationLock;
use super::paths::TransactionPaths;
use super::plan::{self, Conflict, Inspection, Request};
use super::preflight::Host;
use super::preserve::walk;
use super::transaction::{self, Context, InstalledRecordSink, RecordChange, Recovery, Steps};
use crate::catalog::ReleaseManifest;
use crate::contract::{
    Boundary, ErrorCode, FaultInjector, LauncherError, OperationKind, ProductId, RelativePath,
    WriteGuard,
};
use crate::state::{InstalledRelease, StateStore};

/// Everything an operation needs besides the request. Production passes
/// [`crate::contract::NoFaults`] and [`super::preflight::SystemHost`].
pub struct Env<'a> {
    pub downloader: &'a Downloader,
    pub seven_zip: &'a SevenZip,
    pub state: &'a Mutex<StateStore>,
    pub host: &'a dyn Host,
    pub injector: &'a dyn FaultInjector,
    /// Set to cancel; honoured before each caller-owned step and during download
    /// and extraction. Activation is never interrupted, only rolled back.
    pub cancel: &'a AtomicBool,
}

/// What a committed operation did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub kind: OperationKind,
    pub product_root: PathBuf,
    /// [`Inspection::preserved`]: player-edited shipped defaults kept or carried
    /// forward, and changed files update and uninstall left in place.
    pub preserved: Vec<RelativePath>,
    /// Uninstall: every file still in the product root (player and unknown files).
    pub remaining: Vec<String>,
    /// Every labelled clean-install backup beside the product root. They are never
    /// deleted by the launcher.
    pub clean_install_backups: Vec<PathBuf>,
}

fn lock(state: &Mutex<StateStore>) -> Result<MutexGuard<'_, StateStore>, LauncherError> {
    state
        .lock()
        .map_err(|_| LauncherError::new(ErrorCode::Internal, "state lock poisoned"))
}

/// Writes the installed-release record of one product to State.
struct Records<'a> {
    state: &'a Mutex<StateStore>,
    product: &'a ProductId,
}

impl InstalledRecordSink for Records<'_> {
    fn commit(&self, _root: &Path, change: &RecordChange) -> Result<(), LauncherError> {
        let mut state = lock(self.state)?;
        match change {
            RecordChange::Set(value) => {
                let record: InstalledRelease = serde_json::from_value(value.clone())
                    .map_err(|e| LauncherError::new(ErrorCode::Internal, e.to_string()))?;
                if &record.product_id != self.product {
                    return Err(LauncherError::new(
                        ErrorCode::Internal,
                        "journaled record names another product",
                    ));
                }
                state.mark_installed(record)
            }
            RecordChange::Remove => state.remove_installed(self.product),
        }
    }
}

/// Reads the plan for `req` without changing anything: what would move, what the
/// player keeps, and any conflict that would pause it.
pub fn inspect(env: &Env, req: &Request) -> Result<Inspection, LauncherError> {
    let root = plan::product_root(req.d2r, req.release);
    plan::inspect(&*lock(env.state)?, req, &root)
}

fn refuse(conflicts: &[Conflict]) -> Result<(), LauncherError> {
    let settings: Vec<String> = conflicts
        .iter()
        .filter_map(|c| match c {
            Conflict::Setting {
                file,
                key,
                required,
                found,
            } => Some(format!("{file}: {key} must be {required}, found {found:?}")),
            _ => None,
        })
        .collect();
    if !settings.is_empty() {
        return Err(LauncherError::new(
            ErrorCode::MandatorySettingConflict,
            settings.join("; "),
        ));
    }
    if let Some(Conflict::UnknownFile { path }) = conflicts.first() {
        return Err(LauncherError::new(
            ErrorCode::InventoryMismatch,
            format!("{path} is a file the launcher did not install; clean install can replace it"),
        ));
    }
    Ok(())
}

struct OpSteps<'a> {
    env: &'a Env<'a>,
    req: &'a Request<'a>,
    guard: &'a WriteGuard,
    insp: Inspection,
}

impl Steps for OpSteps<'_> {
    fn perform(&mut self, b: Boundary, paths: &Paths) -> Result<(), LauncherError> {
        let cancel = self.env.cancel;
        if cancel.load(Ordering::Relaxed) {
            return Err(LauncherError::new(
                ErrorCode::Cancelled,
                "cancelled by the user",
            ));
        }
        let tx = TransactionPaths::of(self.guard);
        if tx.archive() != paths.archive || tx.staging() != paths.staging {
            return Err(LauncherError::new(
                ErrorCode::Internal,
                "download/extract paths differ from the journal's",
            ));
        }
        let m = self.req.release;
        match b {
            Boundary::DownloadComplete => {
                let injector = self.env.injector;
                self.env
                    .downloader
                    .fetch(&m.archive, &tx, cancel, &mut |done, total| {
                        injector.bytes(done, total)
                    })
                    .map(drop)
            }
            Boundary::Staged => extract(self.env.seven_zip, m, &tx, cancel),
            Boundary::StagedInventoryVerified => {
                verify_staging(&tx, &m.inventory, cancel)?;
                // Recheck preservation right before the first live mutation.
                let fresh = plan::inspect(&*lock(self.env.state)?, self.req, &paths.root)?;
                refuse(&fresh.conflicts)?;
                let moves = |i: &Inspection| {
                    let p = &i.plan;
                    (
                        p.to_backup.clone(),
                        p.to_live.clone(),
                        p.carry_forward.clone(),
                    )
                };
                if moves(&fresh) != moves(&self.insp) {
                    return Err(LauncherError::new(
                        ErrorCode::InventoryMismatch,
                        "the product folder changed during the operation",
                    ));
                }
                self.insp = fresh;
                Ok(())
            }
            Boundary::LiveVerified => {
                // What moved to `B` is what was inspected; else roll back, putting it back.
                // A real I/O error surfaces as itself.
                plan::verify_live(&paths.backup, &self.insp.expect_backup).map_err(|e| {
                    if e.code != ErrorCode::InventoryMismatch {
                        return e;
                    }
                    LauncherError::new(
                        ErrorCode::InventoryMismatch,
                        "the product folder changed during the operation",
                    )
                })?;
                if self.req.kind != OperationKind::Repair {
                    return plan::verify_live(&paths.root, &self.insp.expect);
                }
                // One file at a time, so Repair can report files checked.
                let expect = &self.insp.expect;
                let total = expect.len() as u64;
                self.env.injector.files(0, total);
                for (i, entry) in expect.iter().enumerate() {
                    plan::verify_live(&paths.root, std::slice::from_ref(entry))?;
                    self.env.injector.files(i as u64 + 1, total);
                }
                Ok(())
            }
            _ => Err(LauncherError::new(
                ErrorCode::Internal,
                format!("{b:?} is not a caller step"),
            )),
        }
    }
}

/// Labelled clean-install backups beside `root`, oldest first.
pub fn clean_install_backups(root: &Path) -> Vec<PathBuf> {
    let (Some(parent), Some(name)) = (root.parent(), root.file_name()) else {
        return Vec::new();
    };
    let prefix = format!("{}.clean-install-backup-", name.to_string_lossy());
    let mut found: Vec<PathBuf> = std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}

/// Runs one operation to commit. Refusals before any change:
/// [`ErrorCode::CleanInstallConsentRequired`], [`ErrorCode::SaveBreakingNotAcknowledged`],
/// [`ErrorCode::MandatorySettingConflict`], [`ErrorCode::InventoryMismatch`] (an
/// unknown file where the release needs a path), [`ErrorCode::RecoveryRequired`]
/// (an interrupted transaction), and the guard, lock and preflight errors. A later
/// failure or [`ErrorCode::Cancelled`] restores the previous installation; see
/// [`transaction::run`].
pub fn run(env: &Env, req: &Request) -> Result<Report, LauncherError> {
    use OperationKind::*;
    if req.kind == CleanInstall && !req.consent_clean_install {
        return Err(LauncherError::new(
            ErrorCode::CleanInstallConsentRequired,
            "clean install needs explicit consent",
        ));
    }
    // ADR 0001: warn and require acknowledgement; saves are never converted.
    let installs_new = matches!(req.kind, Install | Update | CleanInstall);
    if installs_new
        && req.release.breaks_saves_from(req.installed)
        && !req.acknowledge_save_breaking
    {
        return Err(LauncherError::new(
            ErrorCode::SaveBreakingNotAcknowledged,
            format!("{} is save-breaking", req.release.release_id),
        ));
    }
    let guard = WriteGuard::check(&plan::product_root(req.d2r, req.release))?;
    let op_lock = OperationLock::acquire(&guard)?;
    refuse_pending_journal(guard.root())?;
    let insp = plan::inspect(&*lock(env.state)?, req, guard.root())?;
    refuse(&insp.conflicts)?;
    let preserved = insp.preserved.clone();
    let sink = Records {
        state: env.state,
        product: &req.release.product_id,
    };
    let ctx = Context {
        injector: env.injector,
        host: env.host,
        sink: &sink,
    };
    let tx_plan = insp.plan.clone();
    if req.kind == Repair {
        // Before the journal: ended at any point from here, the product stays
        // flagged for Repair; the commit's fresh record clears it (#155).
        lock(env.state)?.mark_unverified(&req.release.product_id)?;
    }
    let mut steps = OpSteps {
        env,
        req,
        guard: &guard,
        insp,
    };
    transaction::run(&guard, &op_lock, tx_plan, &mut steps, &ctx)?;
    let root = guard.root().to_path_buf();
    let remaining = if req.kind == Uninstall {
        walk(&root)?.into_iter().map(|(rel, _)| rel).collect()
    } else {
        Vec::new()
    };
    Ok(Report {
        kind: req.kind,
        clean_install_backups: clean_install_backups(&root),
        product_root: root,
        preserved,
        remaining,
    })
}

/// Startup recovery of `manifest`'s product in `d2r`, before Play is enabled:
/// resolves an interrupted transaction to the complete previous installation, or
/// the complete new one once it was verified. Any release manifest of the product
/// will do; only its product ID and install root are used.
pub fn recover(
    env: &Env,
    d2r: &Path,
    manifest: &ReleaseManifest,
) -> Result<Recovery, LauncherError> {
    let sink = Records {
        state: env.state,
        product: &manifest.product_id,
    };
    let ctx = Context {
        injector: env.injector,
        host: env.host,
        sink: &sink,
    };
    transaction::recover(&plan::product_root(d2r, manifest), &ctx)
}
