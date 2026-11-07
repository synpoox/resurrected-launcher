# Use Tauri with signed launcher updates

Build the fresh launcher with Tauri 2, React/TypeScript and a Rust native core to support the artwork-led UI while keeping installation and process operations native. The owner permits Tauri's key-based launcher-update signatures, which avoid a bespoke updater and do not require a paid Windows publisher certificate; signed game-release metadata and paid Authenticode certificates remain deferred. This narrows the earlier signature deferral for launcher self-updates only and requires protected signing keys and a recovery plan.

See the [architecture resolution](https://github.com/synpoox/resurrected-launcher/issues/8) for the module interfaces, implementation order and validation gates.
