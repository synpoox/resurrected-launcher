# Changelog

## Unreleased

### New

- The product page shows the Diablo II: Resurrected logo at the top left of the hero, with Project above it (PD2R) or 09 below it.

### Fixed

- When a newer D2RLoader is installed that the latest PD2R allows, the launcher now offers Update and the ⋯ menu instead of only asking you to get D2RLoader. (#267)
- If the game closes while starting, or a Repair didn't finish, and an update is available, Update is now offered, and the ⋯ menu is there after a startup failure. (#267)
- When a previous change didn't finish, Check again now says why it can't recover (a File Explorer window open in the folder, or the game running), and Repair and Uninstall in ⋯ are disabled until it does. (#267)
- If you delete the PD2R folder in File Explorer, the launcher now offers Repair instead of showing Ready and starting the game with no mod. (#267)
- One product's broken or too-new release no longer makes every product show offline. The others keep their updates. (#271)
- A download or catalog refresh that stalls on a dead connection now fails after a timeout instead of hanging, so Cancel, Quit and Play work again. (#269)
- An installed PD2R now has Clean install… in its ⋯ menu, so an Update that stops on a file the launcher did not install can be recovered. (#264)
- If you change the D2R location to a folder without PD2R, the launcher now offers Install there instead of Update, Repair and Uninstall.
- Install, Update and Repair no longer fail with "cannot write ... (os error 3)" when the D2R folder has no PD2Resurrected folder yet. The launcher now creates it.

## 0.1.5

Community links in the sidebar, and the launcher now offers updates that fix an unsupported D2R version.

### New

- GitHub, Discord and Ko-fi icons sit above Settings in the sidebar. Each opens in your browser. The Discord link now uses the new invite.

### Fixed

- When a new D2R version stops your installed PD2R from working but the latest PD2R supports it, the launcher now offers Update instead of only saying it can't play. The tray offers Update too.
- Check again now looks for newly published releases, not just your files, so you no longer have to restart the launcher to see a new update.

## 0.1.4

A much smaller installer and a tidier product list.

### Changed

- Diablo II: Resurrected Forever is removed from the launcher entirely. The 09 description no longer mentions 3.3, and the Forever and 09 hero images are swapped.
- The launcher installer is much smaller to download. It now needs an internet connection during install only if your PC is missing Microsoft WebView2.

## 0.1.3

Smoother launcher, safer game updates, and a round of fixes.

### New

- **Save-break warning for game patches.** If a game patch changes how character saves are stored, the launcher warns you before it installs, for Install, Clean install and Update alike.
- **Reopens where you left it.** The launcher window comes back on the same monitor and spot where you last closed it. The first time, it opens in the middle of the screen.
- **Smoother motion.** Buttons and dialogs animate more gently. Switching products crossfades the background art, and switching between Overview and Patch Notes slides the new tab in.
- **Product page polish.** A tidier action row next to Play, steadier tabs and a clearer glow on the main button.
- **News.** See all lists every item when a product has more than three, Retry tells you it is trying again, and the feed hides itself when a product has no news.
- **Troubleshooting.** Copy log and Open logs folder sit next to Export diagnostics.
- **What's new.** From the next update on, the launcher shows its release notes once after it updates itself.
- **Patch Notes images.** Patch Notes can show a header image for each release.
- **Smooth Patch Notes contents.** Choosing an entry in the Patch Notes contents list scrolls smoothly to that section.

### Changed

- The window has one fixed size: 1600×900, or 1280×720 on smaller screens. It can no longer be resized.
- The Forever page now uses the 30th Anniversary key art.
- The art credits list is no longer in Settings.
- The browser's right-click menu now works only in text fields, and browser keys such as F5 and Ctrl+F do nothing.
- Text in the launcher can no longer be highlighted, like a regular app.

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
