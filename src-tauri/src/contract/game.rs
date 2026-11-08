//! Slice 3 game DTOs (#55): the D2R location, a product's readiness to play, the
//! pinned loader installer offer and the running game. Wire names are snake_case.
//!
//! Offline single-player only: nothing here can name a mode, server or TCP/IP game.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::ErrorCode;

/// Where a D2R installation was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameStore {
    /// The folder is the `InstallLocation` of a Battle.net D2R uninstall entry.
    BattleNet,
    /// Remembered or chosen manually, and not a Battle.net entry.
    Unknown,
}

/// A validated D2R installation: `path\D2R.exe` was a regular file (not a link) with
/// readable version info when this was built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameLocation {
    /// The D2R root (the folder holding `D2R.exe`), absolute.
    pub path: PathBuf,
    /// `D2R.exe`'s PE ProductVersion, e.g. `3.3.93847`. A label for display only:
    /// support is decided by size and SHA-256 against `compatibility.game_builds`.
    pub build: String,
    pub store: GameStore,
}

/// Whether one product can be played at the selected location, and if not, the
/// first reason, in this precedence order (declaration order):
///
/// 1. `d2r_missing` 2. `running` 3. `recovery_required` 4. `build_unsupported`
/// 5. `loader_missing` 6. `loader_unsupported` 7. `not_installed`
/// 8. `update_available` 9. `ready`.
///
/// Build and loader checks use the installed release's manifest, or the
/// recommended one when the product is not installed. Never `ready` unless verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Readiness {
    /// No D2R location is selected, or the selected one no longer validates.
    D2rMissing,
    /// A process whose image is under the D2R root is running (any product, or one
    /// started outside the launcher). Every operation waits.
    Running {
        /// Earliest start of those processes, seconds since the Unix epoch.
        since: u64,
    },
    /// An interrupted transaction is pending; startup recovery must resolve it.
    RecoveryRequired,
    /// `D2R.exe`'s size and SHA-256 match no `compatibility.game_builds` entry.
    BuildUnsupported {
        /// [`GameLocation::build`] of the unsupported executable.
        found: String,
    },
    /// `D2RLoader.exe` or `D2RCore.dll` is absent or has no readable ProductVersion.
    LoaderMissing,
    /// A loader file's ProductVersion is not in `compatibility.loader_versions`.
    LoaderUnsupported {
        /// The first disallowed ProductVersion, `D2RLoader.exe` checked before `D2RCore.dll`.
        found: String,
        allowed: Vec<String>,
    },
    /// The product has no installed-release record.
    NotInstalled,
    /// Playable; the feed recommends a newer release than the installed one.
    UpdateAvailable,
    /// Playable; the installed release is the recommended one, or the feed is
    /// unavailable (offline) and the installed release was verified.
    Ready,
}

impl Readiness {
    /// Why Play is refused, or `None` when it is allowed (`ready` and
    /// `update_available`: the player may play the installed release).
    pub fn play_refusal(&self) -> Option<ErrorCode> {
        match self {
            Readiness::D2rMissing => Some(ErrorCode::D2rNotFound),
            Readiness::Running { .. } => Some(ErrorCode::GameRunning),
            Readiness::RecoveryRequired => Some(ErrorCode::RecoveryRequired),
            Readiness::BuildUnsupported { .. } => Some(ErrorCode::GameBuildUnsupported),
            Readiness::LoaderMissing | Readiness::LoaderUnsupported { .. } => {
                Some(ErrorCode::LoaderNotReady)
            }
            Readiness::NotInstalled => Some(ErrorCode::ProductNotInstalled),
            Readiness::UpdateAvailable | Readiness::Ready => None,
        }
    }
}

/// Guidance shown when D2RLoader is missing or unsupported. The launcher never
/// downloads or runs the loader installer: the player installs it from the
/// official page, then uses "Check again", which reassesses readiness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderGuidance {
    /// The disallowed ProductVersion found, `None` when the loader is missing.
    pub found: Option<String>,
    /// The loader versions the release allows (`compatibility.loader_versions`).
    pub allowed: Vec<String>,
    /// Always [`LOADER_DOWNLOAD_PAGE`].
    pub download_page: String,
}

/// The official D2RLoader download page, the only place guidance points to.
pub const LOADER_DOWNLOAD_PAGE: &str = "https://d2rloader.net/download.html";

/// One process whose image is under the selected D2R root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningGame {
    pub pid: u32,
    /// Full image path, e.g. `<root>\D2RLoader.exe` or `<root>\D2R.exe`.
    pub image: PathBuf,
    /// Process start, seconds since the Unix epoch.
    pub since: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::Versioned;
    use serde_json::{json, Value};

    fn round_trip<T>(value: &T, wire: Value)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
        assert_eq!(&serde_json::from_value::<T>(wire).unwrap(), value);
    }

    #[test]
    fn game_location_wire_shape() {
        let loc = GameLocation {
            path: PathBuf::from(r"C:\Games\Diablo II Resurrected"),
            build: "3.3.93847".into(),
            store: GameStore::BattleNet,
        };
        round_trip(
            &loc,
            json!({ "path": r"C:\Games\Diablo II Resurrected", "build": "3.3.93847", "store": "battle_net" }),
        );
        round_trip(&GameStore::Unknown, json!("unknown"));
    }

    #[test]
    fn readiness_wire_shape_is_status_tagged() {
        let cases = [
            (Readiness::D2rMissing, json!({ "status": "d2r_missing" })),
            (
                Readiness::Running {
                    since: 1_700_000_000,
                },
                json!({ "status": "running", "since": 1_700_000_000u64 }),
            ),
            (
                Readiness::RecoveryRequired,
                json!({ "status": "recovery_required" }),
            ),
            (
                Readiness::BuildUnsupported {
                    found: "3.3.1".into(),
                },
                json!({ "status": "build_unsupported", "found": "3.3.1" }),
            ),
            (
                Readiness::LoaderMissing,
                json!({ "status": "loader_missing" }),
            ),
            (
                Readiness::LoaderUnsupported {
                    found: "1.2.0".into(),
                    allowed: vec!["1.3.1-beta".into()],
                },
                json!({ "status": "loader_unsupported", "found": "1.2.0", "allowed": ["1.3.1-beta"] }),
            ),
            (
                Readiness::NotInstalled,
                json!({ "status": "not_installed" }),
            ),
            (
                Readiness::UpdateAvailable,
                json!({ "status": "update_available" }),
            ),
            (Readiness::Ready, json!({ "status": "ready" })),
        ];
        for (readiness, wire) in cases {
            round_trip(&readiness, wire);
        }
    }

    #[test]
    fn only_ready_and_update_available_allow_play() {
        assert_eq!(Readiness::Ready.play_refusal(), None);
        assert_eq!(Readiness::UpdateAvailable.play_refusal(), None);
        assert_eq!(
            Readiness::Running { since: 0 }.play_refusal(),
            Some(ErrorCode::GameRunning)
        );
        assert_eq!(
            Readiness::LoaderUnsupported {
                found: String::new(),
                allowed: vec![]
            }
            .play_refusal(),
            Some(ErrorCode::LoaderNotReady)
        );
        let codes = serde_json::to_value([
            ErrorCode::D2rNotFound,
            ErrorCode::GameBuildUnsupported,
            ErrorCode::LoaderNotReady,
            ErrorCode::ProductNotInstalled,
            ErrorCode::LaunchFailed,
        ])
        .unwrap();
        assert_eq!(
            codes,
            json!([
                "d2r_not_found",
                "game_build_unsupported",
                "loader_not_ready",
                "product_not_installed",
                "launch_failed"
            ])
        );
    }

    #[test]
    fn loader_guidance_and_running_game_wire_shape() {
        let guidance = LoaderGuidance {
            found: None,
            allowed: vec!["1.3.1-beta".into()],
            download_page: LOADER_DOWNLOAD_PAGE.into(),
        };
        round_trip(
            &guidance,
            json!({
                "found": null,
                "allowed": ["1.3.1-beta"],
                "download_page": "https://d2rloader.net/download.html"
            }),
        );
        let game = RunningGame {
            pid: 4242,
            image: PathBuf::from(r"C:\D2R\D2RLoader.exe"),
            since: 1_700_000_000,
        };
        let v = serde_json::to_value(Versioned::new(game.clone())).unwrap();
        assert_eq!(
            v["data"],
            json!({ "pid": 4242, "image": r"C:\D2R\D2RLoader.exe", "since": 1_700_000_000u64 })
        );
        round_trip(&game, v["data"].clone());
    }
}
