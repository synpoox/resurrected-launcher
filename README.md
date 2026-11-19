# Resurrected Launcher

Resurrected Launcher installs, updates and starts Diablo II: Resurrected mods. It runs on top of your own Diablo II: Resurrected installation.

[![CI](https://github.com/synpoox/resurrected-launcher/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/synpoox/resurrected-launcher/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Products

- **Project Diablo 2: Resurrected**: alpha, available now.
- **Diablo II: Resurrected 09**: coming soon.

## Features

- Installs and updates mods on your Diablo II: Resurrected installation.
- Checks downloaded files against published hashes.
- Keeps your saves, loot filters and settings. The launcher does not convert saves.
- Warns you before a save-breaking update and asks you to confirm it.
- Rolls back a failed or cancelled update.
- Repair restores missing or damaged mod files.
- Shows Patch Notes for each release, including the installed release offline.

## Download

Download `Project-Resurrected_<version>_x64-setup.exe` from the [Releases page](https://github.com/synpoox/resurrected-launcher/releases/latest) and run it.

- The installer installs for your user only. It needs no administrator rights.
- The installer needs an internet connection only if Microsoft WebView2 is missing from your PC.
- The installer is not Authenticode-signed. Windows SmartScreen may warn you. Choose **More info**, then **Run anyway**.

## Requirements

- Windows 10 (22H2) or Windows 11, 64-bit
- Your own Diablo II: Resurrected installation
- [D2RLoader](https://d2rloader.net/download.html). The launcher tells you if it is missing or the wrong version.
- An internet connection to install and update.

## Updates

- The launcher updates itself.
- Mod releases are installed and updated from the launcher. Each release's Patch Notes are shown in the launcher.
- Back up your saves before you update. Use Settings → Folders and plugins → Open folder.
- The launcher does not delete or convert saves. A new game version can still reject or change them. Going back to an older version does not restore saves.

## Troubleshooting

- **Game won't start or files look damaged:** open the **⋯** menu next to Play and choose **Repair**. Repair keeps your saves, loot filters and settings.
- **Still not working:** go to Settings → Troubleshooting → Export diagnostics. A review dialog shows the report before you save it.

## Plugins

You can install plugins yourself. Plugins are allowed in offline play and in direct TCP/IP co-op, where the mod supports it. They are not allowed in project-hosted multiplayer or ladder play.

## Build from source

Requirements: Windows 10 or 11, Node.js 22, Rust 1.90 (installed with [rustup](https://rustup.rs)), and the [Tauri Windows prerequisites](https://tauri.app/start/prerequisites/#windows). You do not need Diablo II: Resurrected to build.

```sh
git clone https://github.com/synpoox/resurrected-launcher.git
cd resurrected-launcher
npm ci
npm run tauri dev
```

To build the installer, run `npx tauri build`. The installer is written to `src-tauri/target/release/bundle/nsis/`.

## Documentation

- [CONTRIBUTING.md](CONTRIBUTING.md): developer guide, tests and project layout
- [GLOSSARY.md](GLOSSARY.md): terms used in the launcher
- [docs/specs/launcher-v1.md](docs/specs/launcher-v1.md): launcher v1 spec
- [docs/adr/](docs/adr): architecture decisions
- [docs/release/launcher-signing.md](docs/release/launcher-signing.md): launcher release signing
- [docs/third-party.md](docs/third-party.md): third-party components

## License

[MIT](LICENSE). The bundled 7-Zip keeps its own license ([details](docs/third-party.md)).
