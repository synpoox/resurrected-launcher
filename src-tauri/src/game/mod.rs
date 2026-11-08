//! Game module: detect or select D2R, assess game/loader readiness, guide the
//! player to the official loader download, launch a product explicitly and track
//! running processes. The launcher never downloads or runs the loader installer.
//!
//! Returns actionable readiness reasons rather than spreading checks into React.
//! Offline single-player only: launch takes no mode, server or extra argument.
//!
//! Interfaces frozen by #55; each submodule's owner implements its own file only:
//!
//! | Submodule | Owner | Does |
//! | --- | --- | --- |
//! | [`detect`] | #57 | Candidate and manually chosen D2R locations |
//! | [`readiness`] | #57 | Build/loader/installed checks into a [`Readiness`] |
//! | `winapi` | #57 | Registry and PE version FFI behind [`SystemGameHost`] |
//! | [`process`] | #58 | Processes under the D2R root, exit watching |
//! | [`launch`] | #58 | `D2RLoader.exe -mod <folder>` from the root |
//! | [`loader_setup`] | #59 | Guidance to the official D2RLoader download page |
//!
//! Nothing here writes to a D2R installation; [`crate::contract::WriteGuard`]
//! still refuses real roots until the slice 3 gate (#61) phase B.
//!
//! [`Readiness`]: crate::contract::Readiness

pub mod detect;
pub mod launch;
pub mod loader_setup;
pub mod process;
pub mod readiness;
mod winapi;

use std::io;
use std::path::{Path, PathBuf};

use crate::contract::RunningGame;

/// The machine facts and effects the game module uses. Production wires
/// [`SystemGameHost`]; tests inject a fake. Every other check (file metadata,
/// hashing, journal) works on real (temp) files and needs no host.
pub trait GameHost: Send + Sync {
    /// `InstallLocation` of every Battle.net D2R uninstall registry entry the user
    /// can read (HKLM and HKCU, 64- and 32-bit views), in registry order. Empty when
    /// none; never fails.
    fn battle_net_install_locations(&self) -> Vec<PathBuf>;

    /// Every running process whose full image path could be queried, as
    /// [`RunningGame`]s (not yet filtered to a D2R root). Processes that cannot be
    /// opened for query are skipped. Errors only if enumeration itself fails.
    fn processes(&self) -> io::Result<Vec<RunningGame>>;

    /// The PE ProductVersion string of `file`'s version resource, e.g. `3.3.93847`;
    /// `Ok(None)` when the file has no readable version info.
    fn product_version(&self, file: &Path) -> io::Result<Option<String>>;

    /// Starts `exe` with exactly `args` and working directory `cwd`: no shell, no
    /// elevation, not tied to the launcher's lifetime. Returns the new process ID.
    fn spawn(&self, exe: &Path, args: &[String], cwd: &Path) -> io::Result<u32>;
}

/// The real machine. Each method delegates to the submodule that owns its FFI.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemGameHost;

impl GameHost for SystemGameHost {
    fn battle_net_install_locations(&self) -> Vec<PathBuf> {
        winapi::battle_net_install_locations()
    }

    fn processes(&self) -> io::Result<Vec<RunningGame>> {
        process::system_processes()
    }

    fn product_version(&self, file: &Path) -> io::Result<Option<String>> {
        winapi::product_version(file)
    }

    fn spawn(&self, exe: &Path, args: &[String], cwd: &Path) -> io::Result<u32> {
        launch::system_spawn(exe, args, cwd)
    }
}
