# Contributing

Developer guide for the launcher. For what the launcher does for players, see the [README](README.md).

## Roadmap

Remaining v1 work is tracked in the [launcher v1 map (#41)](https://github.com/synpoox/resurrected-launcher/issues/41) and its open sub-issues.

## Tech stack

| Layer | Stack |
| --- | --- |
| App shell | [Tauri 2](https://tauri.app) ([ADR 0003](docs/adr/0003-tauri-with-signed-launcher-updates.md)) |
| UI | React 19, TypeScript, Vite |
| Native core | Rust 1.90 (`src-tauri/`) |
| Tests | Vitest and Testing Library; `cargo test` |
| Installer | NSIS, per-user install |

## Requirements

- Windows 10 or 11. The native core uses Windows APIs, and CI runs on `windows-latest`.
- Node.js 22 and npm.
- Rust through [rustup](https://rustup.rs). `src-tauri/rust-toolchain.toml` pins 1.90.0 with clippy and rustfmt, and rustup installs it on first use.
- [Tauri's Windows prerequisites](https://tauri.app/start/prerequisites/#windows): Microsoft C++ Build Tools and WebView2.

You don't need D2R for development. The browser UI and the frontend tests use fixtures, and the Rust tests use disposable fixture installations.

## Getting started

```sh
git clone https://github.com/synpoox/resurrected-launcher.git
cd resurrected-launcher
npm ci
npm run tauri dev
```

`npm run tauri dev` runs the UI against the native commands, so it uses your real launcher state and detects your real D2R. To work on the UI with fixture data instead, run `npm run dev` and open <http://localhost:1420> in a browser. In dev builds, `?fixture=<key>` picks a journey state (even under Tauri), and ← / → cycle through them. Add `&slow=<ms>` to delay the calls a click waits on (inspect, start, cancel, play, catalog, open folder, open the D2RLoader download page; not startup recovery), so busy states stay visible (#138).

To make a release build, run:

```sh
npx tauri build
```

It writes an NSIS installer under `src-tauri/target/release/bundle/nsis/`. Signed release builds run in CI. See [launcher signing](docs/release/launcher-signing.md).

## Tests and lint

[CI](.github/workflows/ci.yml) runs these checks and also `npx vite build`.

```sh
npm run typecheck
npm run lint
npm test

cd src-tauri
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

The slice 3 gate's phase A runs against a real D2R copy only when `SLICE3_GATE_D2R` is set, and skips otherwise. Point it only at a disposable copy, never at a live installation.

## Project layout

```text
src/                 React UI
  api/               LauncherApi: live adapter and fixture scenes
  art/               hero art (placeholders) and the mark
  shell/             window, sidebar, title bar
  product/           product page, Patch Notes, journey dialogs
  settings/          Settings and troubleshooting
  selfupdate/        launcher self-update notice
  ui/                shared controls: dialog, menu, toast, switch
src-tauri/           Tauri app and Rust native core
  src/app/           Tauri commands, window controls, settings
  src/lifecycle/     window, tray, single instance, Quit
  src/catalog/       release feed and manifest validation
  src/install/       download, extract, install transactions, recovery
  src/game/          D2R detection, readiness, launch, process tracking
  src/state/         preferences, cached metadata, diagnostics
  src/contract/      types shared with the frontend
  resources/7zip/    bundled 7-Zip console
  tests/             integration tests, failure matrix, slice 3 gate
fixtures/            catalog documents and fixture product installations
docs/                specs, ADRs, release docs, screenshots
```

## Documentation

| Topic | Document |
| --- | --- |
| Vocabulary | [GLOSSARY.md](GLOSSARY.md) |
| Launcher v1 spec (index) | [docs/specs/launcher-v1.md](docs/specs/launcher-v1.md) |
| UI design spec | [docs/specs/launcher-ui.md](docs/specs/launcher-ui.md) |
| Release feed and manifest | [docs/specs/release-manifest.md](docs/specs/release-manifest.md) |
| Architecture decisions | [docs/adr/](docs/adr) |
| Launcher release signing | [docs/release/launcher-signing.md](docs/release/launcher-signing.md) |
| Third-party components | [docs/third-party.md](docs/third-party.md) |
| Shared website context | [docs/agents/web-context.md](docs/agents/web-context.md) |
| Screenshots of every UI state | [docs/screenshots/76/](docs/screenshots/76), [docs/screenshots/77/](docs/screenshots/77) |

## Workflow

- Work is tracked in [GitHub Issues](https://github.com/synpoox/resurrected-launcher/issues).
- Read [AGENTS.md](AGENTS.md) and the [glossary](GLOSSARY.md) first, and use glossary terms in issues and PRs.
- Flag any change that contradicts an [ADR](docs/adr).
- PRs must pass CI.

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
