# Launcher shell contrast (variant D)

Research for #23. Measured against `origin/prototype/launcher-shell` @ `b6f4e86`, folder `prototypes/launcher-shell-prototype/` (paths below are relative to it). Proposals only: the prototype is unchanged.

## Target

**WCAG 2.2 Level AA**, as proposed in #23:

- Text: 4.5:1. Large text: 3:1 (SC 1.4.3, https://www.w3.org/TR/WCAG22/#contrast-minimum).
- UI components and meaningful graphics: 3:1 against adjacent colours (SC 1.4.11, https://www.w3.org/TR/WCAG22/#non-text-contrast).
- Focus: must be visible (SC 2.4.7, https://www.w3.org/TR/WCAG22/#focus-visible) and must not be hidden (SC 2.4.11, https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum). SC 2.4.13 Focus Appearance (AAA, https://www.w3.org/TR/WCAG22/#focus-appearance) is checked for information only.
- Large text is at least 18pt, or 14pt bold. That is about 24px, or 18.66px bold (https://www.w3.org/TR/WCAG22/#dfn-large-scale; Understanding 1.4.3, https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html).
- Only the hero `.h1` (30–50px, weight 900) and the 44px brand glyph count as large text. Every other string, including the 16px bold Play button, needs 4.5:1.

**Method.** The formulas are the WCAG 2.2 definitions:

- Relative luminance (https://www.w3.org/TR/WCAG22/#dfn-relative-luminance): sRGB ≤ 0.04045 → /12.92, else ((c+0.055)/1.055)^2.4. Then L = 0.2126R + 0.7152G + 0.0722B.
- Contrast ratio (https://www.w3.org/TR/WCAG22/#dfn-contrast-ratio): (L1+0.05)/(L2+0.05).

Alpha (the token alpha, `opacity`, and the gradient stops) is composited with `c = fg·a + bg·(1−a)` before luminance is taken. The script asserts black/white = 21 and #777/#fff = 4.48.

Luminance inputs, from `src/tokens.css:5-21`:

| Token | Value | L |
|---|---|---|
| `--bg` | #101011 | 0.0052 |
| `--rail` | #0b0b0c | 0.0034 |
| `--panel` | #1d1d1e | 0.0123 |
| `--line` | #353233 | 0.0328 |
| `--text` | #f4f1ed | 0.8826 |
| `--muted` | #b1adac | 0.4221 |
| `--hero-copy` | #cbc3c2 | 0.5562 |
| `--detail` | #eb626b | 0.2746 |
| `--accent` | #e3353d | 0.1921 |
| `--button` | #c72d37 | 0.1429 |
| `--button-hover` | #e13742 | 0.1913 |
| `--ribbon` | #b52b35 | 0.1181 |
| `--focus` / white | #ffffff | 1.0000 |

## Token pairs

| Foreground | Background | Resolved | Ratio | Req. | Result |
|---|---|---|---|---|---|
| `--text` | `--bg` / `--rail` / `--panel` | #f4f1ed on #101011 / #0b0b0c / #1d1d1e | 16.89 / 17.47 / 14.96 | 4.5 | pass |
| `--muted` | `--bg` / `--rail` / `--panel` | #b1adac on … | 8.55 / 8.85 / 7.57 | 4.5 | pass |
| `--hero-copy` | `--bg` / `--rail` / `--panel` | #cbc3c2 on … | 10.98 / 11.36 / 9.72 | 4.5 | pass |
| `--detail` | `--bg` / `--rail` / `--panel` | #eb626b on … | 5.88 / 6.08 / 5.21 | 4.5 | pass |
| `--muted` @ opacity .8 (`.credit`, styles.css:33) | `--bg` (flat) | #918e8d on #101011 | 5.82 | 4.5 | pass (fails over art, see below) |
| white, Play (styles.css:26) | `--button` | #fff on #c72d37 | 5.44 | 4.5 | pass |
| **white, Play :hover** (styles.css:27) | **`--button-hover`** | #fff on #e13742 | **4.35** | 4.5 | **FAIL** |
| white, ribbon (styles.css:22) | `--ribbon` | #fff on #b52b35 | 6.25 | 4.5 | pass |
| white, "Coming soon" as coded (styles.css:22-23) | `--line` | #fff on #353233 | 12.69 | 4.5 | pass |
| `--muted`, "Coming soon" as the issue describes it | `--line` | #b1adac on #353233 | 5.70 | 4.5 | pass |
| `--text`, close :hover (styles.css:19) | `--button` | #f4f1ed on #c72d37 | 4.83 | 4.5 | pass |
| `--text`, window control :hover (styles.css:18) | `--panel` | | 14.96 | 4.5 | pass |
| `--accent`, brand glyph 44px / current-item bar (styles.css:44) | `--rail` | #e3353d on #0b0b0c | 4.54 | 3 | pass |
| `--muted`, ghost button border (styles.css:28) | `--bg` | | 8.55 | 3 | pass |
| `--button`, Play fill boundary | `--bg` | #c72d37 on #101011 | 3.49 | 3 | pass |
| `--detail`, ready dot (graphic) | hero at the dot | #eb626b on #101011 | 5.88 | 3 | pass |
| `--line`, rail and window borders | `--bg` / `--rail` | #353233 | 1.50 / 1.55 | n/a | not a UI component or required graphic, so 1.4.11 does not apply |

The issue says the "Coming soon" ribbon is `--muted` on `--line`. The code actually sets `.ribbon { color: #fff }`, and `.soon` changes only the background. Both readings pass.

### Focus ring

The ring is `outline: 3px solid var(--focus)` with `outline-offset: 2px` (styles.css:5).

| Ring on | Adjacent colour | Ratio | Req. (1.4.11) | Result |
|---|---|---|---|---|
| `--bg` / `--rail` (nav items, Play, ghost button) | #101011 / #0b0b0c | 19.02 / 19.67 | 3 | pass |
| Play fill (ring next to the button) | #c72d37 | 5.44 | 3 | pass |
| Hero art, worst pixel 5–7px outside a focusable box, all heroes and both sizes | window controls #3d464f; feed links #421e1f; Play/ghost #181718 | 9.60 / 13.89 / 17.88 | 3 | pass |

- **2.4.7:** pass. Every `button` and `a` gets the ring.
- **2.4.11:** pass. Nothing sticky or overlaid covers a focused control. The seamless title bar (`z-index: 5`, styles.css:133) holds only the window controls.
- **2.4.13 (AAA, information only):** the ring is 3px thick, so it is larger than the 2px perimeter and the change of state is ≥ 9.6:1. One caveat: the window controls sit at `top: 0` inside `.window { overflow: hidden }` (styles.css:9), so the top edge of their ring is clipped. They still meet 2.4.7 and 1.4.11, but 2.4.13 would not count the clipped edge.

## Text over hero art

**How it was measured.** Vite ran on the prototype with `?variant=D`, in the built-in browser, and each hero (`src/data.js`) was measured at both window sizes:

1. `.D main` was rebuilt on a canvas at 1 CSS px per pixel: the `--bg` fill, then the CORS-loaded art at its authored `background-size` and `background-position`, then both `.shade` layers (styles.css:78).
2. For every pixel inside each text node's client rects, the contrast was computed with the text colour alpha-composited over that pixel (`.credit` at opacity .8).

The window transform was set to scale 1 and animations were finished before measuring. Each cell gives the worst pixel / 5th percentile, and the raw art colour at the worst pixel where it matters.

| Element (req.) | PD2R 1280×720 | PD2R 1024×640 | Forever 1280×720 | Forever 1024×640 | 09 1280×720 | 09 1024×640 |
|---|---|---|---|---|---|---|
| **`.credit` 10px (4.5)** | **3.41** / 3.89 (art #7a2223) | **3.02** / 3.80 | **1.50** / 1.80 (art #868588) | **1.35** / 1.71 (art #918e93) | **3.16** / 3.27 | **2.95** / 3.13 |
| `.h1` large (3) | 9.74 / 15.57 | 8.49 / 13.95 | 11.18 / 16.02 | 10.84 / 15.45 | 16.76 | 16.75 |
| blurb `p` 14px, `--hero-copy` (4.5) | 10.32 | 9.65 | 10.90 | 8.84 | 10.90 | 10.44 |
| ready line 12px, `--muted` (4.5) | 8.06 | 8.49 | n/a | n/a | n/a | n/a |
| "About" ghost button text (4.5) | n/a | n/a | 16.89 | 16.76 | 16.89 | 16.76 |
| feed "News" label 10px (4.5) | 6.87 | 6.90 | 8.06 | 7.74 | 8.35 | 8.33 |
| feed meta `.m` 10px (4.5) | 7.12 | 6.35 | 8.33 | 8.21 | 8.20 | 8.04 |
| feed title `.t` 13px bold (4.5) | 14.24 | 13.54 | 16.49 | 16.49 | n/a (no news) | n/a |
| window-control glyphs, `--muted` (4.5) | 7.90 | 7.56 | 4.68 | 4.59 | 4.75 | **4.54** (thin margin) |
| ribbon text | has its own fill, see the token table | | | | | |

The credit sits at `top: 42px` (styles.css:138). Neither `.shade` layer reaches that spot: the vertical layer is transparent above 62% height, and the horizontal one is transparent beyond 50% width. So the credit is drawn straight on the raw art.

## 10px label size

WCAG sets no minimum font size. Size matters for contrast in only one way: text below 18pt, or 14pt bold, is not large, so the 10px labels (`--label`, tokens.css:23) must reach 4.5:1. They do on flat surfaces and in the feed, but the credit fails over art.

Two related criteria:

- SC 1.4.4 Resize Text (https://www.w3.org/TR/WCAG22/#resize-text) requires 200% scaling without loss.
- SC 1.4.12 Text Spacing (https://www.w3.org/TR/WCAG22/#text-spacing) requires no loss with letter spacing 0.12em, word spacing 0.16em, line height 1.5 and paragraph spacing 2em.

Both mean the 10px uppercase labels must not sit in fixed-size boxes that clip. That was not tested here, because the window is a fixed 1280×720 or 1024×640 frame and Electron zoom is a separate check.

10px is thin at weight 400. `.credit` and `.feed .m` are not bold, so they gain the most from a margin above 4.5.

## Failures and smallest fixes

1. **Hero credit line over art.** Every hero at both sizes fails: worst 1.35:1, needed 4.5:1. The fixes all change `.D .credit`, or `.D .shade` for option C:
   - **A (smallest, keeps `--muted`):** remove `opacity: .8` and give the credit a box scrim: `background: #1010119e; padding: 2px 6px`. Scrim α 0.62 is just above the 0.61 the worst pixel needs, so the worst case is ≥ 4.5:1 for all six cases (Forever 1024×640 is the binding case).
   - **B:** `color: var(--text); opacity: 1` with a lighter scrim `background: #10101142` (α 0.26). The worst case is ≥ 4.5:1. Without any scrim, `--text` alone reaches only 2.87:1 on Forever.
   - **C (gradient instead of a box):** `color: var(--text); opacity: 1`, and add a top layer to `.D .shade`: `linear-gradient(180deg,#101011cc 0,#10101100 80px)`. Worst case 4.92:1 (Forever 1024×640). This also raises the window-control margin.
2. **Play hover, white on `--button-hover`.** 4.35:1, needed 4.5:1, because 16px bold is not large text. Fix: `--button-hover: #e0303b` (same hue, slightly darker) gives **4.51:1**. It is still lighter than `--button` (1.21:1 between the two), so the hover change stays visible. The `translateY(-2px)` lift also signals hover.

Everything else passes. The tightest margins are the window-control glyphs over 09 and Forever (4.54–4.75) and `--accent` on `--rail` (4.54, which only needs 3:1).
