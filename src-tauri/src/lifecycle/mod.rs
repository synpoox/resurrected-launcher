//! Window and tray lifecycle (#75, `docs/specs/launcher-ui.md` → Window and Tray):
//! the frameless main window, close-to-tray with the one first-close notice, the
//! tray icon and menu ([`tray`]), single instance, Quit and `--autostart`.
//!
//! The plugins added here are Rust-side only: capabilities grant them no
//! permission, so the webview cannot call their commands (`tests/lifecycle.rs`).

mod icon;
pub mod tray;

use std::error::Error;
use std::sync::Arc;

use serde::Serialize;
use tauri::path::BaseDirectory;
use tauri::{
    App, AppHandle, Builder, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder, Window,
    WindowEvent,
};

use crate::app::{CachedReleases, Launcher, SystemDesktop};
use crate::contract::{ProductId, Versioned, UMBRELLA_DISPLAY_NAME};
use crate::game::SystemGameHost;
use crate::install::download::Downloader;
use crate::install::extract::SevenZip;
use crate::install::preflight::SystemHost;
use crate::state::StateStore;

pub use tray::{quit_plan, Quit};

/// The flag the Start with Windows Run value passes (#74): start in the tray only.
pub const AUTOSTART_FLAG: &str = "--autostart";

/// Event (`Versioned<TrayRequest>`): the tray opened the window and asks the
/// frontend to show something.
pub const TRAY_REQUEST_EVENT: &str = "tray-request";

/// Marker in app-local data: the first-close notice was shown.
const FIRST_CLOSE_MARKER: &str = "tray-notice-shown";

const FIRST_CLOSE_NOTICE: &str = "Still running in the tray. Downloads and update checks carry on here. To exit, right-click the icon and choose Quit.";

/// Payload of [`TRAY_REQUEST_EVENT`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrayRequest {
    pub action: TrayAction,
    pub product_id: ProductId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayAction {
    /// Update on a save-breaking release: the acknowledgement is needed first (ADR 0001).
    SaveBreaking,
    ShowWhatHappened,
    /// Quit while an operation runs; nothing exits until the frontend calls `quit_launcher`.
    QuitWhileBusy,
}

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
        let handle = app.handle().clone();
        std::thread::spawn(move || launcher.start_up(&handle));
        // Built hidden on autostart, so the frontend is listening when the tray
        // first opens it (tray requests are emitted right after showing).
        WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
            .title(UMBRELLA_DISPLAY_NAME)
            .decorations(false)
            .inner_size(1280.0, 720.0)
            .min_inner_size(1024.0, 640.0)
            .visible(!autostart)
            .build()?;
        tray::start(app.handle())?;
        Ok(())
    })
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
        builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app)
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
        // The first-close notice only.
        .plugin(tauri_plugin_notification::init())
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
fn request<R: Runtime>(app: &AppHandle<R>, action: TrayAction, product_id: &ProductId) {
    show_main(app);
    let request = TrayRequest {
        action,
        product_id: product_id.clone(),
    };
    let _ = app.emit(TRAY_REQUEST_EVENT, Versioned::new(request));
}

/// Closing the main window hides it to the tray.
fn hide_on_close<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        if window.label() == "main" {
            api.prevent_close();
            let _ = window.hide();
            first_close_notice(window.app_handle());
        }
    }
}

/// The one tray notification in v1, shown on the first close ever.
fn first_close_notice<R: Runtime>(app: &AppHandle<R>) {
    use tauri_plugin_notification::NotificationExt;
    let Ok(dir) = app.path().app_local_data_dir() else {
        return;
    };
    let marker = dir.join(FIRST_CLOSE_MARKER);
    // Written first: without a marker the notice would repeat on every close.
    if marker.exists() || std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if std::fs::write(&marker, b"").is_ok() {
        let _ = app
            .notification()
            .builder()
            .title(UMBRELLA_DISPLAY_NAME)
            .body(FIRST_CLOSE_NOTICE)
            .show();
    }
}
