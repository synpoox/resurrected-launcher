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

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A connected monitor, in physical pixels: its full rectangle and its work area
/// (the rectangle minus the taskbar), and its scale factor (1.0 = 100%).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    pub full: Rect,
    pub work: Rect,
    pub scale: f64,
}

impl Rect {
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
/// enough that the whole window (`size`, physical pixels) fits in that monitor's
/// work area. `size` was measured at scale factor `current_scale`; Windows
/// rescales the window when it moves to a monitor with another scale factor, so
/// it is converted to the target monitor's before checking. A window larger than
/// that work area gets no spot (it opens centred on the primary monitor, where
/// its size was chosen to fit).
pub fn restorable(
    saved: Option<(i32, i32)>,
    size: (u32, u32),
    current_scale: f64,
    monitors: &[Monitor],
) -> Option<(i32, i32)> {
    let (x, y) = saved?;
    let target = monitors.iter().find(|m| m.full.contains(x, y))?;
    let m = target.work;
    let ratio = if current_scale > 0.0 && target.scale > 0.0 {
        target.scale / current_scale
    } else {
        1.0
    };
    // Rounded up with a little slack for float error: never under-estimate.
    let rescale = |n: u32| (f64::from(n) * ratio - 1e-6).ceil().max(0.0) as u32;
    let size = (rescale(size.0), rescale(size.1));
    if size.0 > m.width || size.1 > m.height {
        return None;
    }
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
        .map(|m| {
            let work = m.work_area();
            Monitor {
                full: Rect {
                    x: m.position().x,
                    y: m.position().y,
                    width: m.size().width,
                    height: m.size().height,
                },
                work: Rect {
                    x: work.position.x,
                    y: work.position.y,
                    width: work.size.width,
                    height: work.size.height,
                },
                scale: m.scale_factor(),
            }
        })
        .collect();
    let size = window.outer_size().map_or((0, 0), |s| (s.width, s.height));
    let scale = window.scale_factor().unwrap_or(1.0);
    if let Some((x, y)) = restorable(read(&dir), size, scale, &monitors) {
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

    const fn mon(x: i32, y: i32, width: u32, height: u32, taskbar: u32) -> Monitor {
        Monitor {
            full: Rect {
                x,
                y,
                width,
                height,
            },
            work: Rect {
                x,
                y,
                width,
                height: height - taskbar,
            },
            scale: 1.0,
        }
    }

    // 1920x1080 with a 48px taskbar: work area 1920x1032.
    const PRIMARY: Monitor = mon(0, 0, 1920, 1080, 48);
    const WIN: (u32, u32) = (1600, 900);
    const LEFT: Monitor = mon(-2560, -200, 2560, 1440, 0);

    #[test]
    fn a_spot_on_a_connected_monitor_is_restored() {
        assert_eq!(
            restorable(Some((100, 50)), WIN, 1.0, &[PRIMARY]),
            Some((100, 50))
        );
        assert_eq!(
            restorable(Some((-2000, -100)), WIN, 1.0, &[PRIMARY, LEFT]),
            Some((-2000, -100))
        );
    }

    #[test]
    fn a_spot_on_a_disconnected_monitor_is_centred() {
        assert_eq!(restorable(Some((-2000, -100)), WIN, 1.0, &[PRIMARY]), None);
        assert_eq!(restorable(Some((1920, 0)), WIN, 1.0, &[PRIMARY]), None);
        assert_eq!(restorable(None, WIN, 1.0, &[PRIMARY]), None);
        assert_eq!(restorable(Some((10, 10)), WIN, 1.0, &[]), None);
    }

    #[test]
    fn a_window_hanging_off_its_monitor_is_pulled_back_into_the_work_area() {
        // Hanging off the bottom/right: clamped to the work area, above the taskbar.
        assert_eq!(
            restorable(Some((1000, 700)), WIN, 1.0, &[PRIMARY]),
            Some((320, 132))
        );
        // Already fully inside the work area: untouched.
        assert_eq!(
            restorable(Some((320, 132)), WIN, 1.0, &[PRIMARY]),
            Some((320, 132))
        );
    }

    #[test]
    fn a_window_larger_than_its_monitor_is_centred_instead() {
        // 1600x900 on a 1366x768 secondary to the right of PRIMARY.
        let small = mon(1920, 0, 1366, 768, 0);
        assert_eq!(
            restorable(Some((2000, 100)), WIN, 1.0, &[PRIMARY, small]),
            None
        );
        // Fits the full monitor but not the work area (taskbar): also centred.
        let tight = mon(0, 0, 1920, 1080, 200);
        assert_eq!(restorable(Some((10, 10)), WIN, 1.0, &[tight]), None);
    }

    const fn scaled(m: Monitor, scale: f64) -> Monitor {
        Monitor { scale, ..m }
    }

    #[test]
    fn the_size_is_rescaled_to_the_target_monitors_scale_factor() {
        // 1280x720 at 100% on the primary is 1920x1080 physical at 150%.
        let win = (1280, 720);
        // Secondary at 150% whose work area (1600x900) cannot fit 1920x1080.
        let small = scaled(mon(1920, 0, 1600, 900, 0), 1.5);
        assert_eq!(
            restorable(Some((2000, 10)), win, 1.0, &[PRIMARY, small]),
            None
        );
        // The same monitor at 100% fits the unscaled 1280x720.
        let same = mon(1920, 0, 1600, 900, 0);
        assert_eq!(
            restorable(Some((2000, 10)), win, 1.0, &[PRIMARY, same]),
            Some((2000, 10))
        );
        // A 4K secondary at 150% fits 1920x1080 and the spot is clamped to it.
        let big = scaled(mon(1920, 0, 3840, 2160, 0), 1.5);
        assert_eq!(
            restorable(Some((5000, 1500)), win, 1.0, &[PRIMARY, big]),
            Some((3840, 1080))
        );
        // Moving to a lower scale shrinks the check: 1920x1080 at 150% is
        // 1280x720 at 100%, which fits a 1366x768 work area.
        let low = mon(1920, 0, 1366, 768, 0);
        assert_eq!(
            restorable(Some((2000, 10)), (1920, 1080), 1.5, &[PRIMARY, low]),
            Some((2000, 10))
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
