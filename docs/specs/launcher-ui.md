# Launcher UI design spec

Status: **awaiting owner approval** in [#20](https://github.com/synpoox/resurrected-launcher/issues/20). Implementing it in the Tauri app is the next effort. Map: [Chart the launcher UI look and feel](https://github.com/synpoox/resurrected-launcher/issues/12). Behaviour comes from the [launcher v1 spec](launcher-v1.md). This document covers only how that behaviour looks and feels, and adds no features.

## Approved prototype

Tag [`launcher-ui-prototype-v1`](https://github.com/synpoox/resurrected-launcher/tree/launcher-ui-prototype-v1/prototypes/launcher-shell-prototype) (commit `7c5a629` on the throwaway `prototype/launcher-shell` branch). It is never merged. To run it, use `npx vite prototypes/launcher-shell-prototype`. `&size=1024` gives the minimum window.

| Page | What it shows |
| --- | --- |
| `/?variant=E` | Shell, Overview and Patch Notes |
| `/journey.html?state=<key>&tab=overview\|notes` | Every journey state (← → cycles) |
| `/settings.html?variant=B` | Settings and troubleshooting |
| `/tray.html?variant=A&tray=<state>` | Tray icon and menu (tag [`launcher-tray-prototype-v1`](https://github.com/synpoox/resurrected-launcher/tree/launcher-tray-prototype-v1/prototypes/launcher-shell-prototype), branch `prototype/launcher-tray`) |

Other variants (A–D, F; settings A, C, D) are kept for the record only. Copy, versions and paths in the prototype are illustrative.

## Tokens

Launcher-owned CSS custom properties. There is no shared web/launcher package ([web ADR 0002](https://github.com/synpoox/resurrected-web/blob/main/docs/adr/0002-shared-design-and-switchable-themes.md)).

Copied from the resurrected-web `launcher` preset at `155eceed55ba424477f4316a8ffddb0844d5445d` (`site/themes.css`). The values are recorded here because that web PR is unmerged and web history was squashed.

| Token | Value | Note |
| --- | --- | --- |
| `--bg` | `#101011` | |
| `--panel` | `#1d1d1e` | |
| `--text` | `#f4f1ed` | |
| `--muted` | `#b1adac` | |
| `--accent` | `#e3353d` | the one accent, also used for errors and warnings |
| `--line` | `#353233` | |
| `--button` | `#c72d37` | |
| `--button-hover` | **`#e0303b`** | deviates from web `#e13742`, which fails AA with white (4.35:1); #20 |
| `--detail` | `#eb626b` | |

Launcher additions, promoted from prototype literals in [#17](https://github.com/synpoox/resurrected-launcher/issues/17):

| Token | Value | Use |
| --- | --- | --- |
| `--rail` | `#0b0b0c` | sidebar |
| `--ribbon` | `#b52b35` | slanted tag |
| `--focus` | `#fff` | focus ring |
| `--rule` | `#ffffff24` | hairlines over art |
| `--hero-copy` | `#cbc3c2` | body copy over art and in panels |

Type: system Arial (`Arial, Helvetica, sans-serif`), with no bundled font. Headings are weight 900. Labels are uppercase, 10px minimum, letter-spaced. Monospace (`Consolas`) is for numbers and paths only. Corners are square and there are no shadows inside the window.

## Window

- Frameless with a **seamless title bar**: content runs to the top edge, and the minimise, maximise and close controls float top-right over it. There is no title-bar mark or name.
- 1280×720 by default, 1024×640 minimum. Every layout below is checked at both sizes.
- Closing the window keeps the launcher in the tray, and Quit lives in the tray menu (see [Tray](#tray)).

## Layout

**Sidebar** (220px, `--rail`):
- Umbrella branding at the top: `/` glyph in accent at 44px, plus **Project Resurrected** in uppercase, 16px, weight 900. The label is provisional and lives in one swappable place.
- A text-only product list in this order: Project Diablo 2: Resurrected, Diablo II: Resurrected Forever, Diablo II: Resurrected 09. The active product is white with a 2px accent inset, and the others are muted.
- Settings sits at the bottom with a cog icon, 34px from the bottom edge.
- There is no Home entry; the launcher opens on a product. D2R Tools does not appear.

**Sidebar badges**: a small square after the product name shows progress %, `↑` (update), `●` (running) or `!` (error, on `--button`).

**Product page, Overview tab** (layout E, approved in #20):
- Full-bleed hero art behind the whole main area, under a top, left and bottom gradient.
- **Overview | Patch Notes** tabs float top-centre in a square frame (`#101011b3`, 1px `--line`). The selected tab is on `--panel` with a 2px accent underline.
- Hero copy sits left at about 44% height: ribbon tag, a **title that is always exactly two lines** (scaling between 30px and 50px instead of wrapping), and one line of description.
- The bottom-left holds the action row. Play is solid red, at least 260px × 60px, with a plain uppercase label. Next to it is the square `⋯` **Manage** menu (Repair, Open saves folder, Open settings folder, divider, Uninstall… in `--detail`) and one readiness line ("● Ready · alpha 0.4.2 (latest)"). When a secondary button shares the row, the primary narrows to 200px.
- **Notices** sit directly above the action row: a panel with a 2px left rule, muted for information and accent for errors.
- The bottom-right holds a 250px text-only news feed: tag · relative date, headline, and an ↗ link out. It has no images and no in-launcher article view. **The feed shows announcements and guides only; release notes live in Patch Notes.** An empty feed reads "Nothing yet."

**Product page, Patch Notes tab** (approved in #20):
- Fills the whole main area except the sidebar, with no hero art. It scrolls with a dark scrollbar, and content passes under a solid top fade.
- The state's main action moves top-left at 48px height. There is no Manage button. Secondary buttons, readiness and detail lines are hidden, and progress shrinks to a 250px bar with Cancel.
- If the state has a notice, a red dot on the Overview tab points to it. Notices are shown on Overview only.
- Releases are listed newest first in a centred 680px column. Each has an uppercase title ("PD2R ALPHA 0.4.2"), a one-line summary, a "Patch notes · date" line and a double rule, followed by sections (heading plus bullets).
- The installed release's notes ship with it and display offline. Newer releases offline show "Patch notes for newer releases need a connection. Retry".

**Rules for both tabs:**
- **Products without a release** (Forever and 09 today) show no tabs. They get only the Coming soon ribbon, the description and an outlined "About <product>" button, with no install, date, signup or account.
- **Default tab**: Overview. The first launch after an update installs opens Patch Notes once, on the new release.

**Settings** (layout B, approved in [#19](https://github.com/synpoox/resurrected-launcher/issues/19)):
- Opens in the main area. A 220px sub-nav holds the tabs **Game · Launcher · Folders and plugins · Troubleshooting**. One panel shows at a time, and each row has a label, a hint and a right-hand control.
- **Game**:
  - The D2R location with its store and build, and "Change…", which opens the location dialog. Choosing a location moves nothing.
  - Browsing to a folder without `D2R.exe` shows an inline error, and a notice reports the recheck result.
  - While the game runs, the control is disabled with "Quit the game to change this".
- **Launcher**: **Start with Windows** switch, off by default, which opens to the tray.
- **Launcher → About and credits** (new, #20): lists each hero image with its name, source and "Promotional art, not mod gameplay".
- **Folders and plugins**:
  - A PD2R group: saves, settings and filters, and **PD2R plugins** ("Loads only with PD2R", with the `core-*` note).
  - An All products group: **Global plugins** ("Loads for every product").
  - Opening a missing plugins folder for the first time creates it. A failure shows a red toast and never asks for administrator rights.
  - There are no buttons for products that aren't installed. A note says the launcher doesn't list, enable or disable plugins.
- **Troubleshooting**:
  - "Export diagnostics…" opens a review dialog: the full report text, a never-included list, the Windows user name replaced by `<you>`, and an optional launcher-log checkbox.
  - "Save as file…" shows a toast with "Show in folder". Nothing is sent.
  - A pointer says Repair is in the product's `⋯` menu.

**Feedback surfaces**:
- **Notice**: explains a state in words, as above.
- **Dialog**: square, on `--panel`, with a 2px top rule (accent for destructive or risky actions) over a `#000000b3` backdrop.
- **Toast**: bottom-right. Information is muted and auto-dismisses; errors are accent and stay until dismissed.

## Tray

Variant A (plain menu), approved in [#27](https://github.com/synpoox/resurrected-launcher/issues/27). Variants B (status menu with notifications) and C (launcher-styled flyout) are kept on the prototype branch for the record only.

- **Icon**: the mark at 16px with its strokes thickened so they survive at that size, plus one overlay per state:
  - Idle: no overlay.
  - Update available: `↑` on an accent square, bottom-right.
  - Game running: a white dot, bottom-right.
  - Progress: a bar along the bottom edge in accent over `--line`, filled to the %.
  - Error: `!` on a `--button` square, bottom-right.
  The icon sits on its own dark square, so it reads on both dark and light taskbars. Windows 11 puts new icons in the ⌃ overflow by default, and the launcher does not try to change that.
- **Tooltip**: "Project Resurrected", then "PD2R: <status>" (for example "Ready · alpha 0.4.2 (latest)" or "Downloading alpha 0.4.2 · 52%").
- **Left-click**: opens the window, restoring and focusing it if it is hidden or minimised. It never hides the window.
- **Right-click**: a native Windows menu that follows the system theme:
  1. **Open Project Resurrected** (bold, the default item)
  2. separator
  3. The PD2R action for the state: Play PD2R · Update PD2R and Play 0.4.1 · a disabled "PD2R is running" · a disabled "Downloading PD2R · 52%" and Cancel download · Try again and Show what happened (opens the window).
  4. separator
  5. **Quit**
- Products without a release (Forever and 09 today) don't appear in the menu.
- Tray actions don't open the window, except Show what happened, and Update when the release is save-breaking: that opens the window on the save-breaking dialog (ADR 0001). Save-breaking was not simulated in the prototype.
- **Closing the window** hides it to the tray. The first close ever shows one Windows notification: "Still running in the tray. Downloads and update checks carry on here. To exit, right-click the icon and choose Quit." There are no other tray notifications in v1.
- **Start with Windows** starts in the tray only: no window and no notification.
- **Quit**:
  - Idle, update available or error: exits at once.
  - Game running: the launcher exits and the game keeps running.
  - Downloading: the window opens on a dialog, "Quit while PD2R is downloading?", with **Cancel download and quit** and **Keep downloading**. The installed release stays playable.
  - Final application (`applying`, `cancelling`): Quit is not designed; the default is the same dialog, and quitting waits for the safe stop.

## Journey states

Every state keeps the hero, ribbon, title and description. The state appears in the action row, a notice or a dialog ([#18](https://github.com/synpoox/resurrected-launcher/issues/18) has the full treatment of each).

| State | Key | Treatment |
| --- | --- | --- |
| D2R not found | `d2r-missing` | "Locate D2R", a red dot with "wasn't found on this PC", and a notice to choose the folder with `D2R.exe`. Nothing is moved or copied. |
| Choose D2R location | `d2r-choose` | Dialog with a radio list of detected installations (path, store, build, support, whether PD2R is there) and "Browse for another folder…". The choice is remembered and can be changed in Settings. |
| Loader not ready | `loader-missing` | "Set up D2RLoader" and "Manual steps". The notice names the official installer and version, its GitHub source and consent, and that it is **not code-signed**. |
| Ready to install | `install` | "Install", plus the release, download size and free space needed. |
| Install progress | `downloading` | A progress bar replaces Play: label, %, size, speed and time left, with "Cancel". The sidebar badge shows the %. |
| Final application | `applying` | "Cancel safely" plus "Cancelling now waits until it's safe to stop, or rolls back". |
| Safe stop | `cancelling` | A striped bar, "Stopping safely…", "Restoring the previous files. Don't close the launcher.", and Cancel disabled. |
| Ready | `ready` | Play, `⋯`, and the readiness line. |
| Update vs play installed | `update` | "Update" (primary) next to "Play 0.4.1" (outlined), plus an `↑` badge. |
| Save-breaking | `save-breaking` | Accent-topped dialog. Saves stay in place and are not converted. The new version may refuse or change them. Rollback restores game files, not saves. A backup tip with Open folder. A required **acknowledgement checkbox** enables "Update to 0.5.0"; the alternative is "Not now" ([ADR 0001](../adr/0001-warn-on-save-breaking-updates-without-forced-isolation.md)). |
| Blocked prerequisites | `blocked` | Play disabled, "Check again", and an error notice with the reason, recovery steps and "How to fix". `!` badge. |
| Running | `running` | Disabled panel-coloured "Playing" with a pulsing dot, "Started 12 min ago · Repair and updates wait until you quit", and a `●` badge. |
| Startup failure | `startup-failed` | "Try again", "Repair" and "Export diagnostics", with an error notice giving the exit code and next steps. |
| Repair | `repair` | Progress with files checked and to restore, and "saves and edited filters are kept". |
| Manage menu | `manage` | `⋯` open: Repair, Open saves folder, Open settings folder, then Uninstall…. |
| Uninstall | `uninstall` | Accent-topped dialog: removes PD2R's files only. **Kept:** saves and settings with path and Open. **Not touched:** D2R, D2RLoader and other products. |
| Offline | `offline` | Play still works: "Offline · alpha 0.4.1 verified · can't check for updates". The news feed reads "News needs a connection. Retry". |
| Coming soon | `coming-soon` | Ribbon, description, "About Forever"; no tabs. |

## Motion

Subtle only, with no video and no hero rotation: the hero changes only when the player picks a product.
- Hero art fades in over 450ms ease-out. Hero copy lifts 8px and fades over 450ms.
- Button background and transform transition over 150ms. Primary hover lifts 2px.
- Progress width transitions over 300ms. The running dot pulses on a 1.6s cycle.
- Switching tabs and scrolling Patch Notes is instant, with no smooth scrolling.
- **Under `prefers-reduced-motion: reduce`, every animation and transition is off.** The running pulse becomes a static dot, and progress bars jump to their value.

## Accessibility

Target **WCAG 2.2 AA**: 4.5:1 for text, 3:1 for large text (24px, or 18.66px bold) and UI components. Measurements: [launcher shell contrast](../research/launcher-shell-contrast.md).
- Every token pair on `--bg`/`--rail`/`--panel` passes (5.21–17.47:1), as do white on `--button` (5.44) and `--ribbon` (6.25), and text over the hero art (worst 6.35). The tightest pass is the window-control glyphs over art (4.54).
- 10px labels are never large text and need 4.5:1.
- Hero art has no visible credit line. Each hero has an accessible name (e.g. "Reign of the Warlock key art (official D2R promotional art)"), and credits live in Settings.

**Keyboard and focus** (drafted in #20; check them in implementation):
- Tab order: window controls → sidebar → tabs → main action and its row (or the Patch Notes action) → main content → news feed.
- Arrow keys move within the sidebar list, the tab list and open menus. Enter and Space activate.
- Esc closes the open menu or dialog.
- Dialogs trap focus and return it to the control that opened them. Menus return focus to `⋯`.
- Every focusable element shows the 3px `--focus` ring at a 2px offset. The window controls use an **inset** ring, because an outset ring is clipped at the window's top edge.
- Tabs use `tablist`/`tab`/`tabpanel` roles. The Overview attention dot has an accessible label ("needs attention").
- Disabled controls keep their visible label and explain why next to them (e.g. "Quit the game to change this").

## Artwork

Official D2R promotional art, © Blizzard Entertainment, **never presented as mod gameplay**. It is not copied into this repo.

| Product | Image | Framing |
| --- | --- | --- |
| PD2R | Reign of the Warlock key art | `55% center`, cover |
| Forever | 30th Anniversary key art, textless, 1920×1080 ([research](https://github.com/synpoox/resurrected-launcher/blob/launcher-ui-prototype-v1/docs/research/d2r-forever-hero-image.md), [#22](https://github.com/synpoox/resurrected-launcher/issues/22)) | `20% 30%`, cover |
| 09 | Sorceress class art (diablo2.blizzard.com), 2600×906 | `74% 22%`, `auto 135%` |

Source URLs are in the prototype's `src/data.js`. The real app **bundles or caches** the art instead of hotlinking it, so heroes still show offline.

**Release gate:** no licence covers using this art in a third-party launcher. Before a public release, either get permission or replace the art.

## Not specified

Each item has a default to build to unless a later decision replaces it.

| Gap | Default |
| --- | --- |
| Launcher self-update notice | Follow-up prototype ticket. Needed before v1. |
| First-run onboarding | No separate onboarding; use the inline states `d2r-missing`, `d2r-choose` and `loader-missing`. |
| Splash or loading screen | None. The window opens on the last product, or PD2R. |
| News feed size | Up to three items, newest first. Offline and empty lines as above. |
| "Reset player settings" | Not designed. It must be separate from Repair ([#5](https://github.com/synpoox/resurrected-launcher/issues/5) Q9). |
| Clean-install recovery for unrecognised files | Not designed. Use the `blocked` notice treatment. |
| Launcher self-update settings | Not designed. They belong with the self-update ticket. |
| Patch Notes contrast and keyboard checks | Not measured. Check them against the targets above during implementation. |
