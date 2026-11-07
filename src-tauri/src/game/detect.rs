//! D2R location detection and manual selection (#57). Writes nothing.
#![allow(unused_variables)] // shortcut: frozen stubs (#55); remove when #57 implements them.

use std::path::Path;

use super::GameHost;
use crate::contract::{GameLocation, LauncherError};

/// Valid D2R locations, in order: `remembered` (the preference), then each
/// Battle.net registry `InstallLocation`. Duplicates (compared as Windows does,
/// case-insensitively after canonicalizing) keep the first. Candidates that fail
/// [`validate`] are left out, so the result may be empty.
pub fn candidates(remembered: Option<&Path>, host: &dyn GameHost) -> Vec<GameLocation> {
    todo!("#57")
}

/// Validates one folder (remembered, from the registry, or chosen in the picker).
/// Valid only if `path\D2R.exe` is a regular file, not a link, and
/// [`GameHost::product_version`] returns a version. `store` is
/// [`crate::contract::GameStore::BattleNet`] when `path` is a Battle.net registry
/// location.
///
/// Errors: [`crate::contract::ErrorCode::D2rNotFound`], with a message telling the
/// player to choose the folder that contains `D2R.exe`.
pub fn validate(path: &Path, host: &dyn GameHost) -> Result<GameLocation, LauncherError> {
    todo!("#57")
}
