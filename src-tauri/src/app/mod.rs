//! App/Tauri module: narrow typed commands, progress/error events and the
//! window/tray/single-instance lifecycle.
//!
//! The only command is [`ping`]. There is no generic shell, filesystem, download or
//! execute command, and there must never be one: arguments from the frontend are
//! product IDs and intents only, and Rust validates every request. Operation
//! commands arrive with #40; tray and single instance with slice 4.

use tauri::{ipc::Invoke, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::contract::{PingResponse, Versioned, UMBRELLA_DISPLAY_NAME};

/// Every command the frontend may invoke. Keep in step with [`invoke_handler`].
pub const COMMANDS: &[&str] = &["ping"];

/// `ping` command: contract version, launcher version and the umbrella display name.
#[tauri::command]
pub fn ping() -> Versioned<PingResponse> {
    Versioned::new(PingResponse {
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        display_name: UMBRELLA_DISPLAY_NAME.to_owned(),
    })
}

/// The registered command handler (generic so tests can use the mock runtime).
pub fn invoke_handler<R: Runtime>() -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![ping]
}

/// Application entry point.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(invoke_handler())
        .setup(|app| {
            // Built here so the title comes from the one display-name constant.
            // shortcut: native decorations until slice 4 adds the frameless title bar.
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title(UMBRELLA_DISPLAY_NAME)
                .inner_size(1280.0, 720.0)
                .min_inner_size(1024.0, 640.0)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("launcher failed to start");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::CONTRACT_VERSION;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
    use tauri::webview::InvokeRequest;

    fn invoke(cmd: &str) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
        let app = mock_builder()
            .invoke_handler(invoke_handler())
            .build(tauri::generate_context!())
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "http://tauri.localhost".parse().unwrap(),
                body: InvokeBody::default(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
    }

    #[test]
    fn ping_over_ipc_reports_contract_version_and_display_name() {
        let body = invoke("ping").unwrap();
        let reply: Versioned<PingResponse> = body.deserialize().unwrap();
        assert_eq!(reply.contract_version, CONTRACT_VERSION);
        assert_eq!(reply.data.display_name, "Project Resurrected");
        assert_eq!(reply.data.app_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn unregistered_commands_are_refused() {
        for cmd in ["read_file", "plugin:fs|read_file", "plugin:shell|execute"] {
            assert!(invoke(cmd).is_err(), "{cmd} must not be invocable");
        }
    }

    #[test]
    fn capabilities_grant_no_permissions() {
        // shortcut: allowlist is empty; #40 adds exactly what events need and the command allowlist test.
        const ALLOWED: &[&str] = &[];
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/capabilities");
        for entry in std::fs::read_dir(dir).unwrap() {
            let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            let cap: serde_json::Value = serde_json::from_str(&text).unwrap();
            for perm in cap["permissions"].as_array().unwrap() {
                let id = perm.as_str().or(perm["identifier"].as_str()).unwrap();
                assert!(ALLOWED.contains(&id), "capability grants {id}");
            }
        }
        assert_eq!(COMMANDS, ["ping"]);
    }
}
