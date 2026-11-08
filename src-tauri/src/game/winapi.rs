//! Registry and PE version FFI for [`super::SystemGameHost`] (#57): raw
//! `extern "system"` declarations, the same pattern as `GetDiskFreeSpaceExW` in
//! `install::preflight`; no new crate.
#![allow(unused_variables)] // shortcut: frozen stubs (#55); remove when #57 implements them.

use std::io;
use std::path::{Path, PathBuf};

/// See [`super::GameHost::battle_net_install_locations`].
pub(super) fn battle_net_install_locations() -> Vec<PathBuf> {
    todo!("#57")
}

/// See [`super::GameHost::product_version`] (`GetFileVersionInfoW`/`VerQueryValueW`).
pub(super) fn product_version(file: &Path) -> io::Result<Option<String>> {
    todo!("#57")
}
