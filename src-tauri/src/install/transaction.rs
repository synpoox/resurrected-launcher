//! The journaled update transaction and restart recovery (#36), implementing the
//! "Direction and recovery rule" of [`crate::contract::Boundary`].
//!
//! [`run`] walks [`Boundary::sequence`] for the plan's kind. The engine performs the
//! file-moving boundaries itself; the caller performs `DownloadComplete`, `Staged`,
//! `StagedInventoryVerified` and `LiveVerified` through [`Steps`]. Every
//! multi-entry step writes its complete planned entry list to the journal at
//! `BeforeStep`, and every entry is decided from disk, so a crash anywhere is
//! resolved by [`recover`]: roll forward once `LiveVerified` is journaled, otherwise
//! roll back to the previous complete installation. The installed-release record is
//! written only at `InstalledRecordCommitted`, after `LiveVerified`.
//!
//! `RollbackVerified` is decided from the journal and disk: every planned live
//! entry moved to `B` (or `L` moved to `K`) is back in place and every incoming
//! entry is back in `S`. Activation only renames, never rewrites, so this proves the
//! previous installation's bytes are what they were.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::journal::{
    canonical_root, sibling, Direction, Entry, Journal, Paths, StepRecord, JOURNAL_VERSION,
};
use super::lock::OperationLock;
use super::preflight::{self, Host, SpaceNeeds};
use crate::contract::{
    Boundary, EntryOnDisk, ErrorCode, FaultInjector, FaultPoint, InjectedFault, LauncherError,
    OperationKind, PlannedCopy, PlannedRename, RelativePath, WriteGuard, DISPOSABLE_SENTINEL,
};

/// The installed-release record change committed at `InstalledRecordCommitted`.
/// The value is the record State (#34) stores, carried verbatim in the journal so
/// that roll-forward after a restart can commit it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordChange {
    Set(serde_json::Value),
    Remove,
}

/// Where the installed-release record lives (#34; wired by #37). `commit` must be
/// idempotent: recovery repeats it if a crash hid whether it finished.
pub trait InstalledRecordSink {
    fn commit(&self, product_root: &Path, change: &RecordChange) -> Result<(), LauncherError>;
}

/// What the transaction does, decided by the caller (#37) before it opens. Paths
/// are relative to `L`, `S`, `B` or `K` as named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub kind: OperationKind,
    /// Managed regular files moved `L` -> `B` (replaced or removed). Never player
    /// or unknown files.
    pub to_backup: Vec<RelativePath>,
    /// Entries moved `S` -> `L`. Ignored by clean install, which moves `S` whole.
    pub to_live: Vec<RelativePath>,
    /// Clean install: recognised player files copied `K` -> new `L`.
    pub carry_forward: Vec<RelativePath>,
    /// Dirs in `L` removed after commit if they are empty (uninstall's managed
    /// dirs, required empty dirs the new release dropped).
    pub prune_if_empty: Vec<RelativePath>,
    pub record: RecordChange,
    pub space: SpaceNeeds,
}

/// The caller-owned boundaries. `perform` is called once for each of
/// `DownloadComplete`, `Staged`, `StagedInventoryVerified` and `LiveVerified` in the
/// kind's sequence. An error rolls the transaction back (return
/// [`ErrorCode::Cancelled`] to cancel); after `LiveVerified` nothing is asked.
pub trait Steps {
    fn perform(&mut self, boundary: Boundary, paths: &Paths) -> Result<(), LauncherError>;
}

/// Injected dependencies. Production: [`crate::contract::NoFaults`],
/// [`preflight::SystemHost`] and the State record sink.
pub struct Context<'a> {
    pub injector: &'a dyn FaultInjector,
    pub host: &'a dyn Host,
    pub sink: &'a dyn InstalledRecordSink,
}

/// What [`recover`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// No interrupted transaction.
    Nothing,
    /// The previous complete installation was restored.
    RolledBack,
    /// Activation had been verified; the new release was committed.
    RolledForward,
}

impl From<InjectedFault> for LauncherError {
    fn from(f: InjectedFault) -> Self {
        LauncherError::new(ErrorCode::Internal, f.to_string())
    }
}

/// Why a step stopped: a simulated crash (return at once, touch nothing) or an error.
enum Halt {
    Fault(InjectedFault),
    Error(LauncherError),
}

impl From<LauncherError> for Halt {
    fn from(e: LauncherError) -> Self {
        Halt::Error(e)
    }
}

fn recovery_required(why: impl std::fmt::Display) -> LauncherError {
    LauncherError::new(ErrorCode::RecoveryRequired, why.to_string())
}

/// `base` joined with a `/`-separated relative path, one component at a time
/// (verbatim `\\?\` paths do not accept `/`).
fn under(base: &Path, rel: &RelativePath) -> PathBuf {
    rel.as_str()
        .split('/')
        .fold(base.to_path_buf(), |p, c| p.join(c))
}

fn present(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

struct Tx<'a> {
    j: Journal,
    ctx: &'a Context<'a>,
}

impl Tx<'_> {
    fn check(&self, b: Boundary, p: FaultPoint) -> Result<(), Halt> {
        self.ctx.injector.check(b, p).map_err(Halt::Fault)
    }

    fn record(&self, b: Boundary) -> impl Iterator<Item = &Entry> {
        self.j
            .steps
            .iter()
            .filter(move |s| s.boundary == b)
            .flat_map(|s| &s.entries)
    }

    fn renames(&self, b: Boundary) -> impl Iterator<Item = &PlannedRename> {
        self.record(b).filter_map(|e| match e {
            Entry::Rename(r) => Some(r),
            _ => None,
        })
    }

    /// The undo of forward step `b`'s planned entries, last entry first.
    fn reversed(&self, b: Boundary) -> Vec<Entry> {
        let entries: Vec<_> = self.record(b).collect();
        entries
            .into_iter()
            .rev()
            .filter_map(Entry::reversed)
            .collect()
    }

    fn moved_files(&self) -> Vec<PathBuf> {
        let root = &self.j.paths.root;
        self.j
            .plan
            .to_backup
            .iter()
            .map(|x| under(root, x))
            .collect()
    }

    /// The complete planned entry list of multi-entry step `b`.
    fn plan_entries(&self, b: Boundary) -> Result<Vec<Entry>, LauncherError> {
        let (p, plan) = (&self.j.paths, &self.j.plan);
        let keep = || {
            p.clean_backup.clone().ok_or_else(|| {
                LauncherError::new(ErrorCode::Internal, "clean install without backup path")
            })
        };
        let mut entries = Vec::new();
        match b {
            Boundary::LiveMovedToBackup => {
                for x in &plan.to_backup {
                    let from = under(&p.root, x);
                    if !std::fs::symlink_metadata(&from).is_ok_and(|m| m.is_file()) {
                        return Err(LauncherError::new(
                            ErrorCode::InventoryMismatch,
                            format!("managed file {x} is not a regular file in the live root"),
                        ));
                    }
                    let to = under(&p.backup, x);
                    entries.push(Entry::Rename(PlannedRename { from, to }));
                }
            }
            Boundary::CleanInstallBackupMade => {
                let keep = keep()?;
                entries.push(Entry::Copy(PlannedCopy::plan(
                    p.root.join(DISPOSABLE_SENTINEL),
                    p.staging.join(DISPOSABLE_SENTINEL),
                )?));
                entries.push(Entry::Rename(PlannedRename {
                    from: p.root.clone(),
                    to: keep,
                }));
            }
            Boundary::StagedMovedToLive if plan.kind == OperationKind::CleanInstall => {
                entries.push(Entry::Rename(PlannedRename {
                    from: p.staging.clone(),
                    to: p.root.clone(),
                }));
            }
            Boundary::StagedMovedToLive => {
                let mut made = HashSet::new();
                for x in &plan.to_live {
                    let to = under(&p.root, x);
                    // Live dirs this step creates are journaled so rollback removes them.
                    let mut missing: Vec<_> = to
                        .ancestors()
                        .skip(1)
                        .take_while(|d| *d != p.root && !present(d))
                        .map(Path::to_path_buf)
                        .collect();
                    missing.reverse();
                    for d in missing {
                        if made.insert(d.clone()) {
                            entries.push(Entry::MakeDir { path: d });
                        }
                    }
                    entries.push(Entry::Rename(PlannedRename {
                        from: under(&p.staging, x),
                        to,
                    }));
                }
            }
            Boundary::PlayerFilesCarriedForward => {
                let keep = keep()?;
                for x in &plan.carry_forward {
                    let copy = PlannedCopy::plan(under(&keep, x), under(&p.root, x))?;
                    entries.push(Entry::Copy(copy));
                }
            }
            Boundary::NewEntriesRemovedFromLive => {
                entries = self.reversed(Boundary::StagedMovedToLive);
            }
            Boundary::BackupRestoredToLive => {
                entries = self.reversed(Boundary::LiveMovedToBackup);
                entries.extend(self.reversed(Boundary::CleanInstallBackupMade));
            }
            Boundary::CleanedUp => {
                for s in &self.j.steps {
                    for e in &s.entries {
                        if let Entry::Copy(c) = e {
                            if c.temp_is_reserved() {
                                entries.push(Entry::Delete {
                                    path: c.temp.clone(),
                                });
                            }
                        }
                    }
                }
                for path in [&p.staging, &p.backup, &p.archive] {
                    entries.push(Entry::Delete { path: path.clone() });
                }
                if self.j.direction == Direction::Forward {
                    let mut prune: Vec<_> = plan
                        .prune_if_empty
                        .iter()
                        .map(|x| under(&p.root, x))
                        .collect();
                    prune.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
                    entries.extend(prune.into_iter().map(|path| Entry::RemoveDir { path }));
                }
            }
            _ => unreachable!("{b:?} is not multi-entry"),
        }
        Ok(entries)
    }

    /// Performs boundary `b` with every fault point, then journals it as reached.
    fn step(&mut self, b: Boundary, steps: Option<&mut dyn Steps>) -> Result<(), Halt> {
        let first_live_mutation = matches!(
            b,
            Boundary::LiveMovedToBackup | Boundary::CleanInstallBackupMade
        );
        if first_live_mutation && self.j.direction == Direction::Forward {
            let needs = self.j.plan.space.recovery_copy;
            preflight::check(&self.j.paths, needs, &self.moved_files(), self.ctx.host)?;
        }
        if b.is_multi_entry() {
            let entries = self.plan_entries(b)?;
            self.j.steps.retain(|s| s.boundary != b);
            self.j.steps.push(StepRecord {
                boundary: b,
                entries: entries.clone(),
            });
            self.j.save()?;
            self.j.save_progress(b, 0)?;
            self.check(b, FaultPoint::BeforeStep)?;
            for (i, entry) in entries.iter().enumerate() {
                self.check(b, FaultPoint::BeforeEntry(i))?;
                match entry.on_disk() {
                    EntryOnDisk::NotDone => entry.apply(&self.j.paths)?,
                    EntryOnDisk::Done => {}
                    EntryOnDisk::Conflict => {
                        return Err(Halt::Error(recovery_required(format!(
                            "{b:?} entry {i} matches neither state on disk: {entry:?}"
                        ))))
                    }
                }
                self.check(b, FaultPoint::AfterEntry(i))?;
                self.j.save_progress(b, i + 1)?;
            }
        } else {
            self.check(b, FaultPoint::BeforeStep)?;
            match b {
                Boundary::TransactionOpened | Boundary::RollbackStarted => {}
                Boundary::InstalledRecordCommitted => self
                    .ctx
                    .sink
                    .commit(&self.j.paths.root, &self.j.plan.record)?,
                Boundary::RollbackVerified => self.verify_rollback()?,
                _ => match steps {
                    Some(s) => s.perform(b, &self.j.paths)?,
                    None => {
                        return Err(Halt::Error(LauncherError::new(
                            ErrorCode::Internal,
                            format!("{b:?} cannot be performed during recovery"),
                        )))
                    }
                },
            }
        }
        self.check(b, FaultPoint::AfterStep)?;
        if b == Boundary::CleanedUp {
            self.j.close()?;
        } else {
            self.j.reached = Some(b);
            self.j.save()?;
        }
        Ok(())
    }

    fn verify_rollback(&self) -> Result<(), LauncherError> {
        let out_of_live: Vec<_> = self
            .renames(Boundary::LiveMovedToBackup)
            .chain(self.renames(Boundary::CleanInstallBackupMade))
            .collect();
        let restored: HashSet<_> = out_of_live.iter().map(|r| &r.from).collect();
        for r in &out_of_live {
            if r.on_disk() != EntryOnDisk::NotDone {
                return Err(recovery_required(format!(
                    "{} is not back in the live root",
                    r.from.display()
                )));
            }
        }
        for r in self.renames(Boundary::StagedMovedToLive) {
            if !present(&r.from) || (present(&r.to) && !restored.contains(&r.to)) {
                return Err(recovery_required(format!(
                    "{} is not back in staging",
                    r.to.display()
                )));
            }
        }
        Ok(())
    }

    /// `LiveVerified` is journaled: from here the transaction rolls forward.
    fn rolls_forward(&self) -> bool {
        self.j.direction == Direction::Forward
            && matches!(
                self.j.reached,
                Some(Boundary::LiveVerified | Boundary::InstalledRecordCommitted)
            )
    }

    fn finish_forward(&mut self) -> Result<(), Halt> {
        let seq = Boundary::sequence(self.j.kind);
        let next = self
            .j
            .reached
            .and_then(|r| seq.iter().position(|b| *b == r))
            .map_or(0, |i| i + 1);
        for &b in &seq[next..] {
            self.step(b, None)?;
        }
        Ok(())
    }

    /// Switches the journal to rollback (once) and resumes [`Boundary::ROLLBACK`]
    /// after the last rollback boundary reached.
    fn rollback(&mut self, reason: &str) -> Result<(), Halt> {
        if self.j.direction == Direction::Forward {
            self.j.direction = Direction::Rollback;
            self.j.rollback_reason = Some(reason.to_owned());
            self.j.save()?;
        }
        let next = self
            .j
            .reached
            .and_then(|r| Boundary::ROLLBACK.iter().position(|b| *b == r))
            .map_or(0, |i| i + 1);
        for &b in &Boundary::ROLLBACK[next..] {
            self.step(b, None)?;
        }
        Ok(())
    }
}

/// An error that stops recovery (or a rollback) leaves the journal for the next
/// recovery and is reported as [`ErrorCode::RecoveryRequired`].
fn stuck(h: Halt) -> LauncherError {
    match h {
        Halt::Fault(f) => f.into(),
        Halt::Error(e) if e.code == ErrorCode::RecoveryRequired => e,
        Halt::Error(e) => recovery_required(e),
    }
}

/// Runs `plan` as one journaled transaction on `guard`'s root, which `lock` must
/// hold. Preflight runs first and again before the first live mutation; a refusal
/// there leaves the installation unchanged. On a failure before `LiveVerified` the
/// previous installation is restored and the failure returned; if that restore
/// cannot finish, or a failure comes after `LiveVerified`, the result is
/// [`ErrorCode::RecoveryRequired`] and [`recover`] resolves it. An injected fault
/// returns at once, as if the process died.
pub fn run(
    guard: &WriteGuard,
    lock: &OperationLock,
    plan: Plan,
    steps: &mut dyn Steps,
    ctx: &Context,
) -> Result<(), LauncherError> {
    let root = guard.root();
    if lock.root() != root {
        return Err(LauncherError::new(
            ErrorCode::InvalidRequest,
            "the operation lock is for another product root",
        ));
    }
    if Journal::load(root)?.is_some() {
        return Err(recovery_required(
            "an interrupted transaction needs recovery",
        ));
    }
    let mut paths = Paths::for_root(root);
    if plan.kind == OperationKind::CleanInstall {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        paths.clean_backup = Some(sibling(root, &format!(".clean-install-backup-{secs}")));
    }
    let scratch = [&paths.staging, &paths.backup, &paths.archive];
    if let Some(left) = scratch
        .into_iter()
        .chain(&paths.clean_backup)
        .find(|p| present(p))
    {
        return Err(LauncherError::new(
            ErrorCode::InvalidRequest,
            format!("{} already exists", left.display()),
        ));
    }
    let mut tx = Tx {
        j: Journal {
            version: JOURNAL_VERSION,
            kind: plan.kind,
            paths,
            plan,
            direction: Direction::Forward,
            reached: None,
            rollback_reason: None,
            steps: Vec::new(),
        },
        ctx,
    };
    preflight::check(
        &tx.j.paths,
        tx.j.plan.space.total(),
        &tx.moved_files(),
        ctx.host,
    )?;
    tx.j.save()?;

    let mut forward = || -> Result<(), Halt> {
        for &b in Boundary::sequence(tx.j.kind) {
            tx.step(b, Some(&mut *steps))?;
        }
        Ok(())
    };
    match forward() {
        Ok(()) => Ok(()),
        Err(Halt::Fault(f)) => Err(f.into()),
        Err(Halt::Error(e)) if tx.rolls_forward() => Err(recovery_required(e)),
        Err(Halt::Error(e)) => match tx.rollback(&e.message) {
            Ok(()) => Err(e),
            Err(h) => Err(stuck(h)),
        },
    }
}

/// Startup recovery for one product root, run before Play is enabled: resolves an
/// interrupted transaction to the complete previous installation or, when
/// `LiveVerified` was journaled, the complete new one. Takes the root's operation
/// lock itself. Idempotent; a crash during recovery is resolved by calling it again.
pub fn recover(product_root: &Path, ctx: &Context) -> Result<Recovery, LauncherError> {
    let root = canonical_root(product_root)?;
    let Some(peek) = Journal::load(&root)? else {
        return Ok(Recovery::Nothing);
    };
    // Mid clean install the root may be absent while `K` holds the sentinel.
    let guard = match &peek.paths.clean_backup {
        Some(keep) => WriteGuard::check_with_backup(&root, keep)?,
        None => WriteGuard::check(&root)?,
    };
    let _lock = OperationLock::acquire(&guard)?;
    let Some(j) = Journal::load(guard.root())? else {
        return Ok(Recovery::Nothing);
    };
    let mut tx = Tx { j, ctx };
    if tx.rolls_forward() {
        tx.finish_forward().map_err(stuck)?;
        Ok(Recovery::RolledForward)
    } else {
        tx.rollback("restart recovery").map_err(stuck)?;
        Ok(Recovery::RolledBack)
    }
}

#[cfg(test)]
mod tests;
