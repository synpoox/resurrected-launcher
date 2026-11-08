//! App/Tauri module: narrow typed commands, progress/error events and the
//! window/tray/single-instance lifecycle.
//!
//! Commands are [`ping`], the install operation commands in [`operations`] (#40)
//! and the D2R location, readiness, catalog and Play commands in [`game`] (#60).
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

use std::sync::Arc;

use tauri::path::BaseDirectory;
use tauri::{ipc::Invoke, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::contract::{PingResponse, Versioned, UMBRELLA_DISPLAY_NAME};
use crate::game::SystemGameHost;
use crate::install::download::Downloader;
use crate::install::extract::SevenZip;
use crate::install::preflight::SystemHost;
use crate::state::StateStore;

pub use game::{CatalogState, Desktop, ProductState, SystemDesktop, STATE_EVENT};
pub use operations::{
    CachedReleases, ConflictView, InspectionView, Launcher, RecoveryView, Release, ReleaseSummary,
    Releases, OUTCOME_EVENT, PROGRESS_EVENT,
};

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
        game::open_loader_download_page
    ]
}

/// Application entry point.
pub fn run() {
    tauri::Builder::default()
        // Rust-side folder picker only: capabilities grant no `dialog:` permission.
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(invoke_handler())
        .setup(|app| {
            let paths = app.path();
            let seven_zip = paths.resolve("resources/7zip/7za.exe", BaseDirectory::Resource)?;
            let launcher = Launcher::new(
                StateStore::open(&paths.app_local_data_dir()?),
                Box::new(CachedReleases),
                Downloader::new(),
                SevenZip::bundled(&seven_zip)?,
                Box::new(SystemHost),
                Box::new(SystemGameHost),
                Box::new(SystemDesktop(app.handle().clone())),
            );
            let launcher = Arc::new(launcher);
            app.manage(Arc::clone(&launcher));
            let handle = app.handle().clone();
            std::thread::spawn(move || launcher.start_up(&handle));
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
        let cmds = [
            "read_file",
            "plugin:fs|read_file",
            "plugin:shell|execute",
            "plugin:dialog|open",
            "plugin:opener|open_url",
        ];
        for cmd in cmds {
            assert!(invoke_with(invoke_handler(), cmd).is_err(), "{cmd}");
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

    /// Every command registered by non-test code anywhere under `src/`: each
    /// `generate_handler!` before a file's `#[cfg(test)]` (#53 item 5).
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
                    let code = text.split("#[cfg(test)]").next().unwrap();
                    out.extend(listed(code, "generate_handler!"));
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
    }

    #[test]
    fn capabilities_grant_only_the_allowlisted_commands() {
        let allowed: Vec<String> = COMMANDS
            .iter()
            .map(|c| format!("allow-{}", c.replace('_', "-")))
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
