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
| `/selfupdate.html?variant=C&phase=<phase>&product=<state>` | Launcher self-update notice (tag [`launcher-self-update-prototype-v1`](https://github.com/synpoox/resurrected-launcher/tree/launcher-self-update-prototype-v1/prototypes/launcher-shell-prototype), branch `prototype/launcher-self-update`) |

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

- Frameless with a **seamless title bar**: content runs to the top edge, and the minimise and close controls float top-right over it. There is no title-bar mark or name.
- 1280×720 by default, 1024×640 minimum and **1600×900 maximum**. Like the Riot Client, there is no maximise: no Maximise button, double-clicking the title bar does nothing and Windows maximise (Win+↑, snap to top) is off (#135). Every layout below is checked at all three sizes.
- Scrolling pages (Patch Notes, the Settings panel) start below the 32px title bar, so their scrollbar never sits under the window controls (#135).
- Closing the window keeps the launcher in the tray, and Quit lives in the tray menu (see [Tray](#tray)).

## Layout

**Sidebar** (220px, `--rail`):
- Umbrella branding at the top: `/` glyph in accent at 44px, plus **Project Resurrected** in uppercase, 16px, weight 900. The label is provisional and lives in one swappable place.
- A text-only product list in this order: Project Diablo 2: Resurrected, Diablo II: Resurrected Forever, Diablo II: Resurrected 09. The active product is white with a 2px accent inset, and the others are muted. Items are 16px apart vertically (16px padding each side of the name; #135).
- Settings sits at the bottom with a cog icon, 34px from the bottom edge.
- There is no Home entry; the launcher opens on a product. D2R Tools does not appear.

**Sidebar badges**: a small square after the product name shows progress %, `↑` (update), `●` (running) or `!` (error, on `--button`).

**Product page, Overview tab** (layout E, approved in #20):
- Full-bleed hero art behind the whole main area, under a top, left and bottom gradient.
- **Overview | Patch Notes** tabs float top-centre in a square frame (`#101011b3`, 1px `--line`). The selected tab is on `--panel` with a 2px accent underline.
- Hero copy sits left at about 44% height: ribbon tag, a **title that is always exactly two lines** (scaling between 30px and 50px instead of wrapping), and one line of description.
- The bottom-left holds the action row. Play is solid red, at least 260px × 60px, with a plain uppercase label. Next to it is the square `⋯` **Manage** menu (Repair, Open saves folder, Open settings folder, divider, Uninstall… in `--detail`) and one readiness line ("● Ready · alpha 0.4.2 (latest)"). When a secondary button shares the row, the primary narrows to 200px.
- **Notices** sit directly above the action row: a panel with a 2px left rule, muted for information and accent for errors.
- The bottom-right holds a text-only news feed of cards stacked vertically ([#139](https://github.com/synpoox/resurrected-launcher/issues/139)), `clamp(300px, 32vw, 440px)` wide (about 330px at 1024, 410px at 1280). Each card is one link on a `--scrim` panel with a 1px `--line` border and 18px × 20px padding: tag · relative date, an 18px headline with ↗, and the item's optional 14px summary in up to two lines. The hero copy narrows so it stays left of the feed. It has no images and no in-launcher article view. **The feed shows announcements and guides only; release notes live in Patch Notes.** An empty feed reads "Nothing yet." Source, format and link rules: `docs/specs/news-feed.md`.

**Product page, Patch Notes tab** (approved in #20):
- Fills the whole main area except the sidebar, with no hero art. It scrolls with a dark scrollbar, and content passes under a solid top fade.
- The state's main action moves top-left at 48px height. There is no Manage button. Secondary buttons, readiness and detail lines are hidden, and progress shrinks to a 250px bar with Cancel.
- If the state has a notice, a red dot on the Overview tab points to it. Notices are shown on Overview only.
- Layout follows the League client's patch notes in structure and feel, with our tokens and no Riot branding (owner design pass, #128):
  - **Banner**: the product's bundled hero art (`src/art`, never a remote image), 260px high across an 880px column, 1px `--line` frame.
  - **Release head**, centred in a 760px column, newest release first: a 32px uppercase title in two lines, the product name over "Patch 0.2.0 notes". The patch version drops a leading or trailing status; the status becomes the tag. Then the notes' opening paragraph as the summary (17px), a 3px double `--line` divider, and the meta row: tag (`--detail`: Alpha, Beta or Release) · date.
  - **Table of contents**: a full-width row between rules, "TABLE OF CONTENTS" with `+`/`−` in `--detail`, collapsed by default. Opened, it lists every section heading as a ruled row. Choosing one jumps to that section (instantly, no smooth scrolling) and moves focus to its heading. Entries are buttons, because the webview has no links.
  - **Sections**: a 32px bold italic uppercase heading, then a 1px `--line` box on `--bg`. In the box: an optional description or `>` quote (quotation marks in `--muted`), a divider, `###` sub-headings (21px uppercase) and bullets nested up to three levels (disc, then circle). Body text is 17px at line height 1.7.
- Notes are a Markdown subset (`docs/specs/release-manifest.md`). It is parsed into data and rendered as React text: HTML, links and images are never interpreted, so the CSP stays `img-src 'self'`.
- The installed release's notes ship with it and display offline. Newer releases offline show "Patch notes for newer releases need a connection. Retry".

**Rules for both tabs:**
- **Products without a release** (Forever and 09 today) show no tabs. They get only the Coming soon ribbon, the description and one outlined link button, with no install, date, signup or account. A product with a bundled public page shows **About <product>**, which opens it (#80, `docs/specs/news-feed.md`). A product without one shows **Follow for updates on Discord ↗**, which opens the bundled project Discord directly, with no dialog. Forever and 09 show Follow today (#135).
- **Default tab**: Overview. The first launch after an update installs opens Patch Notes once, on the new release.

**Settings** (layout B, approved in [#19](https://github.com/synpoox/resurrected-launcher/issues/19)):
- Type scale and contrast (#135): body 16px in `--text`; row labels 18px; sub-nav tabs 17px in `--hero-copy` (selected `--text`), 14px vertical padding with 4px between tabs; hints, notes and notices 15px in `--hero-copy`; section labels 13px uppercase in `--hero-copy`; the heading 30px; rows have 20px vertical padding.
- Opens in the main area. A 220px sub-nav holds the tabs **Game · Launcher · Folders and plugins · Troubleshooting**. One panel shows at a time, and each row has a label, a hint and a right-hand control.
- **Game**:
  - The D2R location with its store and build, and "Change…", which opens the location dialog. Choosing a location moves nothing.
  - Browsing to a folder without `D2R.exe` shows an inline error, and a notice reports the recheck result.
  - While the game runs, the control is disabled with "Quit the game to change this".
- **Launcher**: **Start with Windows** switch, off by default, which opens to the tray.
- **Launcher → Launcher version** (#28): the version and its update status, with **Check for updates**. When an update is ready or has failed, the control becomes Restart to update or Try again (see [Launcher self-update](#launcher-self-update)).
- **Launcher → Download launcher updates automatically** (#28): switch, on by default. Hint when on: "Updates download in the background and install when you restart or Quit." Hint when off: "You'll be told when an update is available. Nothing downloads until you choose Download."
- **Launcher → About and credits** (new, #20): lists each hero image with its name, source and "Promotional art, not mod gameplay".
- **Folders and plugins**:
  - A PD2R group: saves, settings and filters, and **PD2R plugins** ("Loads only with PD2R", with the `core-*` note).
  - An All products group: **Global plugins** ("Loads for every product").
  - Opening a missing plugins folder for the first time creates it. A failure shows a red toast and never asks for administrator rights.
  - There are no buttons for products that aren't installed. A note says the launcher doesn't list, enable or disable plugins.
- **Troubleshooting**:
  - "Export diagnostics…" opens a review dialog: the full report text, a never-included list, the Windows user name replaced by `<you>`, and an optional launcher-log checkbox.
  - The launcher log is `launcher.log` in app-local data (`%LOCALAPPDATA%\com.projectd2r.launcher`), written in release builds too: a line at every start, state file problems, and failures that have no other trace (startup recovery, catalog refresh, launcher update download or install, the first-close notice). It is rotated to `launcher.log.1` at 1 MiB. The report includes its last 64 KiB, redacted ([#125](https://github.com/synpoox/resurrected-launcher/issues/125)).
  - "Save as file…" shows a toast with "Show in folder". Nothing is sent.
  - A pointer says Repair is in the product's `⋯` menu.

**Feedback surfaces**:
- **Notice**: explains a state in words, as above.
- **Dialog**: square, on `--panel`, with a 2px top rule (accent for destructive or risky actions) over a `#000000b3` backdrop.
- **Toast**: bottom-right, 380px wide, text 17px with 15px actions (#138). Information is muted and auto-dismisses; errors are accent and stay until dismissed. A closing toast fades and drops 8px over 200ms ease-in, and is inert meanwhile; under reduced motion it leaves at once ([#156](https://github.com/synpoox/resurrected-launcher/issues/156)).
- **Outcome feedback is short and stale-proof** ([#156](https://github.com/synpoox/resurrected-launcher/issues/156)): player-facing copy never shows a full path or a `\?\` prefix; an Open action opens the folder instead. An operation's failure toasts and the clean-install notice clear when a newer operation on that product starts. A failure that arrives before its own start answers stays.
- **Busy actions** (#138): from the click until the result or dialog arrives, the chosen button keeps the disabled look and shows a small square spinner beside its unchanged label (`aria-busy`). Every other action in its group is `aria-disabled` (still focusable). A second click sends nothing. A request counts as answered only when its effect shows: the product state shows the operation running, the uninstall outcome report arrives (the Uninstall confirm stays busy until then), or the launcher reports its next self-update phase. An outcome that arrives before the start answers ends the wait at once. As a safety net, busy never lasts more than 2 minutes without an answer. The product's pending action is kept per product, so leaving the page and coming back mid-request keeps it busy. This covers the product actions and the `⋯` Manage menu, Settings folder buttons, the location dialog, and the launcher self-update Download, Try again and Restart to update.
- **Dialogs open on the click**: a consent dialog that needs an inspection (Clean install, Uninstall) opens at once with a spinner and "Checking…" line in place of its body; its confirm button waits until the inspection answers. Not now closes it meanwhile, and the page's actions are free again at once. Update inspects first (busy) because only the answer says whether it is save-breaking.

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

## Launcher self-update

Variant C (toast, then a dot on Settings), approved in [#28](https://github.com/synpoox/resurrected-launcher/issues/28). Variants A (sidebar block) and B (titlebar chip) are kept on the prototype branch for the record only. Updates are signed Tauri updates ([ADR 0003](../adr/0003-tauri-with-signed-launcher-updates.md)). The notice never enters a product's hero, notice or action row.

- **Download** happens in the background with no notice. Settings → Launcher shows "Downloading 1.3.0 · 38%".
- **Automatic downloads off**: the launcher still checks for updates. When one exists, an information toast, "Launcher 1.3.0 is available", offers **Download**, **What's new** and **Later**, and Settings → Launcher shows **Download** in place of Check for updates. After Download, the flow is the same as above.
- **Ready**: an information toast, "Launcher 1.3.0 is ready", with **Restart to update**, **What's new** and **Later**. It does not auto-dismiss. Later closes it and puts a red dot on the Settings sidebar item until the update installs or the player opens Settings.
- **Install** happens on Restart to update, or the next time the player quits the launcher (tray Quit or the window's Quit, #113). Both check the kept installer's signature again first. Quit installs without relaunching the launcher. Quit is refused while an operation runs, so it never installs during an install transaction. While a game runs, Quit still exits but leaves the update kept for a later Quit or Restart to update.
- **Held restart**: while a product is running or any of its operations is in progress, Restart to update is disabled and the text says why, naming the operation: "Restarts after you quit PD2R", or "Restarts after PD2R finishes downloading" (install, update, clean install), "…finishes repairing", "…finishes uninstalling" or "…finishes setting up" (adopt). The ready toast adds what Quit does meanwhile. While a game runs, Quit closes the launcher without installing, and the update stays ready ("Quitting the launcher while PD2R runs doesn't install it; it stays ready for later"). While an operation runs, Quit itself waits, so the toast says "After that, it also installs when you Quit".
- **Restarting**: the main area shows the mark, "Updating the launcher…" and "Back in a few seconds." When Quit installs the update instead, the second line is "The launcher closes now. Start it again whenever you like.", because Quit doesn't relaunch.
- **Updated**: after the restart, an information toast "Updated to launcher 1.3.0" with **What's new**, which auto-dismisses.
- **Failures** use the error toast, which stays until dismissed. Later leaves the Settings dot. The current launcher always keeps working.
  - Download failed: "Couldn't download the launcher update", with **Try again**.
  - Signature check failed: "Launcher update rejected", saying the update was deleted and nothing was installed, with **Export diagnostics**.
- **Tray**: no self-update item or notification in v1.

## Journey states

Every state keeps the hero, ribbon, title and description. The state appears in the action row, a notice or a dialog ([#18](https://github.com/synpoox/resurrected-launcher/issues/18) has the full treatment of each).

| State | Key | Treatment |
| --- | --- | --- |
| D2R not found | `d2r-missing` | "Locate D2R", a red dot with "wasn't found on this PC", and a notice to choose the folder with `D2R.exe`. Nothing is moved or copied. |
| Choose D2R location | `d2r-choose` | Dialog with a radio list of detected installations (path, store, build, support, whether PD2R is there) and "Browse for another folder…". The choice is remembered and can be changed in Settings. |
| Loader not ready | `loader-missing` | "Get D2RLoader" opens the official download page (`https://d2rloader.net/download.html`) in the system browser, plus "Check again". The notice says D2RLoader is missing, or names the found version, and lists the versions this release allows. When an allowed version's bytes don't match the release's pinned official file (`loader_modified`, [#148](https://github.com/synpoox/resurrected-launcher/issues/148)), it says "D2RLoader.exe doesn't match the official D2RLoader 1.3.1-beta download, so it may be damaged or changed" and asks the player to reinstall it from the official page. The launcher never downloads or runs the loader installer ([#60](https://github.com/synpoox/resurrected-launcher/issues/60)). |
| Ready to install | `install` | "Install", plus the release, download size and free space needed. When some of the release's files are already in the product folder and none differ, the line also says how many are missing; Install adds only those ([#126](https://github.com/synpoox/resurrected-launcher/issues/126)). |
| Existing install to adopt | `adopt` | Nothing installed by the launcher, but the product folder already holds every file of the offered release: "Adopt" replaces Install, with "alpha 0.4.2 is already in your game folder · Adopt uses it without downloading". Adopt writes only the launcher's record ([#126](https://github.com/synpoox/resurrected-launcher/issues/126)). |
| Clean install finished | — | After a clean install, a notice above the action row: "**Clean install finished.** Your previous files were backed up." with **Open backup** (opens the newest backup folder) and **Dismiss**. No path is shown. It clears when another operation on the product starts ([#156](https://github.com/synpoox/resurrected-launcher/issues/156)). |
| Existing install that differs | `clean-install` | The product folder holds files that differ from the offered release (an older version, or changed managed files), so Install would be refused: "Clean install…" replaces Install, with a warning line "The PD2R in your game folder isn't alpha 0.4.2 · older or changed files". It opens an accent-topped consent dialog: the whole folder is moved aside as a backup in `mods` and never deleted, edited settings are carried forward (listed), a count of files that stay only in the backup, and saves, D2R, D2RLoader and other products aren't touched. "Clean install" or "Not now" ([#126](https://github.com/synpoox/resurrected-launcher/issues/126)). |
| Install progress | `downloading` | A progress bar replaces Play: label, %, size, speed and time left, with "Cancel". The sidebar badge shows the %. |
| Final application | `applying` | "Cancel safely" plus "Cancelling now waits until it's safe to stop, or rolls back". |
| Safe stop | `cancelling` | A striped bar, "Stopping safely…", "Restoring the previous files. Don't close the launcher.", and Cancel disabled. |
| Ready | `ready` | Play, `⋯`, and the readiness line. |
| Update vs play installed | `update` | "Update" (primary) next to "Play 0.4.1" (outlined), plus an `↑` badge. |
| Save-breaking | `save-breaking` | Accent-topped dialog. Saves stay in place and are not converted. The new version may refuse or change them. Rollback restores game files, not saves. A backup tip with Open folder. A required **acknowledgement checkbox** enables "Update to 0.5.0"; the alternative is "Not now" ([ADR 0001](../adr/0001-warn-on-save-breaking-updates-without-forced-isolation.md)). |
| Blocked prerequisites | `blocked` | Play disabled, "Check again", and an error notice with the reason, recovery steps and "How to fix". `!` badge. |
| Running | `running` | Disabled panel-coloured "Playing" with a pulsing dot, `⋯`, "Started 12 min ago · Repair and updates wait until you quit", and a `●` badge. The `⋯` menu stays: the folder items work, and Repair and Uninstall… are `aria-disabled` (still focusable) with "Quit PD2R first" under each ([#156](https://github.com/synpoox/resurrected-launcher/issues/156)). |
| Startup failure | `startup-failed` | "Try again", "Repair" and "Export diagnostics", with an error notice giving the exit code and next steps. |
| Repair | `repair` | Progress with files checked and to restore, and "saves and edited filters are kept". |
| Repair didn't finish | `repair-needed` | A Repair was ended (launcher killed, failure or cancel) before it committed, so readiness is `repair_needed` until one finishes ([#155](https://github.com/synpoox/resurrected-launcher/issues/155)): solid "Repair", ghost "Play <version>", `⋯`, and an error notice that files may be missing or damaged. `!` badge. |
| Manage menu | `manage` | `⋯` open: Repair, Open saves folder, Open settings folder, then Uninstall…. |
| Uninstall | `uninstall` | Accent-topped dialog: removes PD2R's files only. **Kept:** saves and settings with path and Open. **Not touched:** D2R, D2RLoader and other products. Once it succeeds, the dialog becomes a short result: "PD2R was uninstalled" and "Your saves and settings were kept.", with **Done**, **Open saves folder** and, when files the launcher didn't install are left, **Open PD2R folder**. No file lists ([#156](https://github.com/synpoox/resurrected-launcher/issues/156)). |
| Offline | `offline` | Play still works: "Offline · alpha 0.4.1 verified · can't check for updates". The news feed reads "News needs a connection. Retry". |
| Offline, never installed | `offline-empty` | No release to offer: Install is replaced by "Can't reach the release catalog" with "Try again" (refreshes the catalog). Nothing is downloaded or installed until it loads. |
| Coming soon | `coming-soon` | Ribbon, description and "Follow for updates on Discord ↗"; no tabs. It becomes "About Forever" once Forever has a public page. |

## Motion

Subtle only, with no video and no hero rotation: the hero changes only when the player picks a product.
- Hero art fades in over 450ms ease-out. Hero copy lifts 8px and fades over 450ms.
- Button background and transform transition over 150ms. Primary hover lifts 2px.
- Progress width transitions over 300ms. The running dot pulses on a 1.6s cycle.
- A busy button's spinner (a 14px square, three sides drawn) turns once every 800ms (#138). Under reduced motion it stays still.
- A closing toast fades and drops 8px over 200ms ease-in ([#156](https://github.com/synpoox/resurrected-launcher/issues/156)).
- Switching tabs and scrolling Patch Notes is instant, with no smooth scrolling.
- **Under `prefers-reduced-motion: reduce`, every animation and transition is off.** The running pulse becomes a static dot, and progress bars jump to their value.

## Accessibility

Target **WCAG 2.2 AA**: 4.5:1 for text, 3:1 for large text (24px, or 18.66px bold) and UI components. Measurements: [launcher shell contrast](../research/launcher-shell-contrast.md).
- Every token pair on `--bg`/`--rail`/`--panel` passes (5.21–17.47:1), as do white on `--button` (5.44) and `--ribbon` (6.25), and text over the hero art (worst 6.35). The tightest pass is the window-control glyphs over art (4.54).
- 10px labels are never large text and need 4.5:1.
- Hero art has no visible credit line. Each hero has an accessible name (e.g. "Reign of the Warlock key art (official D2R promotional art)"), and credits live in Settings.

**Keyboard and focus** (drafted in #20; check them in implementation):
- Tab order: window controls (minimise, close) → sidebar → tabs → main action and its row (or the Patch Notes action) → main content → news feed.
- Arrow keys move within the sidebar list, the tab list and open menus. Enter and Space activate. The vertical Settings section list takes ← → as well as ↑ ↓ (#127).
- Esc closes the open menu or dialog.
- Dialogs trap focus and return it to the control that opened them. Menus return focus to `⋯`.
- Every focusable element shows the 3px `--focus` ring at a 2px offset. The window controls use an **inset** ring, because an outset ring is clipped at the window's top edge.
- Tabs use `tablist`/`tab`/`tabpanel` roles. The Overview attention dot has an accessible label ("needs attention").
- Disabled controls keep their visible label and explain why next to them (e.g. "Quit the game to change this").

## Artwork

Official D2R promotional art, © Blizzard Entertainment, **never presented as mod gameplay**. The images match resurrected-web's hero set and are bundled in `src/art` as optimised WebP ([#165](https://github.com/synpoox/resurrected-launcher/issues/165)), credited in Settings → About and credits as "Official D2R promotional artwork © Blizzard Entertainment", the website's credit.

| Product | Image | Framing |
| --- | --- | --- |
| PD2R | Reign of the Warlock key art | `55% center`, cover |
| Forever | Necromancer promotional art, 2600×906 (the website's Forever hero; replaces the 30th Anniversary key art from [#22](https://github.com/synpoox/resurrected-launcher/issues/22); same composition as 09, so the same framing) | `74% 22%`, `auto 135%` |
| 09 | Paladin promotional art, 2600×906 (the website's 09 hero) | `74% 22%`, `auto 135%` |

Source URLs are in the prototype's `src/data.js`. The real app **bundles or caches** the art instead of hotlinking it, so heroes still show offline.

**Release gate:** no licence covers using this art in a third-party launcher. Before a public release, either get permission or replace the art.

## Not specified

Each item has a default to build to unless a later decision replaces it.

| Gap | Default |
| --- | --- |
| First-run onboarding | No separate onboarding; use the inline states `d2r-missing`, `d2r-choose` and `loader-missing`. |
| Splash or loading screen | None. The window opens on the last product, or PD2R. |
| News feed size | Up to three items, newest first. Offline and empty lines as above. |
| "Reset player settings" | Not designed. It must be separate from Repair ([#5](https://github.com/synpoox/resurrected-launcher/issues/5) Q9). |
| Clean-install recovery for unrecognised files | Before anything is installed: `clean-install` above. With a release installed (an update that meets unrecognised files): not designed; use the `blocked` notice treatment. |
| Patch Notes contrast and keyboard checks | Not measured. Check them against the targets above during implementation. |
