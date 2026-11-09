//! Window and tray lifecycle (#75, `docs/specs/launcher-ui.md` → Window and Tray):
//! the frameless main window, close-to-tray with the one first-close notice, the
//! tray icon and menu ([`tray`]), single instance, Quit and `--autostart`.
//!
//! The plugins added here are Rust-side only: capabilities grant them no
//! permission, so the webview cannot call their commands (`tests/lifecycle.rs`).

mod icon;
pub mod notice;
pub mod position;
pub mod tray;

use std::error::Error;
use std::sync::Arc;

use tauri::path::BaseDirectory;
use tauri::{
    App, AppHandle, Builder, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder, Window,
    WindowEvent,
};

use crate::app::{
    CachedReleases, Launcher, SelfUpdater, SystemDesktop, SystemUpdateHost, TRAY_EVENT, UPDATE_DIR,
};
use crate::contract::{
    LauncherError, ProductId, TrayAction, TrayRequest, Versioned, UMBRELLA_DISPLAY_NAME,
};
use crate::game::SystemGameHost;
use crate::install::download::Downloader;
use crate::install::extract::SevenZip;
use crate::install::preflight::SystemHost;
use crate::state::StateStore;

pub use tray::{play, quit, Model, Quit};

/// The flag the Start with Windows Run value passes (#74): start in the tray only.
pub const AUTOSTART_FLAG: &str = "--autostart";

/// Application entry point.
pub fn run() {
    let autostart = std::env::args().skip(1).any(|a| a == AUTOSTART_FLAG);
    builder(tauri::Builder::default(), move |app| {
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
        // Downloads stay in app data, never under D2R. Checks start from the webview.
        app.manage(Arc::new(SelfUpdater::new(
            paths.app_local_data_dir()?.join(UPDATE_DIR),
            app.package_info().version.to_string(),
            Box::new(SystemUpdateHost),
        )));
        let handle = app.handle().clone();
        std::thread::spawn(move || launcher.start_up(&handle));
        // Built hidden on autostart, so the frontend is listening when the tray
        // first opens it (tray requests are emitted right after showing).
        let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
            .title(UMBRELLA_DISPLAY_NAME)
            .decorations(false)
            .inner_size(1280.0, 720.0)
            // Centred on the primary monitor (#174) unless the last spot is still
            // on a connected monitor (#188, [`position`]).
            .center()
            .min_inner_size(1024.0, 640.0)
            // A fixed maximum, like the Riot Client, and no maximise (#135): the
            // layout is designed between these two sizes.
            .max_inner_size(1600.0, 900.0)
            .maximizable(false)
            // Built hidden and shown after the move, so it never flashes centred.
            .visible(false)
            .build()?;
        position::restore(&window);
        if !autostart {
            window.show()?;
        }
        tray::start(app.handle())?;
        Ok(())
    })
    // Launcher self-update (#88): key and endpoint from tauri.conf.json only. Added
    // here, after single instance, because it needs that config to start.
    .plugin(tauri_plugin_updater::Builder::new().build())
    .run(tauri::generate_context!())
    .expect("launcher failed to start");
}

/// The app builder. Single instance is the first plugin: a second launch focuses
/// the first window and exits during plugin setup, before `setup` runs, so it never
/// constructs a [`Launcher`] or takes the operation lock.
pub fn builder<R: Runtime>(
    builder: Builder<R>,
    setup: impl FnOnce(&mut App<R>) -> Result<(), Box<dyn Error>> + Send + 'static,
) -> Builder<R> {
    plugins(
        builder.plugin(tauri_plugin_single_instance::init(|app, args, _| {
            // A second instance started by Windows at sign-in stays in the tray.
            if !args.iter().any(|a| a == AUTOSTART_FLAG) {
                show_main(app)
            }
        })),
    )
    .invoke_handler(crate::app::invoke_handler())
    .on_window_event(hide_on_close)
    .setup(setup)
}

/// The Rust-side plugins besides single instance.
pub fn plugins<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder
        // Folder picker for choose_game_location (#60).
        .plugin(tauri_plugin_dialog::init())
}

/// Shows, restores and focuses the main window. Never hides it.
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Shows the window, then asks the frontend for `action` on `product_id`.
fn request<R: Runtime>(
    app: &AppHandle<R>,
    action: TrayAction,
    product_id: &ProductId,
    error: Option<LauncherError>,
) {
    show_main(app);
    let request = TrayRequest {
        action,
        product_id: product_id.clone(),
        error,
    };
    let _ = app.emit(TRAY_EVENT, Versioned::new(request));
}

/// Closing the main window hides it to the tray.
fn hide_on_close<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        if window.label() == "main" {
            api.prevent_close();
            position::save(window.app_handle());
            let _ = window.hide();
            notice::on_close(window.app_handle());
        }
    }
}
