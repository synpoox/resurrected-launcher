//! The first-close notice (#75, #104): the one tray notification in v1.
//!
//! Shown as a Windows toast under the launcher's own AppUserModelID, registered
//! per user under `HKCU\Software\Classes\AppUserModelId\<identifier>` so the toast
//! reads "Project Resurrected" wherever the exe runs from.
//!
//! Why (#104): tauri-plugin-notification, used before, sent the toast under the
//! bare identifier unless the exe sat in `target\debug` or `target\release`. Windows
//! accepts a toast for an AppUserModelID nothing registered (no shortcut carrying
//! it, no registry entry), reports success and shows no banner, so an exe copied
//! anywhere else never showed the notice. The plugin also dropped any error.
//!
//! The "shown" marker is written only after Windows accepted the toast. A failure
//! is logged to `launcher.log` and the next close tries again.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, Runtime};

use crate::contract::UMBRELLA_DISPLAY_NAME;

/// Marker in app-local data: the first-close notice was shown.
pub const MARKER: &str = "tray-notice-shown";

/// The notice text (`docs/specs/launcher-ui.md` → Window and Tray).
pub const TEXT: &str = "Still running in the tray. Downloads and update checks carry on here. To exit, right-click the icon and choose Quit.";

/// The icon the toast shows, written next to the marker for the registry's `IconUri`.
const ICON_FILE: &str = "notice-icon.png";
const ICON: &[u8] = include_bytes!("../../icons/128x128.png");

/// What one attempt did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The marker exists: the notice was shown before.
    AlreadyShown,
    /// Windows took the toast and the marker is written.
    Shown,
    /// The toast failed; no marker, so the next close tries again.
    Failed(String),
}

/// Shows the notice through `show` unless `dir` holds the marker, and writes the
/// marker only once `show` succeeded.
pub fn attempt(dir: &Path, show: impl FnOnce() -> Result<(), String>) -> Outcome {
    let marker = dir.join(MARKER);
    if marker.exists() {
        return Outcome::AlreadyShown;
    }
    if let Err(e) = show() {
        return Outcome::Failed(e);
    }
    match std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&marker, b"")) {
        Ok(()) => Outcome::Shown,
        // Shown, but it may show again on a later close.
        Err(e) => Outcome::Failed(format!("notice shown, marker not written: {e}")),
    }
}

/// Where the marker, icon and log go instead of app-local data, when managed.
/// Only tests manage it, so a test launcher never writes to the user's
/// `%LOCALAPPDATA%` (#129).
pub struct Dir(pub PathBuf);

/// The notice's directory: the managed [`Dir`], else app-local data.
pub fn dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    match app.try_state::<Dir>() {
        Some(dir) => Some(dir.0.clone()),
        None => app.path().app_local_data_dir().ok(),
    }
}

/// One attempt at a time: two quick closes must not show it twice.
static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Called on every close of the main window; shows the notice on the first one.
/// Runs off the event loop, since the toast call blocks briefly.
pub fn on_close<R: Runtime>(app: &AppHandle<R>) {
    let Some(dir) = dir(app) else {
        return;
    };
    if dir.join(MARKER).exists() || IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return;
    }
    let identifier = app.config().identifier.clone();
    let spawned = std::thread::Builder::new()
        .name("first-close-notice".into())
        .spawn(move || {
            let outcome = attempt(&dir, || show_toast(&dir, &identifier));
            if let Outcome::Failed(e) = &outcome {
                log(&dir, &format!("first-close notice not shown: {e}"));
            }
            IN_FLIGHT.store(false, Ordering::SeqCst);
        });
    if spawned.is_err() {
        IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}

/// Registers the launcher's AppUserModelID, then shows the toast under it.
#[cfg(windows)]
fn show_toast(dir: &Path, identifier: &str) -> Result<(), String> {
    use tauri_winrt_notification::{Duration, Sound, Toast};
    register_app_id(dir, identifier).map_err(|e| format!("AppUserModelID not registered: {e}"))?;
    Toast::new(identifier)
        .title(UMBRELLA_DISPLAY_NAME)
        .text1(TEXT)
        .duration(Duration::Short)
        .sound(Some(Sound::Default))
        .show()
        .map_err(|e| format!("toast failed: {e:?}"))
}

#[cfg(not(windows))]
fn show_toast(_: &Path, _: &str) -> Result<(), String> {
    Err("toasts are Windows only".into())
}

/// The per-user registration that lets an unpackaged exe show toasts under
/// `identifier` without a Start-menu shortcut carrying it.
pub fn app_id_key(identifier: &str) -> String {
    format!(r"Software\Classes\AppUserModelId\{identifier}")
}

#[cfg(windows)]
fn register_app_id(dir: &Path, identifier: &str) -> std::io::Result<()> {
    use crate::app::winapi::set_hkcu_value;
    let key = app_id_key(identifier);
    set_hkcu_value(&key, "DisplayName", Some(UMBRELLA_DISPLAY_NAME))?;
    // The icon is cosmetic: without it Windows shows a generic one.
    if let Ok(icon) = write_icon(dir) {
        set_hkcu_value(&key, "IconUri", Some(&icon.display().to_string()))?;
    }
    Ok(())
}

#[cfg(windows)]
fn write_icon(dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(ICON_FILE);
    if std::fs::read(&path).ok().as_deref() != Some(ICON) {
        std::fs::write(&path, ICON)?;
    }
    Ok(path)
}

/// Appends one line to `launcher.log` in the notice's directory.
fn log(dir: &Path, line: &str) {
    crate::state::log::append(dir, line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_close_shows_the_notice_and_writes_the_marker() {
        let dir = tempfile::tempdir().unwrap();
        let mut shown = 0;
        let outcome = attempt(dir.path(), || {
            shown += 1;
            Ok(())
        });
        assert_eq!(outcome, Outcome::Shown);
        assert_eq!(shown, 1);
        assert!(dir.path().join(MARKER).exists());
    }

    #[test]
    fn a_failed_notice_writes_no_marker_and_the_next_close_tries_again() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = attempt(dir.path(), || Err("toast failed: denied".into()));
        assert_eq!(outcome, Outcome::Failed("toast failed: denied".into()));
        assert!(!dir.path().join(MARKER).exists());
        assert_eq!(attempt(dir.path(), || Ok(())), Outcome::Shown);
    }

    #[test]
    fn the_notice_is_not_shown_once_the_marker_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MARKER), b"").unwrap();
        let outcome = attempt(dir.path(), || panic!("must not show"));
        assert_eq!(outcome, Outcome::AlreadyShown);
    }

    #[test]
    fn the_marker_dir_is_created_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        assert_eq!(attempt(&nested, || Ok(())), Outcome::Shown);
        assert!(nested.join(MARKER).exists());
    }

    #[test]
    fn failures_are_logged_to_launcher_log() {
        let dir = tempfile::tempdir().unwrap();
        log(dir.path(), "first-close notice not shown: x");
        let text = std::fs::read_to_string(dir.path().join(crate::state::log::LOG_FILE)).unwrap();
        assert!(
            text.ends_with("first-close notice not shown: x\n"),
            "{text}"
        );
    }

    /// Shows a real toast under a scratch ID, then removes the registration.
    /// Manual: `cargo test notice -- --ignored`, and watch for the banner.
    #[cfg(windows)]
    #[test]
    #[ignore = "shows a real Windows toast"]
    fn a_registered_app_id_shows_a_real_toast() {
        let dir = tempfile::tempdir().unwrap();
        let id = format!("com.projectd2r.launcher.test-{}", std::process::id());
        let shown = show_toast(dir.path(), &id);
        std::thread::sleep(std::time::Duration::from_secs(2));
        #[link(name = "advapi32")]
        extern "system" {
            fn RegDeleteTreeW(hkey: isize, sub_key: *const u16) -> i32;
        }
        let key: Vec<u16> = app_id_key(&id).encode_utf16().chain(Some(0)).collect();
        // SAFETY: NUL-terminated string; removes the scratch registration.
        unsafe { RegDeleteTreeW(0x8000_0001_u32 as i32 as isize, key.as_ptr()) };
        shown.unwrap();
    }

    #[test]
    fn a_managed_dir_replaces_app_local_data() {
        let app = tauri::test::mock_app();
        let local = app.path().app_local_data_dir().unwrap();
        assert_eq!(dir(app.handle()), Some(local));
        let tmp = tempfile::tempdir().unwrap();
        app.manage(Dir(tmp.path().to_path_buf()));
        assert_eq!(dir(app.handle()), Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn the_app_id_is_registered_under_classes() {
        assert_eq!(
            app_id_key("com.projectd2r.launcher"),
            r"Software\Classes\AppUserModelId\com.projectd2r.launcher"
        );
    }
}
