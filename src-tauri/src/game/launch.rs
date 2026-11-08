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
use crate::install::journal::refuse_pending_journal;
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
    refuse_pending_journal(&product_root(root, manifest))?;
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
///
/// The game must outlive the launcher, holding none of its handles. Std spawns
/// with handle inheritance on, so the launcher's own std handles (pipes when it
/// was started by a script, CI or `tauri dev`) are made non-inheritable first;
/// otherwise a parent reading such a pipe would wait for the game to exit (#71).
/// The game gets null stdio, and dropping the `Child` closes our handle without killing
/// it. If the launcher runs in a job object that kills its processes on close, the
/// game is created with `CREATE_BREAKAWAY_FROM_JOB`; a job that forbids breakaway
/// makes that fail with access denied, and the game is started inside it instead.
///
/// No `DETACHED_PROCESS` or `CREATE_NEW_PROCESS_GROUP`: D2RLoader.exe is a GUI
/// program (PE subsystem 2, checked on the owner's copy), so it never attaches to
/// the launcher's console or receives its Ctrl+C.
pub(super) fn system_spawn(exe: &Path, args: &[String], cwd: &Path) -> io::Result<u32> {
    #[cfg(windows)]
    stop_inheriting_std_handles();
    let spawn = |flags: u32| {
        let mut cmd = Command::new(exe);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, flags);
        #[cfg(not(windows))]
        let _ = flags;
        cmd.spawn().map(|child| child.id())
    };
    match spawn(CREATE_BREAKAWAY_FROM_JOB) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => spawn(0),
        result => result,
    }
}

const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

/// Clears `HANDLE_FLAG_INHERIT` on this process's std handles. Missing or
/// unusable handles (a GUI build started from Explorer has none) are skipped.
#[cfg(windows)]
fn stop_inheriting_std_handles() {
    type Handle = isize;
    const STD_HANDLES: [u32; 3] = [-10i32 as u32, -11i32 as u32, -12i32 as u32];
    const HANDLE_FLAG_INHERIT: u32 = 0x1;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(which: u32) -> Handle;
        fn SetHandleInformation(handle: Handle, mask: u32, flags: u32) -> i32;
    }

    for which in STD_HANDLES {
        // SAFETY: plain Win32 calls; a null or invalid handle only makes
        // SetHandleInformation fail, which is ignored.
        unsafe {
            let handle = GetStdHandle(which);
            if handle != 0 && handle != -1 {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}
