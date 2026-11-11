# Changelog

## 0.1.3

Smoother launcher, safer game updates, and a round of fixes.

### New

- **Save-break warning for game patches.** If a game patch changes how character saves are stored, the launcher warns you before it installs, for Install, Clean install and Update alike.
- **Reopens where you left it.** The launcher window comes back on the same monitor and spot where you last closed it. The first time, it opens in the middle of the screen.
- **Smoother motion.** Buttons, tabs, dialogs and screen changes now animate more gently.
- **Product page polish.** A tidier action row next to Play, steadier tabs, a softer Play fade-in and a clearer glow on the main button.
- **News.** See all lists every item when a product has more than three, Retry tells you it is trying again, and the feed hides itself when a product has no news.
- **Troubleshooting.** Copy log and Open logs folder sit next to Export diagnostics.
- **What's new.** From the next update on, the launcher shows its release notes once after it updates itself.
- **Patch Notes images.** Patch Notes can show a header image for each release.

### Changed

- The window has one fixed size: 1600×900, or 1280×720 on smaller screens. It can no longer be resized.
- The Forever page now uses the 30th Anniversary key art.
- The art credits list is no longer in Settings.
- The browser's right-click menu now works only in text fields, and browser keys such as F5 and Ctrl+F do nothing.

### Fixed

- Play no longer fails when the launcher was started from its installer after a browser download.
- Play no longer reports "The game closed while starting" when the loader takes a moment to hand off to the game.
- A successful Repair, Update or Clean install clears the "The game closed while starting" notice.
- A briefly refused file rename is retried instead of failing the update. If Windows keeps blocking it, the message now tells you to close Explorer windows in the PD2R folder.

## 0.1.2 — first public release

The first public build of the Project Resurrected launcher for Diablo II: Resurrected. Versions 0.1.0 and 0.1.1 were internal test builds and were never published.

### What you can do

- **Install and play Project Diablo 2: Resurrected (alpha)** on top of your own Diablo II: Resurrected. The launcher finds your game, downloads the release and checks every file before you play.
- **Update with one click.** Updates download, verify and apply in place. A failed or cancelled update rolls back cleanly.
- **Keep your characters safe.** Installing or updating never deletes or converts your saves, and keeps edited loot filters and settings. A release marked save-breaking warns you and asks before it updates.
- **Repair** from the **⋯** menu next to Play restores missing or damaged mod files.
- **Read Patch Notes** for each release, including offline for the version you have installed, and the **news feed** for announcements and guides.
- **D2RLoader check.** The launcher tells you if D2RLoader is missing or the wrong version.
- **Diagnostics.** Settings → Troubleshooting → Export diagnostics writes a report you review and save yourself. Nothing is sent.
- **Coming soon:** Diablo II: Resurrected Forever and Diablo II: Resurrected 09 are listed, with a link to follow them on Discord.

### Good to know

- Windows 10 (22H2) or Windows 11, 64-bit. The installer is per user and needs no administrator rights.
- The installer is not Authenticode-signed, so Windows SmartScreen may warn you. Choose **More info → Run anyway**.
- The launcher updates itself. It checks for a new version, verifies its signature and installs it when you restart or quit, never while a game is running.
- Uninstalling the launcher leaves your saves and Diablo II: Resurrected untouched.
