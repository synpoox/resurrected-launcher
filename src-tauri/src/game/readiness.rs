//! Readiness assessment (#57): game build, loader and installed-release checks
//! against a release manifest's `compatibility`. Writes nothing.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::SystemTime;

use super::detect::{is_link, D2R_EXE};
use super::GameHost;
use crate::catalog::ReleaseManifest;
use crate::contract::{
    ErrorCode, GameLocation, LauncherError, Readiness, RunningGame, Sha256Digest,
};
use crate::install::download::hash_file;

/// The loader files whose ProductVersion must be allowed, in check order.
const LOADER_FILES: [&str; 2] = ["D2RLoader.exe", "D2RCore.dll"];

/// Everything [`assess`] needs that it does not read from disk itself. The caller
/// (#60) gathers these fresh for each assessment.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// The selected location, `None` when none is selected or it failed
    /// [`super::detect::validate`].
    pub location: Option<&'a GameLocation>,
    /// The installed release's manifest; `None` when the product has no
    /// installed-release record. The caller fails with
    /// [`crate::contract::ErrorCode::MetadataInvalid`] itself when a record exists
    /// but its manifest is unavailable; it never passes `None` for that.
    pub installed: Option<&'a ReleaseManifest>,
    /// The feed's recommended release, `None` offline or when metadata is invalid.
    pub recommended: Option<&'a ReleaseManifest>,
    /// An interrupted transaction journal exists for this product.
    pub recovery_pending: bool,
    /// The installed-release record is not verified: a Repair started and did
    /// not finish (#155). Ignored when nothing is installed.
    pub unverified: bool,
    /// [`super::process::running_under`] the location's root.
    pub running: &'a [RunningGame],
}

/// SHA-256 of game executables and pinned loader files keyed by (path, size,
/// mtime), so an unchanged `D2R.exe` or `D2RLoader.exe` is hashed once per
/// launcher run. Shared across assessments.
#[derive(Debug, Default)]
pub struct BuildHashCache {
    entries: Mutex<HashMap<(PathBuf, u64, SystemTime), Sha256Digest>>,
}

/// The product's [`Readiness`], first reason in the precedence order documented on
/// it. Build and loader checks use `installed`, or `recommended` when not installed:
///
/// - Build: `<root>\<executable>` of some `compatibility.game_builds` entry matches
///   its size and SHA-256 (hashed through `cache`); else `build_unsupported` with
///   the location's build. A missing executable is `d2r_missing`.
/// - Loader: [`GameHost::product_version`] of both `<root>\D2RLoader.exe` and
///   `<root>\D2RCore.dll` is in `compatibility.loader_versions`; then, for each
///   file whose (version, file) `compatibility.loader_builds` pins, its size and
///   SHA-256 match one of those pins, else `loader_modified`. Unpinned files (and
///   every file when `loader_builds` is empty) are checked by version only.
/// - `not_installed` when `installed` is `None` (also when `recommended` is `None`).
/// - `repair_needed` when installed but [`Facts::unverified`].
/// - `update_available` when both are present with different `release_id`s.
///
/// Errors only for unexpected I/O while hashing ([`crate::contract::ErrorCode::Internal`]
/// or [`crate::contract::ErrorCode::AccessDenied`]); never returns a readiness it has
/// not verified.
pub fn assess(
    facts: &Facts<'_>,
    cache: &BuildHashCache,
    host: &dyn GameHost,
) -> Result<Readiness, LauncherError> {
    let Some(location) = facts.location else {
        return Ok(Readiness::D2rMissing);
    };
    let root = &location.path;
    let manifest = facts.installed.or(facts.recommended);
    let builds = manifest.map_or(&[][..], |m| &m.compatibility.game_builds[..]);
    let present = |rel: &str| regular_file(&root.join(rel)).is_some();
    if !present(D2R_EXE)
        || (!builds.is_empty() && !builds.iter().any(|b| present(b.executable.as_str())))
    {
        return Ok(Readiness::D2rMissing);
    }
    if let Some(since) = facts.running.iter().map(|g| g.since).min() {
        return Ok(Readiness::Running { since });
    }
    if facts.recovery_pending {
        return Ok(Readiness::RecoveryRequired);
    }
    let Some(manifest) = manifest else {
        // Nothing to check the build and loader against, and nothing installed.
        return Ok(Readiness::NotInstalled);
    };
    let mut supported = false;
    for build in builds {
        if cache.matches(
            &root.join(build.executable.as_str()),
            build.size,
            &build.sha256,
        )? {
            supported = true;
            break;
        }
    }
    if !supported {
        return Ok(Readiness::BuildUnsupported {
            found: location.build.clone(),
        });
    }
    let mut versions = Vec::new();
    for file in LOADER_FILES {
        let path = root.join(file);
        let version = regular_file(&path).and_then(|_| host.product_version(&path).ok().flatten());
        match version {
            Some(v) => versions.push((file, v)),
            None => return Ok(Readiness::LoaderMissing),
        }
    }
    let allowed = &manifest.compatibility.loader_versions;
    if let Some((_, found)) = versions.iter().find(|(_, v)| !allowed.contains(v)) {
        return Ok(Readiness::LoaderUnsupported {
            found: found.clone(),
            allowed: allowed.clone(),
        });
    }
    for (file, version) in versions {
        let pins: Vec<_> = manifest
            .compatibility
            .loader_builds
            .iter()
            .filter(|b| b.version == version && b.file.as_str().eq_ignore_ascii_case(file))
            .collect();
        if pins.is_empty() {
            continue;
        }
        let path = root.join(file);
        let mut official = false;
        for pin in pins {
            if cache.matches(&path, pin.size, &pin.sha256)? {
                official = true;
                break;
            }
        }
        if !official {
            return Ok(Readiness::LoaderModified {
                file: file.to_string(),
                version,
            });
        }
    }
    Ok(match (facts.installed, facts.recommended) {
        (None, _) => Readiness::NotInstalled,
        (Some(_), _) if facts.unverified => Readiness::RepairNeeded,
        (Some(i), Some(r)) if r.release_id != i.release_id => Readiness::UpdateAvailable,
        _ => Readiness::Ready,
    })
}

/// Metadata of `path` when it is a regular file, not a link or reparse point.
fn regular_file(path: &Path) -> Option<fs::Metadata> {
    fs::symlink_metadata(path)
        .ok()
        .filter(|m| m.is_file() && !is_link(m))
}

impl BuildHashCache {
    /// Whether `path` is a regular file of exactly `size` bytes and SHA-256 `sha256`.
    /// Hashes only on a size match, and once per (path, size, mtime).
    fn matches(
        &self,
        path: &Path,
        size: u64,
        sha256: &Sha256Digest,
    ) -> Result<bool, LauncherError> {
        let Some(meta) = regular_file(path) else {
            return Ok(false);
        };
        if meta.len() != size {
            return Ok(false);
        }
        let mtime = meta
            .modified()
            .map_err(|e| hash_error(path, ErrorCode::Internal, e.to_string()))?;
        let key = (path.to_path_buf(), meta.len(), mtime);
        let cached = self.lock().get(&key).copied();
        let digest = match cached {
            Some(d) => d,
            None => {
                let (n, bytes) = hash_file(&key.0, &AtomicBool::new(false))
                    .map_err(|e| hash_error(&key.0, e.code, e.message))?;
                let Some(d) = Sha256Digest::from_bytes(bytes).filter(|_| n == key.1) else {
                    return Ok(false); // changed while hashing; not cached
                };
                self.lock().insert(key, d);
                d
            }
        };
        Ok(digest == *sha256)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(PathBuf, u64, SystemTime), Sha256Digest>> {
        // The map holds only finished digests, so a poisoned lock is still consistent.
        self.entries.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Only [`ErrorCode::AccessDenied`] or [`ErrorCode::Internal`] leave [`assess`].
fn hash_error(path: &Path, code: ErrorCode, detail: String) -> LauncherError {
    let code = match code {
        ErrorCode::AccessDenied => ErrorCode::AccessDenied,
        _ => ErrorCode::Internal,
    };
    LauncherError::new(code, format!("hashing {}: {detail}", path.display()))
}
