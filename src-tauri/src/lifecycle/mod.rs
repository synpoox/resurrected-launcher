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
        let (width, height) = window_size(app.primary_monitor().ok().flatten().map(|m| {
            (
                f64::from(m.work_area().size.width) / m.scale_factor(),
                f64::from(m.work_area().size.height) / m.scale_factor(),
            )
        }));
        let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
            .title(UMBRELLA_DISPLAY_NAME)
            .decorations(false)
            .inner_size(width, height)
            // Centred on the primary monitor (#174) unless the last spot is still
            // on a connected monitor (#188, [`position`]).
            .center()
            // A fixed size, like the Riot Client (#203): not resizable, so no maximise either (#135).
            .resizable(false)
            .maximizable(false)
            // Built hidden and shown after the move, so it never flashes centred.
            .visible(false)
            // No Ctrl+zoom by keys, wheel or pinch (#185); the layout has fixed sizes.
            .zoom_hotkeys_enabled(false)
            .build()?;
        position::restore(&window);
        #[cfg(all(windows, not(debug_assertions)))]
        disable_browser_accelerator_keys(app);
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

/// The fixed logical window size (#203): 1600x900, or 1280x720 when the primary
/// monitor's work area (logical) is smaller than that or unknown.
fn window_size(work_area: Option<(f64, f64)>) -> (f64, f64) {
    match work_area {
        Some((w, h)) if w >= 1600.0 && h >= 900.0 => (1600.0, 900.0),
        _ => (1280.0, 720.0),
    }
}

/// Release builds turn off WebView2's browser keys (#185): F5/Ctrl+R reload, Ctrl+P,
/// Ctrl+F, Ctrl+S, Ctrl+U and the devtools keys. Tauri has no option for this, so it
/// goes through the WebView2 settings. Copy, paste and other editing keys still work.
#[cfg(all(windows, not(debug_assertions)))]
fn disable_browser_accelerator_keys(app: &App) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use windows_core::Interface;
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let result = window.with_webview(|webview| unsafe {
        let set = || -> windows_core::Result<()> {
            let settings = webview.controller().CoreWebView2()?.Settings()?;
            settings
                .cast::<ICoreWebView2Settings3>()?
                .SetAreBrowserAcceleratorKeysEnabled(false)
        };
        if let Err(e) = set() {
            eprintln!("could not turn off WebView2 browser keys: {e}");
        }
    });
    if let Err(e) = result {
        eprintln!("could not reach the WebView2 settings: {e}");
    }
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

#[cfg(test)]
mod window_size_tests {
    use super::window_size;

    #[test]
    fn large_work_area_gets_1600x900() {
        assert_eq!(window_size(Some((1920.0, 1040.0))), (1600.0, 900.0));
        assert_eq!(window_size(Some((1600.0, 900.0))), (1600.0, 900.0));
    }

    #[test]
    fn smaller_or_unknown_work_area_gets_1280x720() {
        assert_eq!(window_size(Some((1536.0, 824.0))), (1280.0, 720.0));
        assert_eq!(window_size(Some((1700.0, 860.0))), (1280.0, 720.0));
        assert_eq!(window_size(None), (1280.0, 720.0));
    }
}
