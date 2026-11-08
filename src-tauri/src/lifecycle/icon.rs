//! The 16px tray icon, drawn at runtime so the progress bar can show any %: the
//! `/` mark with thickened strokes on its own dark square (readable on dark and
//! light taskbars), plus one overlay per state.

use tauri::image::Image;

const SIZE: usize = 16;
type Rgba = [u8; 4];
const BG: Rgba = [0x10, 0x10, 0x11, 0xff]; // --bg
const ACCENT: Rgba = [0xe3, 0x35, 0x3d, 0xff]; // --accent
const LINE: Rgba = [0x35, 0x32, 0x33, 0xff]; // --line
const BUTTON: Rgba = [0xc7, 0x2d, 0x37, 0xff]; // --button
const WHITE: Rgba = [0xff, 0xff, 0xff, 0xff];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    Idle,
    /// `↑` on an accent square, bottom-right.
    Update,
    /// A white dot, bottom-right.
    Running,
    /// A bar along the bottom edge in accent over `--line`, filled to the %.
    Progress(u8),
    /// `!` on a `--button` square, bottom-right.
    Error,
}

/// RGBA pixels, row by row.
pub fn pixels(overlay: Overlay) -> Vec<u8> {
    let mut px = [[BG; SIZE]; SIZE];
    // The mark: a slash three pixels thick from bottom-left to top-right.
    for (y, row) in px.iter_mut().enumerate() {
        for (x, p) in row.iter_mut().enumerate() {
            if (3..=12).contains(&x) && (x + y).abs_diff(15) <= 1 {
                *p = ACCENT;
            }
        }
    }
    let mut badge = |fill: Rgba, glyph: &[(usize, usize)]| {
        for row in &mut px[9..] {
            row[9..].fill(fill);
        }
        for &(x, y) in glyph {
            px[y][x] = WHITE;
        }
    };
    match overlay {
        Overlay::Idle => {}
        Overlay::Update => badge(
            ACCENT,
            &[(12, 10), (12, 11), (12, 12), (12, 13), (12, 14), (11, 11), (13, 11), (10, 12), (14, 12)],
        ),
        Overlay::Error => badge(BUTTON, &[(12, 10), (12, 11), (12, 12), (12, 14)]),
        Overlay::Running => {
            // A dark ring keeps the dot apart from the mark.
            for (y, row) in px.iter_mut().enumerate().skip(8) {
                for (x, p) in row.iter_mut().enumerate().skip(8) {
                    let d = (x as i32 - 12).pow(2) + (y as i32 - 12).pow(2);
                    if d <= 13 {
                        *p = if d <= 5 { WHITE } else { BG };
                    }
                }
            }
        }
        Overlay::Progress(pct) => {
            let filled = usize::from(pct.min(100)) * SIZE / 100;
            for row in &mut px[13..] {
                for (x, p) in row.iter_mut().enumerate() {
                    *p = if x < filled { ACCENT } else { LINE };
                }
            }
        }
    }
    px.iter().flatten().flatten().copied().collect()
}

pub fn draw(overlay: Overlay) -> Image<'static> {
    Image::new_owned(pixels(overlay), SIZE as u32, SIZE as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(px: &[u8], x: usize, y: usize) -> Rgba {
        let i = (y * SIZE + x) * 4;
        px[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn idle_is_the_mark_on_a_dark_square() {
        let px = pixels(Overlay::Idle);
        assert_eq!(px.len(), SIZE * SIZE * 4);
        assert_eq!(at(&px, 0, 0), BG);
        assert_eq!(at(&px, 7, 8), ACCENT);
        assert_eq!(at(&px, 13, 13), BG);
    }

    #[test]
    fn progress_fills_the_bottom_bar_to_the_percent() {
        let px = pixels(Overlay::Progress(50));
        assert_eq!(at(&px, 7, 15), ACCENT);
        assert_eq!(at(&px, 8, 15), LINE);
        assert_eq!(at(&px, 15, 13), LINE);
        assert_eq!(at(&pixels(Overlay::Progress(100)), 15, 15), ACCENT);
    }

    #[test]
    fn each_state_has_its_own_overlay() {
        assert_eq!(at(&pixels(Overlay::Error), 15, 15), BUTTON);
        assert_eq!(at(&pixels(Overlay::Update), 15, 15), ACCENT);
        assert_eq!(at(&pixels(Overlay::Running), 12, 12), WHITE);
        let all = [
            Overlay::Idle,
            Overlay::Update,
            Overlay::Running,
            Overlay::Progress(0),
            Overlay::Error,
        ]
        .map(pixels);
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
