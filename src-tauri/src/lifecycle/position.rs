//! Window position (#188): the main window reopens where it was last closed, but
//! only while that spot is still on a connected monitor; otherwise it opens centred
//! (#174). Only the position is kept: the size stays fixed. A spot near a monitor's
//! edge is pulled back so the whole window stays on that monitor (#210).
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

/// The saved spot, if its top-left corner is on one of `monitors`, moved just far
/// enough that the whole window (`size`, physical pixels) fits on that monitor. A
/// window larger than the monitor is pinned to its top-left corner.
pub fn restorable(
    saved: Option<(i32, i32)>,
    size: (u32, u32),
    monitors: &[Monitor],
) -> Option<(i32, i32)> {
    let (x, y) = saved?;
    let m = monitors.iter().find(|m| m.contains(x, y))?;
    let fit = |pos: i32, start: i32, len: u32, extent: u32| {
        let max = i64::from(start) + i64::from(len) - i64::from(extent);
        let clamped = i64::from(pos).min(max).max(i64::from(start));
        i32::try_from(clamped).unwrap_or(pos)
    };
    Some((fit(x, m.x, m.width, size.0), fit(y, m.y, m.height, size.1)))
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
    let size = window.outer_size().map_or((0, 0), |s| (s.width, s.height));
    if let Some((x, y)) = restorable(read(&dir), size, &monitors) {
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
    const WIN: (u32, u32) = (1600, 900);
    const LEFT: Monitor = Monitor {
        x: -2560,
        y: -200,
        width: 2560,
        height: 1440,
    };

    #[test]
    fn a_spot_on_a_connected_monitor_is_restored() {
        assert_eq!(
            restorable(Some((100, 50)), WIN, &[PRIMARY]),
            Some((100, 50))
        );
        assert_eq!(
            restorable(Some((-2000, -100)), WIN, &[PRIMARY, LEFT]),
            Some((-2000, -100))
        );
    }

    #[test]
    fn a_spot_on_a_disconnected_monitor_is_centred() {
        assert_eq!(restorable(Some((-2000, -100)), WIN, &[PRIMARY]), None);
        assert_eq!(restorable(Some((1920, 0)), WIN, &[PRIMARY]), None);
        assert_eq!(restorable(None, WIN, &[PRIMARY]), None);
        assert_eq!(restorable(Some((10, 10)), WIN, &[]), None);
    }

    #[test]
    fn a_window_hanging_off_its_monitor_is_pulled_back() {
        // 1600x900 restored onto a 1366x768 secondary to the right of PRIMARY.
        let small = Monitor {
            x: 1920,
            y: 0,
            width: 1366,
            height: 768,
        };
        assert_eq!(
            restorable(Some((2000, 100)), WIN, &[PRIMARY, small]),
            Some((1920, 0))
        );
        // Hanging off the bottom/right of a monitor the window does fit on.
        assert_eq!(
            restorable(Some((1000, 700)), WIN, &[PRIMARY]),
            Some((320, 180))
        );
        // Already fully on screen: untouched.
        assert_eq!(
            restorable(Some((320, 180)), WIN, &[PRIMARY]),
            Some((320, 180))
        );
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
