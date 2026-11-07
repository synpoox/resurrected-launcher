//! App/Tauri module: narrow typed commands, progress/error events and the
//! window/tray/single-instance lifecycle.
//!
//! The only command is [`ping`]. There is no generic shell, filesystem, download or
//! execute command, and there must never be one: arguments from the frontend are
//! product IDs and intents only, and Rust validates every request. Operation
//! commands arrive with #40; tray and single instance with slice 4.
//!
//! # Adding a command
//!
//! Registering a command in [`invoke_handler`] is not enough: the ACL refuses every
//! app command that is not also listed in `build.rs` (`AppManifest::commands`) and
//! granted as `allow-<name>` in `capabilities/`. The tests below check both sides.
//!
//! # Provisional names
//!
//! `tauri.conf.json` uses the technical `productName` `resurrected-launcher` and the
//! identifier `com.projectd2r.launcher` on purpose, so the provisional display name
//! stays in one place, [`crate::contract::UMBRELLA_DISPLAY_NAME`]. The identifier
//! follows the planned projectd2r.com domain and is equally provisional; changing it
//! later moves the per-user data directory.

use tauri::{ipc::Invoke, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::contract::{PingResponse, Versioned, UMBRELLA_DISPLAY_NAME};

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
    use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
    use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
    use tauri::webview::InvokeRequest;

    /// Stands in for a command someone registers without listing it in build.rs.
    #[tauri::command]
    fn run_anything(cmd: String) -> String {
        cmd
    }

    fn invoke_with(
        handler: impl Fn(Invoke<MockRuntime>) -> bool + Send + Sync + 'static,
        cmd: &str,
    ) -> Result<InvokeResponseBody, serde_json::Value> {
        let app = mock_builder()
            .invoke_handler(handler)
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
                body: InvokeBody::Json(serde_json::json!({ "cmd": "whoami" })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
    }

    #[test]
    fn ping_over_ipc_reports_contract_version_and_display_name() {
        let body = invoke_with(invoke_handler(), "ping").unwrap();
        let reply: Versioned<PingResponse> = body.deserialize().unwrap();
        assert_eq!(reply.contract_version, CONTRACT_VERSION);
        assert_eq!(reply.data.display_name, "Project Resurrected");
        assert_eq!(reply.data.app_version, env!("CARGO_PKG_VERSION"));
    }

    fn handler_with_unlisted_command(
    ) -> impl Fn(Invoke<MockRuntime>) -> bool + Send + Sync + 'static {
        tauri::generate_handler![ping, run_anything]
    }

    #[test]
    fn registered_but_unlisted_command_is_refused_by_the_acl() {
        let err = invoke_with(handler_with_unlisted_command(), "run_anything").unwrap_err();
        assert!(err.to_string().contains("not allowed"), "{err}");
        // The same handler still serves the listed command.
        assert!(invoke_with(handler_with_unlisted_command(), "ping").is_ok());
    }

    #[test]
    fn plugin_and_unknown_commands_are_refused() {
        for cmd in ["read_file", "plugin:fs|read_file", "plugin:shell|execute"] {
            assert!(invoke_with(invoke_handler(), cmd).is_err(), "{cmd}");
        }
    }

    #[test]
    fn capabilities_grant_only_allow_ping() {
        // #40 extends this list deliberately, together with build.rs.
        const ALLOWED: &[&str] = &["allow-ping"];
        let root = env!("CARGO_MANIFEST_DIR");
        let mut granted = Vec::new();
        for entry in std::fs::read_dir(format!("{root}/capabilities")).unwrap() {
            let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            let cap: serde_json::Value = serde_json::from_str(&text).unwrap();
            for perm in cap["permissions"].as_array().unwrap() {
                granted.push(
                    perm.as_str()
                        .or(perm["identifier"].as_str())
                        .unwrap()
                        .to_owned(),
                );
            }
        }
        assert_eq!(granted, ALLOWED);

        // Capabilities may also be declared inline in tauri.conf.json; none are allowed there.
        let conf: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(format!("{root}/tauri.conf.json")).unwrap(),
        )
        .unwrap();
        let inline = &conf["app"]["security"]["capabilities"];
        assert!(
            inline.is_null() || inline.as_array().is_some_and(|a| a.is_empty()),
            "inline capabilities in tauri.conf.json: {inline}"
        );
    }
}
