//! Native core of the launcher.
//!
//! Modules follow the architecture table in issue #8 (ordinary modules, not crates):
//!
//! | Module | Owns |
//! | --- | --- |
//! | [`app`] | Narrow typed Tauri commands and events. |
//! | [`lifecycle`] | Window, tray, single instance, close-to-tray, Quit and `--autostart`. |
//! | [`catalog`] | Product catalog, release feed and manifest reading/validation. |
//! | [`install`] | Install, adopt, repair, update, uninstall, cancel and restart recovery. |
//! | [`game`] | D2R detection, readiness, explicit product launch and process tracking. |
//! | [`state`] | Preferences, cached metadata, installed-release records, local diagnostics. |
//!
//! [`contract`] holds the frozen types shared between them and the frontend.

pub mod app;
pub mod catalog;
pub mod contract;
pub mod game;
pub mod install;
pub mod lifecycle;
pub mod state;

pub use lifecycle::run;
