//! D2R location detection and manual selection (#57). Writes nothing.

use std::fs;
use std::path::{Path, PathBuf};

use super::GameHost;
use crate::contract::{ErrorCode, GameLocation, GameStore, LauncherError};

/// The game executable every D2R root holds.
pub(super) const D2R_EXE: &str = "D2R.exe";

/// Valid D2R locations, in order: `remembered` (the preference), then each
/// Battle.net registry `InstallLocation`. Duplicates (compared as Windows does,
/// case-insensitively after canonicalizing) keep the first. Candidates that fail
/// [`validate`] are left out, so the result may be empty.
pub fn candidates(remembered: Option<&Path>, host: &dyn GameHost) -> Vec<GameLocation> {
    let registry = host.battle_net_install_locations();
    let mut seen = Vec::new();
    let mut found = Vec::new();
    for path in remembered
        .into_iter()
        .chain(registry.iter().map(PathBuf::as_path))
    {
        let key = same_folder_key(path);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        if let Ok(location) = validate_with(path, &registry, host) {
            found.push(location);
        }
    }
    found
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
    validate_with(path, &host.battle_net_install_locations(), host)
}

fn validate_with(
    path: &Path,
    registry: &[PathBuf],
    host: &dyn GameHost,
) -> Result<GameLocation, LauncherError> {
    let not_found = |why: String| {
        LauncherError::new(
            ErrorCode::D2rNotFound,
            format!("{why}. Choose the Diablo II: Resurrected folder that contains {D2R_EXE}."),
        )
    };
    let root = std::path::absolute(path)
        .map_err(|e| not_found(format!("{} is not a usable folder: {e}", path.display())))?;
    let exe = root.join(D2R_EXE);
    let meta = fs::symlink_metadata(&exe)
        .map_err(|e| not_found(format!("No readable {D2R_EXE} in {}: {e}", root.display())))?;
    if is_link(&meta) {
        return Err(not_found(format!(
            "{} is a link or reparse point, not the game itself",
            exe.display()
        )));
    }
    if !meta.is_file() {
        return Err(not_found(format!("{} is not a file", exe.display())));
    }
    let build = match host.product_version(&exe) {
        Ok(Some(version)) => version,
        Ok(None) => {
            return Err(not_found(format!(
                "{} has no version information, so it is not the D2R game",
                exe.display()
            )))
        }
        Err(e) => {
            return Err(not_found(format!(
                "Could not read the version of {}: {e}",
                exe.display()
            )))
        }
    };
    let key = same_folder_key(&root);
    let store = if registry.iter().any(|r| same_folder_key(r) == key) {
        GameStore::BattleNet
    } else {
        GameStore::Unknown
    };
    Ok(GameLocation {
        path: root,
        build,
        store,
    })
}

/// A Windows symlink, junction or other reparse point (or a Unix symlink).
pub(super) fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

/// Two spellings of one folder give the same key: canonical when the folder
/// exists, absolute otherwise, uppercased, without a trailing separator.
fn same_folder_key(path: &Path) -> String {
    let resolved = fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf());
    resolved
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_uppercase()
}
