//! App/Tauri module: narrow typed commands, progress/error events and the
//! window/tray/single-instance lifecycle.
//!
//! The only command is [`ping`]. There is no generic shell, filesystem, download or
//! execute command, and there must never be one: arguments from the frontend are
//! product IDs and intents only, and Rust validates every request.
//!
//! Tauri wiring lands in a follow-up PR for #32; operation commands come with #40.

use crate::contract::{PingResponse, Versioned, UMBRELLA_DISPLAY_NAME};

/// `ping` command: contract version, launcher version and the umbrella display name.
pub fn ping() -> Versioned<PingResponse> {
    Versioned::new(PingResponse {
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        display_name: UMBRELLA_DISPLAY_NAME.to_owned(),
    })
}

/// Application entry point.
pub fn run() {
    // shortcut: no window yet; the Tauri builder replaces this in the follow-up PR for #32.
    let info = ping();
    println!("{} {}", info.data.display_name, info.data.app_version);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::CONTRACT_VERSION;

    #[test]
    fn ping_reports_contract_version_and_display_name() {
        let reply = ping();
        assert_eq!(reply.contract_version, CONTRACT_VERSION);
        assert_eq!(reply.data.display_name, "Project Resurrected");
        assert_eq!(reply.data.app_version, env!("CARGO_PKG_VERSION"));
    }
}
