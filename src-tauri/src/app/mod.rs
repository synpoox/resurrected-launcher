//! App/Tauri module: narrow typed commands, progress/error events and the
//! window/tray/single-instance lifecycle.
//!
//! Commands are [`ping`], the install operation commands in [`operations`] (#40)
//! and the D2R location, readiness, catalog and Play commands in [`game`] (#60),
//! the Settings and troubleshooting commands in [`settings`] and the window
//! controls in [`window`] (#74).
//! There is no generic shell, filesystem, path, URL, download or execute command,
//! and there must never be one: arguments from the frontend are product IDs,
//! intents, consents and inspection tokens only, and Rust validates every request.
//! Tray and single instance arrive with slice 4.
//!
//! # Adding a command
//!
//! Registering a command in [`invoke_handler`] is not enough: the ACL refuses every
//! app command that is not also listed in `build.rs` (`AppManifest::commands`) and
//! granted as `allow-<name>` in `capabilities/`. The tests below compare all three
//! with the reviewed `COMMANDS` list; `src/ipc.test.ts` compares the frontend
//! bindings with build.rs.
//!
//! # Provisional names
//!
//! `tauri.conf.json` uses the technical `productName` `resurrected-launcher` and the
//! identifier `com.projectd2r.launcher` on purpose, so the provisional display name
//! stays in one place, [`crate::contract::UMBRELLA_DISPLAY_NAME`]. The identifier
//! follows the planned projectd2r.com domain and is equally provisional; changing it
//! later moves the per-user data directory.

mod game;
mod operations;
mod settings;
mod winapi;
mod window;

#[cfg(test)]
use tauri::WebviewWindowBuilder;
use tauri::{ipc::Invoke, Runtime};

use crate::contract::{PingResponse, Versioned, UMBRELLA_DISPLAY_NAME};

pub use game::{
    CatalogState, Desktop, LocationView, ProductState, StartupFailed, SystemDesktop, STATE_EVENT,
};
pub use operations::{
    CachedReleases, ConflictView, InspectionView, Launcher, RecoveryView, Refusal, Release,
    ReleaseSummary, Releases, OUTCOME_EVENT, PROGRESS_EVENT,
};
pub use settings::{Folder, ReleaseNotes};
pub use window::TRAY_EVENT;
// The tray (crate::lifecycle) calls the same commands natively, with every check.
pub(crate) use game::{get_product_state, play_product, refresh_catalog};
pub(crate) use operations::{cancel_operation, inspect_operation, start_operation};

/// `ping` command: contract version, launcher version and the umbrella display name.
#[tauri::command]
pub fn ping() -> Versioned<PingResponse> {
    Versioned::new(PingResponse {
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        display_name: UMBRELLA_DISPLAY_NAME.to_owned(),
    })
}

/// The registered command handler (generic so tests can use the mock runtime).
/// Commands that touch installations need an `Arc<Launcher>` in managed state.
pub fn invoke_handler<R: Runtime>() -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    // The allowlist test reads every handler macro under src/ outside tests.
    tauri::generate_handler![
        ping,
        game::get_product_state,
        operations::inspect_operation,
        operations::start_operation,
        operations::cancel_operation,
        operations::recover_product,
        game::detect_game_locations,
        game::choose_game_location,
        game::refresh_catalog,
        game::play_product,
        game::open_loader_download_page,
        game::select_game_location,
        settings::get_release_notes,
        settings::open_folder,
        settings::preview_diagnostics,
        settings::save_diagnostics,
        settings::show_diagnostics_in_folder,
        settings::get_start_with_windows,
        settings::set_start_with_windows,
        window::minimize_window,
        window::toggle_maximize_window,
        window::close_window,
        window::start_window_drag,
        window::quit_launcher
    ]
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
        let cmds = [
            "read_file",
            "plugin:fs|read_file",
            "plugin:shell|execute",
            "plugin:opener|open_url",
        ];
        for cmd in cmds {
            assert!(invoke_with(invoke_handler(), cmd).is_err(), "{cmd}");
        }
    }

    /// The dialog plugin is registered (for the Rust-side picker), but no
    /// capability grants its commands to the webview.
    #[test]
    fn registered_dialog_plugin_commands_are_refused_by_the_acl() {
        let app = mock_builder()
            .plugin(tauri_plugin_dialog::init())
            .invoke_handler(invoke_handler())
            .build(tauri::generate_context!())
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        for cmd in [
            "plugin:dialog|open",
            "plugin:dialog|save",
            "plugin:dialog|message",
        ] {
            let err = get_ipc_response(
                &webview,
                InvokeRequest {
                    cmd: cmd.into(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    url: "http://tauri.localhost".parse().unwrap(),
                    body: InvokeBody::Json(serde_json::json!({ "options": {} })),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.to_string(),
                },
            )
            .unwrap_err();
            assert!(err.to_string().contains("not allowed"), "{cmd}: {err}");
        }
    }

    /// #79: the webview may subscribe to the launcher's events, but never emit one.
    #[test]
    fn webview_may_listen_to_events_but_not_emit() {
        let app = mock_builder()
            .invoke_handler(invoke_handler())
            .build(tauri::generate_context!())
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let call = |cmd: &str, body: serde_json::Value| {
            get_ipc_response(
                &webview,
                InvokeRequest {
                    cmd: cmd.into(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    url: "http://tauri.localhost".parse().unwrap(),
                    body: InvokeBody::Json(body),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.to_string(),
                },
            )
        };
        let target = serde_json::json!({ "kind": "Any" });
        let id = call(
            "plugin:event|listen",
            serde_json::json!({ "event": "operation-progress", "target": target, "handler": 5 }),
        )
        .expect("listen is granted")
        .deserialize::<u32>()
        .unwrap();
        call(
            "plugin:event|unlisten",
            serde_json::json!({ "event": "operation-progress", "eventId": id }),
        )
        .expect("unlisten is granted");
        for cmd in ["plugin:event|emit", "plugin:event|emit_to"] {
            let err = call(
                cmd,
                serde_json::json!({ "event": "product-state-changed", "target": target, "payload": null }),
            )
            .unwrap_err();
            assert!(err.to_string().contains("not allowed"), "{cmd}: {err}");
        }
    }

    /// #53 item 5: a command that is registered and listed in build.rs, but not
    /// granted to the calling window, is refused.
    #[test]
    fn registered_and_listed_but_ungranted_command_is_refused() {
        let app = mock_builder()
            .invoke_handler(invoke_handler())
            .build(tauri::generate_context!())
            .unwrap();
        // capabilities/ grant only to the "main" window.
        let other = WebviewWindowBuilder::new(&app, "other", Default::default())
            .build()
            .unwrap();
        let err = get_ipc_response(
            &other,
            InvokeRequest {
                cmd: "ping".into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "http://tauri.localhost".parse().unwrap(),
                body: InvokeBody::default(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("not allowed"), "{err}");
    }

    /// The command allowlist. Adding a command means adding it here, in
    /// `invoke_handler`, in build.rs and in capabilities/, and passing review: no
    /// generic shell, filesystem, path, download or execute command.
    const COMMANDS: &[&str] = &[
        "ping",
        "get_product_state",
        "inspect_operation",
        "start_operation",
        "cancel_operation",
        "recover_product",
        "detect_game_locations",
        "choose_game_location",
        "refresh_catalog",
        "play_product",
        "open_loader_download_page",
        "select_game_location",
        "get_release_notes",
        "open_folder",
        "preview_diagnostics",
        "save_diagnostics",
        "show_diagnostics_in_folder",
        "get_start_with_windows",
        "set_start_with_windows",
        "minimize_window",
        "toggle_maximize_window",
        "close_window",
        "start_window_drag",
        "quit_launcher",
    ];

    fn root_file(rel: &str) -> String {
        std::fs::read_to_string(format!("{}/{rel}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    /// The identifiers in every `[...]` after a `marker` in `text`, in order.
    fn listed(text: &str, marker: &str) -> Vec<String> {
        let mut found = Vec::new();
        for (at, _) in text.match_indices(marker) {
            let rest = &text[at + marker.len()..];
            let list = &rest[rest.find('[').unwrap() + 1..rest.find(']').unwrap()];
            found.extend(
                list.split(',')
                    .map(|c| {
                        c.trim()
                            .trim_matches('"')
                            .rsplit("::")
                            .next()
                            .unwrap()
                            .to_owned()
                    })
                    .filter(|c| !c.is_empty()),
            );
        }
        found
    }

    /// `text` without its `mod tests { ... }` blocks (braces counted).
    fn without_test_modules(text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        // Spelled with an escape so this source text keeps its braces balanced.
        const OPEN: &str = "mod tests \u{7b}";
        while let Some(at) = rest.find(OPEN) {
            out.push_str(&rest[..at]);
            let body = &rest[at + OPEN.len()..];
            let mut depth = 1;
            let end = body
                .char_indices()
                .find(|&(_, c)| {
                    match c {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                    depth == 0
                })
                .map_or(body.len(), |(i, _)| i + 1);
            rest = &body[end..];
        }
        out.push_str(rest);
        out
    }

    /// Every command registered by non-test code anywhere under `src/`: each
    /// `generate_handler!` outside `mod tests` blocks (#53 item 5).
    fn registered() -> Vec<String> {
        fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
            let mut entries: Vec<_> = std::fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            entries.sort();
            for path in entries {
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    out.extend(listed(&without_test_modules(&text), "generate_handler!"));
                }
            }
        }
        let mut found = Vec::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut found,
        );
        found
    }

    #[test]
    fn registered_commands_match_the_allowlist() {
        assert_eq!(registered(), COMMANDS, "invoke_handler");
        let build = root_file("build.rs");
        assert_eq!(
            listed(&build, "AppManifest::new().commands("),
            COMMANDS,
            "build.rs"
        );
    }

    #[test]
    fn allowlist_check_catches_an_extra_registered_command() {
        let sneaky = "tauri::generate_handler![ping, run_anything]";
        assert_ne!(listed(sneaky, "generate_handler!"), COMMANDS);
        // A second handler elsewhere is read too, not only the first.
        let second = "generate_handler![ping] generate_handler![run_anything]";
        assert_eq!(
            listed(second, "generate_handler!"),
            ["ping", "run_anything"]
        );
        // Code after a test module still counts; the test module does not.
        let after = "mod tests { fn f() { generate_handler![a] } } generate_handler![b]";
        assert_eq!(
            listed(&without_test_modules(after), "generate_handler!"),
            ["b"]
        );
    }

    /// The only core permissions granted: the webview subscribes to events but never emits.
    const EVENT_PERMISSIONS: &[&str] = &["core:event:allow-listen", "core:event:allow-unlisten"];

    #[test]
    fn capabilities_grant_only_the_allowlisted_commands() {
        // Plus subscribing to the launcher's events (#79): listen and unlisten only, never emit.
        let allowed: Vec<String> = COMMANDS
            .iter()
            .map(|c| format!("allow-{}", c.replace('_', "-")))
            .chain(EVENT_PERMISSIONS.iter().map(|p| p.to_string()))
            .collect();
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
        assert_eq!(granted, allowed);

        // Capabilities may also be declared inline in tauri.conf.json; none are allowed there.
        let conf: serde_json::Value = serde_json::from_str(&root_file("tauri.conf.json")).unwrap();
        let inline = &conf["app"]["security"]["capabilities"];
        assert!(
            inline.is_null() || inline.as_array().is_some_and(|a| a.is_empty()),
            "inline capabilities in tauri.conf.json: {inline}"
        );
    }
}
