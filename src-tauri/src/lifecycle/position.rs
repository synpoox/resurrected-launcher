//! Window position (#188): the main window reopens where it was last closed, but
//! only while that spot is still on a connected monitor; otherwise it opens centred
//! (#174). Only the position is kept: the size stays fixed.
//!
//! Saved in physical pixels to `window-position` (`"x y"`) in the notice's
//! directory ([`notice::dir`]) when the window closes to the tray and when the
//! launcher quits. A minimised window is not saved: Windows parks it off screen.

use std::path::Path;

use tauri::{AppHandle, Manager, PhysicalPosition, Runtime, WebviewWindow};

use super::notice;

/// The file in app-local data holding the last position.
pub const FILE: &str = "window-position";

/// A connected monitor's rectangle, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Monitor {
    fn contains(&self, x: i32, y: i32) -> bool {
        let (x, y) = (i64::from(x), i64::from(y));
        let (left, top) = (i64::from(self.x), i64::from(self.y));
        x >= left
            && y >= top
            && x < left + i64::from(self.width)
            && y < top + i64::from(self.height)
    }
}

/// The saved spot, if its top-left corner is on one of `monitors`.
pub fn restorable(saved: Option<(i32, i32)>, monitors: &[Monitor]) -> Option<(i32, i32)> {
    saved.filter(|&(x, y)| monitors.iter().any(|m| m.contains(x, y)))
}

/// Reads the saved spot; a missing or malformed file is no spot.
pub fn read(dir: &Path) -> Option<(i32, i32)> {
    let text = std::fs::read_to_string(dir.join(FILE)).ok()?;
    let mut parts = text.split_whitespace();
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((x, y))
}

/// Writes the spot.
pub fn write(dir: &Path, (x, y): (i32, i32)) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(FILE), format!("{x} {y}"))
}

/// Moves the (still hidden) window to the saved spot when it is on a monitor;
/// otherwise leaves it where it was built (centred).
pub fn restore<R: Runtime>(window: &WebviewWindow<R>) {
    let Some(dir) = notice::dir(window.app_handle()) else {
        return;
    };
    let monitors: Vec<Monitor> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Monitor {
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
        })
        .collect();
    if let Some((x, y)) = restorable(read(&dir), &monitors) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// Saves the main window's spot, unless it is minimised.
pub fn save<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_minimized().unwrap_or(true) {
        return;
    }
    let (Ok(position), Some(dir)) = (window.outer_position(), notice::dir(app)) else {
        return;
    };
    if let Err(e) = write(&dir, (position.x, position.y)) {
        crate::state::log::append(&dir, &format!("window position not saved: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: Monitor = Monitor {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };
    const LEFT: Monitor = Monitor {
        x: -2560,
        y: -200,
        width: 2560,
        height: 1440,
    };

    #[test]
    fn a_spot_on_a_connected_monitor_is_restored() {
        assert_eq!(restorable(Some((100, 50)), &[PRIMARY]), Some((100, 50)));
        assert_eq!(
            restorable(Some((-2000, -100)), &[PRIMARY, LEFT]),
            Some((-2000, -100))
        );
    }

    #[test]
    fn a_spot_on_a_disconnected_monitor_is_centred() {
        assert_eq!(restorable(Some((-2000, -100)), &[PRIMARY]), None);
        assert_eq!(restorable(Some((1920, 0)), &[PRIMARY]), None);
        assert_eq!(restorable(None, &[PRIMARY]), None);
        assert_eq!(restorable(Some((10, 10)), &[]), None);
    }

    #[test]
    fn the_spot_round_trips_and_bad_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read(dir.path()), None);
        write(dir.path(), (-2000, 37)).unwrap();
        assert_eq!(read(dir.path()), Some((-2000, 37)));
        for bad in ["", "1", "a b", "1 2 3"] {
            std::fs::write(dir.path().join(FILE), bad).unwrap();
            assert_eq!(read(dir.path()), None, "{bad:?}");
        }
    }
}
