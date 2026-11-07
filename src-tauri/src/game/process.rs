//! Game process tracking (#58): every process whose image is under the selected
//! D2R root, including games started outside the launcher.
#![allow(unused_variables)] // shortcut: frozen stubs (#55); remove when #58 implements them.

use std::io;
use std::path::Path;
use std::time::Duration;

use super::GameHost;
use crate::contract::{LauncherError, RunningGame};

/// The processes from [`GameHost::processes`] whose image path is inside `root`,
/// compared by path components as Windows does (case-insensitive; `C:\D2R2` is not
/// under `C:\D2R`). Empty when nothing runs there. Also used after a launcher
/// restart to rediscover a running game.
///
/// Errors: [`crate::contract::ErrorCode::Internal`] if enumeration fails.
pub fn running_under(root: &Path, host: &dyn GameHost) -> Result<Vec<RunningGame>, LauncherError> {
    todo!("#58")
}

/// Exit watcher: blocks, checking [`running_under`] every `poll`, until nothing runs
/// under `root`. The caller then reassesses readiness (back to ready).
pub fn wait_until_exited(
    root: &Path,
    host: &dyn GameHost,
    poll: Duration,
) -> Result<(), LauncherError> {
    todo!("#58")
}

/// See [`GameHost::processes`]: Toolhelp32 snapshot plus
/// `QueryFullProcessImageNameW` and the process creation time.
pub(super) fn system_processes() -> io::Result<Vec<RunningGame>> {
    todo!("#58")
}
