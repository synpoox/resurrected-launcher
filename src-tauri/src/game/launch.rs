//! Explicit product launch (#58): `<root>\D2RLoader.exe -mod <folder>`, offline
//! single-player only.
#![allow(unused_variables)] // shortcut: frozen stubs (#55); remove when #58 implements them.

use std::io;
use std::path::Path;

use super::GameHost;
use crate::catalog::ReleaseManifest;
use crate::contract::{GameLocation, LauncherError, Readiness, RunningGame};

/// Starts the installed release of `manifest`'s product at `location` through
/// [`GameHost::spawn`]: exe `<root>\D2RLoader.exe`, args exactly
/// `["-mod", manifest.mod_folder()]`, working directory the root.
///
/// `manifest` is the installed release's manifest and `readiness` the product's
/// assessment just made. Refuses with `readiness.play_refusal()` when set, then
/// rechecks immediately before spawning: a journal under the product root
/// ([`crate::contract::ErrorCode::RecoveryRequired`]) or anything
/// [`super::process::running_under`] the root
/// ([`crate::contract::ErrorCode::GameRunning`]).
///
/// Returns the started loader process. Errors from spawning are
/// [`crate::contract::ErrorCode::LaunchFailed`].
pub fn play(
    location: &GameLocation,
    manifest: &ReleaseManifest,
    readiness: &Readiness,
    host: &dyn GameHost,
) -> Result<RunningGame, LauncherError> {
    todo!("#58")
}

/// See [`GameHost::spawn`] (`std::process::Command`, never a shell).
pub(super) fn system_spawn(exe: &Path, args: &[String], cwd: &Path) -> io::Result<u32> {
    todo!("#58")
}
