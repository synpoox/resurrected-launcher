//! Explicit product launch (#58): `<root>\D2RLoader.exe -mod <folder>`, offline
//! single-player only.

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use super::process::running_under;
use super::GameHost;
use crate::catalog::ReleaseManifest;
use crate::contract::{ErrorCode, GameLocation, LauncherError, Readiness, RunningGame};
use crate::install::journal::journal_path;
use crate::install::plan::product_root;

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
    if let Some(code) = readiness.play_refusal() {
        return Err(LauncherError::new(
            code,
            format!("not playable: {readiness:?}"),
        ));
    }
    let root = &location.path;
    if std::fs::symlink_metadata(journal_path(&product_root(root, manifest))).is_ok() {
        return Err(LauncherError::new(
            ErrorCode::RecoveryRequired,
            "an interrupted transaction needs recovery",
        ));
    }
    if let Some(game) = running_under(root, host)?.first() {
        return Err(LauncherError::new(
            ErrorCode::GameRunning,
            format!("{} (pid {}) is running", game.image.display(), game.pid),
        ));
    }
    let exe = root.join("D2RLoader.exe");
    let args = ["-mod".to_owned(), manifest.mod_folder().to_owned()];
    let pid = host.spawn(&exe, &args, root).map_err(|e| {
        LauncherError::new(
            ErrorCode::LaunchFailed,
            format!("starting {}: {e}", exe.display()),
        )
    })?;
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Ok(RunningGame {
        pid,
        image: exe,
        since,
    })
}

/// See [`GameHost::spawn`] (`std::process::Command`, never a shell).
pub(super) fn system_spawn(exe: &Path, args: &[String], cwd: &Path) -> io::Result<u32> {
    // Null stdio so the game holds none of the launcher's handles; dropping the
    // `Child` closes our handle without killing the game.
    let child = Command::new(exe)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(child.id())
}
