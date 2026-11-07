//! Native core of the launcher.
//!
//! Modules follow the architecture table in issue #8 (ordinary modules, not crates):
//!
//! | Module | Owns |
//! | --- | --- |
//! | [`app`] | Narrow typed Tauri commands, events, window/tray/single-instance lifecycle. |
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
pub mod state;

pub use app::run;
