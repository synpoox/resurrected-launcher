//! Inspect/plan (#8 pipeline step 1): decides, from the request, the installed
//! record and the live product root, exactly which release files a transaction
//! moves, what the player keeps and what conflicts stop it. Reads disk; writes
//! nothing.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::journal::io_error;
use super::paths::under;
use super::preflight::SpaceNeeds;
use super::preserve::{self, at, fate, is, mandatory_settings, on_disk, Digest, Fate, OnDisk};
use super::transaction::{Plan, RecordChange};
use crate::catalog::{Ownership, ReleaseManifest};
use crate::contract::{
    ErrorCode, ExpectedFile, LauncherError, OperationKind, RelativePath, Sha256Digest,
    DISPOSABLE_SENTINEL,
};
use crate::state::{InstalledRelease, StateStore};

/// One operation on one product, as the UI commands (#40) and tests ask for it.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub kind: OperationKind,
    /// The selected D2R installation; the product root is its `install_root`.
    pub d2r: &'a Path,
    /// The release acted on: the target of install, adopt, update and clean
    /// install; the installed release for repair and uninstall.
    pub release: &'a ReleaseManifest,
    /// SHA-256 of `release`'s manifest bytes, stored in the installed record.
    pub release_sha256: Sha256Digest,
    /// The installed release's manifest (State's cache). Required exactly when
    /// State records an installed release of this product. Its published hashes
    /// are the baseline that tells untouched shipped defaults from edited ones.
    pub installed: Option<&'a ReleaseManifest>,
    /// The player acknowledged the save-incompatibility notice (ADR 0001).
    pub acknowledge_save_breaking: bool,
    /// Explicit consent for [`OperationKind::CleanInstall`].
    pub consent_clean_install: bool,
}

/// Why an operation is paused before it changes anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// A kept (edited) shipped default does not hold a setting the release needs.
    /// The player's file is never overwritten; they edit it, or clean install
    /// (which leaves it in the labelled backup).
    Setting {
        file: RelativePath,
        key: String,
        required: String,
        found: Option<String>,
    },
    /// The release needs this path, but a file the launcher did not install is
    /// there. Unknown files are never replaced; clean install (with consent) is the
    /// way out.
    UnknownFile { path: RelativePath },
}

/// What live verification requires of one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Want {
    File(Digest),
    /// Unknown files: present, same size (not re-hashed).
    Size(u64),
    Absent,
    Dir,
}

/// The result of inspecting a request against disk.
#[derive(Debug, Clone)]
pub struct Inspection {
    /// The transaction plan.
    pub plan: Plan,
    /// Non-empty: the operation is refused until they are resolved.
    pub conflicts: Vec<Conflict>,
    /// Edited shipped defaults kept in place (uninstall: also changed managed
    /// files it could not positively identify; update: also changed files the new
    /// release drops), or carried forward by clean install.
    pub preserved: Vec<RelativePath>,
    /// Clean install: files that stay only in the labelled backup (unknown files,
    /// and edited defaults that fail a mandatory setting).
    pub backup_only: Vec<RelativePath>,
    /// Shown, and acknowledged, before installing a save-breaking release.
    pub save_breaking_notice: Option<String>,
    /// `/`-separated path -> what `LiveVerified` requires.
    pub(crate) expect: Vec<(String, Want)>,
}

fn invalid(why: impl Into<String>) -> LauncherError {
    LauncherError::new(ErrorCode::InvalidRequest, why)
}

/// The product root of `manifest` in the D2R installation `d2r`.
pub fn product_root(d2r: &Path, manifest: &ReleaseManifest) -> PathBuf {
    under(d2r, &manifest.install_root)
}

fn fold(p: &str) -> String {
    p.to_uppercase()
}

fn files_by_fold(m: &ReleaseManifest) -> HashMap<String, &ExpectedFile> {
    m.inventory
        .files()
        .iter()
        .map(|f| (fold(f.path.as_str()), f))
        .collect()
}

/// Directories a release creates: every file's ancestors and its empty dirs.
fn dirs(m: &ReleaseManifest) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let paths = m.inventory.files().iter().map(|f| f.path.as_str());
    for p in paths {
        for (i, _) in p.match_indices('/') {
            out.insert(p[..i].to_owned());
        }
    }
    out.extend(m.inventory.empty_dirs().iter().map(|d| d.to_string()));
    out
}

fn check_request(state: &StateStore, req: &Request) -> Result<(), LauncherError> {
    use OperationKind::*;
    let m = req.release;
    let record = state.installed_release(&m.product_id);
    match (record, req.installed) {
        (Some(r), Some(i)) if i.release_id == r.release_id && i.product_id == m.product_id => {}
        (None, None) => {}
        _ => {
            return Err(invalid(
                "the installed manifest must match the installed-release record",
            ))
        }
    }
    let installed_id = record.map(|r| r.release_id.as_str());
    let ok = match req.kind {
        Install | Adopt => installed_id.is_none(),
        Update => installed_id.is_some_and(|id| id != m.release_id),
        Repair | Uninstall => installed_id == Some(m.release_id.as_str()),
        CleanInstall => true,
    };
    if ok {
        Ok(())
    } else {
        Err(invalid(format!(
            "{:?} of {} does not fit installed release {installed_id:?}",
            req.kind, m.release_id
        )))
    }
}

/// Decides the plan for `req` against the live root `root` (the guarded product
/// root). Refusals of the request itself are errors; conflicts are data.
pub fn inspect(
    state: &StateStore,
    req: &Request,
    root: &Path,
) -> Result<Inspection, LauncherError> {
    use OperationKind::*;
    check_request(state, req)?;
    let (kind, new) = (req.kind, req.release);
    // Repair restores the installed release, so it is its own baseline.
    let baseline = if kind == Repair {
        Some(new)
    } else {
        req.installed
    };
    let mut insp = Inspection {
        plan: Plan {
            kind,
            to_backup: Vec::new(),
            to_live: Vec::new(),
            carry_forward: Vec::new(),
            prune_if_empty: Vec::new(),
            record: RecordChange::Remove,
            space: SpaceNeeds::default(),
        },
        conflicts: Vec::new(),
        preserved: Vec::new(),
        backup_only: Vec::new(),
        save_breaking_notice: new.saves.incompatibility_notice.clone(),
        expect: Vec::new(),
    };
    let (plan, expect) = (&mut insp.plan, &mut insp.expect);
    let mut recovery = 0u64;
    let mut known: BTreeSet<String> = new
        .inventory
        .files()
        .iter()
        .map(|f| fold(f.path.as_str()))
        .collect();

    if kind == Uninstall {
        // Only files whose bytes prove they came from the release are removed.
        for f in new.inventory.files() {
            match on_disk(&under(root, &f.path))? {
                OnDisk::File(d) if is(&d, f) => {
                    plan.to_backup.push(f.path.clone());
                    expect.push((f.path.to_string(), Want::Absent));
                    recovery += d.0;
                }
                OnDisk::File(d) => {
                    insp.preserved.push(f.path.clone());
                    expect.push((f.path.to_string(), Want::File(d)));
                }
                _ => {}
            }
        }
        plan.prune_if_empty = dirs(new)
            .into_iter()
            .map(RelativePath::new)
            .collect::<Result<_, _>>()
            .map_err(|e| invalid(format!("release dir: {e}")))?;
    } else {
        let old = baseline.map(files_by_fold).unwrap_or_default();
        for f in new.inventory.files() {
            let disk = on_disk(&under(root, &f.path))?;
            let base = old.get(&fold(f.path.as_str())).map(|o| {
                (
                    *o,
                    baseline.map_or(Ownership::Unknown, |b| b.ownership_of(&o.path)),
                )
            });
            let want = Want::File((f.size, *f.sha256.as_bytes()));
            let fate = match kind {
                // Adopt changes nothing; LiveVerified checks managed files match.
                Adopt => match (disk, new.ownership_of(&f.path)) {
                    (OnDisk::File(d), Ownership::ShippedDefault) if !is(&d, f) => Fate::KeepEdited,
                    _ => Fate::Keep,
                },
                _ => fate(disk, f, new.ownership_of(&f.path), base),
            };
            match fate {
                Fate::Keep => expect.push((f.path.to_string(), want)),
                Fate::Add => {
                    plan.to_live.push(f.path.clone());
                    expect.push((f.path.to_string(), want));
                }
                Fate::Replace => {
                    plan.to_backup.push(f.path.clone());
                    plan.to_live.push(f.path.clone());
                    expect.push((f.path.to_string(), want));
                    recovery += f.size;
                }
                Fate::KeepEdited => {
                    let OnDisk::File(d) = disk else {
                        unreachable!("only a file is edited")
                    };
                    let bad = if matches!(kind, Install | Update | CleanInstall) {
                        setting_conflicts(new, root, &f.path)?
                    } else {
                        Vec::new()
                    };
                    if kind == CleanInstall && !bad.is_empty() {
                        // The new default goes live; the player's file stays in the backup.
                        insp.backup_only.push(f.path.clone());
                        expect.push((f.path.to_string(), want));
                    } else {
                        insp.preserved.push(f.path.clone());
                        expect.push((f.path.to_string(), Want::File(d)));
                        if kind == CleanInstall {
                            plan.carry_forward.push(f.path.clone());
                        }
                        insp.conflicts.extend(bad);
                    }
                }
                Fate::Conflict if kind == CleanInstall => {
                    insp.backup_only.push(f.path.clone());
                    expect.push((f.path.to_string(), want));
                }
                Fate::Conflict => insp.conflicts.push(Conflict::UnknownFile {
                    path: f.path.clone(),
                }),
            }
        }
        for d in new.inventory.empty_dirs() {
            match std::fs::symlink_metadata(under(root, d)) {
                Ok(m) if super::journal::real_dir(&m) => {}
                Err(_) if kind == Adopt => {}
                Err(_) => {
                    if kind != CleanInstall {
                        plan.to_live.push(d.clone());
                    }
                }
                Ok(_) if kind == CleanInstall => {}
                Ok(_) => insp
                    .conflicts
                    .push(Conflict::UnknownFile { path: d.clone() }),
            }
            expect.push((d.to_string(), Want::Dir));
        }
        if kind == Update {
            if let Some(b) = baseline {
                let current = files_by_fold(new);
                for f in b.inventory.files() {
                    let key = fold(f.path.as_str());
                    if current.contains_key(&key) {
                        continue;
                    }
                    known.insert(key);
                    // Removed by the new release: only the old release's exact bytes go.
                    if let OnDisk::File(d) = on_disk(&under(root, &f.path))? {
                        if is(&d, f) {
                            plan.to_backup.push(f.path.clone());
                            expect.push((f.path.to_string(), Want::Absent));
                            recovery += d.0;
                        } else {
                            insp.preserved.push(f.path.clone());
                            expect.push((f.path.to_string(), Want::File(d)));
                        }
                    }
                }
                let keep = dirs(new);
                plan.prune_if_empty = dirs(b)
                    .difference(&keep)
                    .map(|d| RelativePath::new(d.as_str()))
                    .collect::<Result<_, _>>()
                    .map_err(|e| invalid(format!("release dir: {e}")))?;
            }
        }
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let record = InstalledRelease {
            product_id: new.product_id.clone(),
            release_id: new.release_id.clone(),
            display_version: new.display_version.clone(),
            manifest_sha256: req.release_sha256,
            installed_at: secs,
            verified: true,
        };
        plan.record =
            RecordChange::Set(serde_json::to_value(record).map_err(|e| invalid(e.to_string()))?);
        if kind != Adopt {
            let staged: u64 = new.inventory.files().iter().map(|f| f.size).sum();
            plan.space.download = new.archive.size;
            plan.space.staging = staged;
        }
    }
    if kind == CleanInstall {
        // The whole old root becomes the labelled backup; nothing moves file by file.
        plan.to_backup.clear();
        plan.to_live.clear();
        recovery = 0;
    }
    plan.space.recovery_copy = recovery;

    // Unknown files (saves, plugins, anything unlisted) must still be there after.
    // Clean install leaves them only in the labelled backup, and says so.
    if std::fs::symlink_metadata(root).is_ok() {
        let old = baseline.map(files_by_fold).unwrap_or_default();
        for (rel, size) in preserve::walk(root)? {
            if known.contains(&fold(&rel)) {
                continue;
            }
            if kind != CleanInstall {
                expect.push((rel, Want::Size(size)));
            } else if rel != DISPOSABLE_SENTINEL {
                // An old-release file with its published bytes is not the player's.
                if let Some(f) = old.get(&fold(&rel)) {
                    if matches!(on_disk(&at(root, &rel))?, OnDisk::File(d) if is(&d, f)) {
                        continue;
                    }
                }
                let p =
                    RelativePath::new(rel).map_err(|e| invalid(format!("unknown file: {e}")))?;
                insp.backup_only.push(p);
            }
        }
    }
    Ok(insp)
}

/// Mandatory settings of `m` that the kept file `path` under `root` fails.
fn setting_conflicts(
    m: &ReleaseManifest,
    root: &Path,
    path: &RelativePath,
) -> Result<Vec<Conflict>, LauncherError> {
    let mut out = Vec::new();
    for s in mandatory_settings(m).filter(|s| fold(s.file.as_str()) == fold(path.as_str())) {
        let p = under(root, &s.file);
        let text = std::fs::read_to_string(&p).map_err(|e| io_error("reading", &p, e))?;
        let found = preserve::setting_value(&text, &s.key);
        if found != Some(s.value.as_str()) {
            out.push(Conflict::Setting {
                file: s.file.clone(),
                key: s.key.clone(),
                required: s.value.clone(),
                found: found.map(str::to_owned),
            });
        }
    }
    Ok(out)
}

/// Checks `root` against `expect` (the `LiveVerified` boundary).
pub(crate) fn verify_live(root: &Path, expect: &[(String, Want)]) -> Result<(), LauncherError> {
    for (rel, want) in expect {
        let p = at(root, rel);
        let meta = std::fs::symlink_metadata(&p);
        let ok = match want {
            Want::File(d) => on_disk(&p)? == OnDisk::File(*d),
            Want::Size(n) => meta.is_ok_and(|m| m.is_file() && m.len() == *n),
            Want::Absent => meta.is_err(),
            Want::Dir => meta.is_ok_and(|m| super::journal::real_dir(&m)),
        };
        if !ok {
            return Err(LauncherError::new(
                ErrorCode::InventoryMismatch,
                format!("{rel} in the live root is not as planned ({want:?})"),
            ));
        }
    }
    Ok(())
}
