//! Window controls for the frameless title bar and Quit (#74). Each acts on the
//! calling window, which must be `main`; capabilities grant them to `main` only,
//! and no `core:window` permission is granted. #75 turns close into hide-to-tray.

use std::sync::Arc;

use tauri::{AppHandle, Runtime, State, WebviewWindow};

use super::operations::{invalid, Launcher};
use crate::contract::{ErrorCode, LauncherError};

/// Event carrying `Versioned<TrayRequest>`; emitted by the tray (#75).
pub const TRAY_EVENT: &str = "tray-request";

fn main_only<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), LauncherError> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err(invalid("window controls act on the main window only"))
    }
}

fn failed(e: tauri::Error) -> LauncherError {
    LauncherError::new(ErrorCode::Internal, e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn minimize_window<R: Runtime>(window: WebviewWindow<R>) -> Result<(), LauncherError> {
    main_only(&window)?;
    window.minimize().map_err(failed)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn toggle_maximize_window<R: Runtime>(
    window: WebviewWindow<R>,
) -> Result<(), LauncherError> {
    main_only(&window)?;
    if window.is_maximized().map_err(failed)? {
        window.unmaximize().map_err(failed)
    } else {
        window.maximize().map_err(failed)
    }
}

/// A normal close request, which #75 turns into hide-to-tray.
#[tauri::command(rename_all = "snake_case")]
pub async fn close_window<R: Runtime>(window: WebviewWindow<R>) -> Result<(), LauncherError> {
    main_only(&window)?;
    window.close().map_err(failed)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn start_window_drag<R: Runtime>(window: WebviewWindow<R>) -> Result<(), LauncherError> {
    main_only(&window)?;
    window.start_dragging().map_err(failed)
}

/// `quit_launcher`: exits; refused with `OperationInProgress` while an operation
/// or recovery runs. A running game keeps running.
#[tauri::command(rename_all = "snake_case")]
pub async fn quit_launcher<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    launcher: State<'_, Arc<Launcher>>,
) -> Result<(), LauncherError> {
    main_only(&window)?;
    // Held while exiting, so no operation starts in between.
    let running = launcher.running();
    running.refuse_any()?;
    app.exit(0);
    Ok(())
}
