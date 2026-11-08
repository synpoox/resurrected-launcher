//! Readiness assessment (#57): game build, loader and installed-release checks
//! against a release manifest's `compatibility`. Writes nothing.
#![allow(unused_variables, dead_code)] // shortcut: frozen stubs (#55); remove when #57 implements them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use super::GameHost;
use crate::catalog::ReleaseManifest;
use crate::contract::{GameLocation, LauncherError, Readiness, RunningGame, Sha256Digest};

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
    /// [`super::process::running_under`] the location's root.
    pub running: &'a [RunningGame],
}

/// SHA-256 of game executables keyed by (path, size, mtime), so an unchanged
/// `D2R.exe` is hashed once per launcher run. Shared across assessments.
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
///   `<root>\D2RCore.dll` is in `compatibility.loader_versions`.
/// - `not_installed` when `installed` is `None` (also when `recommended` is `None`).
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
    todo!("#57")
}
