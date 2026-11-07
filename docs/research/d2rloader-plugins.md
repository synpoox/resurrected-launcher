# How D2RLoader discovers, loads and disables plugins

Research for [#15](https://github.com/synpoox/resurrected-launcher/issues/15) (map [#12](https://github.com/synpoox/resurrected-launcher/issues/12)), feeding the per-product plugin toggle grilling in [#16](https://github.com/synpoox/resurrected-launcher/issues/16). Context: [ADR 0004](../adr/0004-manual-plugins-allowed-in-direct-coop.md).

Investigated 2026-11-07. D2RLoader itself is closed source ([Installer README](https://github.com/D2RLoader/Installer/blob/752c0afe03382d085eaee1f44656c6d2e731fedf/README.md)), so loader behaviour comes from its official docs, the official PluginSDK, and the owner's own field notes. Nothing was run against a real game in this research.

## Sources

| Key | Source | Trust |
|---|---|---|
| SDK | [PluginSDK README @03a5af9](https://github.com/D2RLoader/PluginSDK/blob/03a5af9e0feb3f95f95e148e330e01f6635950df/README.md) (SDK 0.4.0, plugin ABI 4), [resource.h](https://github.com/D2RLoader/PluginSDK/blob/03a5af9e0feb3f95f95e148e330e01f6635950df/include/D2RLPlugin/resource.h), [SECURITY.md](https://github.com/D2RLoader/PluginSDK/blob/03a5af9e0feb3f95f95e148e330e01f6635950df/SECURITY.md) | Official, primary |
| DOCS | [d2rloader.net/docs.html](https://d2rloader.net/docs.html) (General settings, Extensions tab), [changelog](https://d2rloader.net/changelog.html) (latest 1.3.1-beta) | Official, primary; mutable website, read 2026-11-07 |
| WIKI | [installing-a-mod.md @ccca8c7](https://github.com/D2RLoader/wiki/blob/ccca8c729a967bd36b627a7a61c0e22c25bdbd46/players/installing-a-mod.md) | Official, primary |
| KB | [d2rAI-toolkit knowledge/d2rloader.md @ec5b2e3](https://github.com/synpoox/d2rAI-toolkit/blob/ec5b2e3ff1798a66e572fd22b001b8f954597e0d/knowledge/d2rloader.md) (private) | Owner's observations on 1.3.1; secondary but first-hand |
| Plugin repos | [scrollable-waypoints @10a20de](https://github.com/synpoox/scrollable-waypoints/tree/10a20de31356287fab8f7d15c3615f6f4fadc1fe), [d2r-tools @24884d7](https://github.com/synpoox/d2r-tools/tree/24884d7340544637b82f2f9b77b53a0ff33becd0), [better-automap @6ff23cf](https://github.com/synpoox/better-automap/tree/6ff23cf1bd01afadc127e9c5df521ca0681183b9), [monster-count @debfd81](https://github.com/synpoox/monster-count/tree/debfd81ee87f8aa87b5a71ebffdadaf638010c68), [d2rl-charm-inv release 1.0.1](https://github.com/synpoox/d2rl-charm-inv/releases/tag/1.0.1) | Primary for how the owner packages plugins |
| Products | [pd2-resurrected @3416271](https://github.com/synpoox/pd2-resurrected/tree/341627184a92d234b5c9b95eadcf6a75d97beb60), [resurrected-forever @3e61f5d](https://github.com/synpoox/resurrected-forever/tree/3e61f5d425c9d3d29cd4923e5a207527a5340831), [resurrected-09 @fb32442](https://github.com/synpoox/resurrected-09/tree/fb32442a0c00b12c1e7e2e5a56b5cfceb4af1f88) | Primary |

The `D2RLoader/D2RL-Plugins` repo ("downloadable plugins") is empty, so there is no official plugin registry or manifest format to adopt.

## Verified facts

### Discovery and load order

- Two plugin folders exist per game installation: global `<game>\d2rloader\plugins\` and mod-scoped `<game>\mods\<Mod>\d2rloader\plugins\`. JSON memory patches sit beside them in `...\d2rloader\patches\`. [SDK "Load a Plugin"; DOCS Extensions]
- Mod plugins load first. A mod plugin with the same id as a global one replaces the global copy while that mod runs. [SDK]
- Global plugins load with every mod (and with no mod). Mod plugins load only when that mod is the active one. [DOCS; KB]
- A plugin flagged `ModScopedOnly` loads only from a mod folder; the owner's notes and code say the loader refuses it in the global folder. [SDK flag list; KB; scrollable-waypoints `plugin/src/plugin.cpp` header comment]
- Which mod is active comes from `D2RLoader.exe -mod <Folder>` or `default_mod` in `<game>\d2rloader\config\d2rloader.toml`. A mod's own `d2rloader.toml` overrides only `[d2rcore.*]` and cannot pick the mod. [WIKI; KB]

### What makes a file a plugin

- A plugin is a native x64 DLL. It must embed an `RCDATA` manifest resource id `0x03E9` holding the plugin ABI as a DWORD, and export `D2RLoaderGetPluginInfo` and `D2RLoaderLoadPlugin` (`D2RLoaderUnloadPlugin` optional). [SDK; resource.h]
- The loader reads the manifest before calling any code and rejects unsupported ABIs (accepts 2 to 4). A DLL without the manifest is listed as "not compatible ... looks like an old alpha plugin". [SDK; KB]
- The plugin's identity is `PluginInfo::id` inside the DLL, not the filename. `PluginInfo` also carries name, version (SemVer, `invalid` if unparsable but still loads), author, description and role flags. [SDK]
- Naming convention: the companion data archive is `d2rl-<plugin-id>.mpq` beside the DLL, and the DLL "should" be `d2rl-<plugin-id>.dll`. [SDK resources section] The owner does not follow it consistently: `d2rl-charm-inv.dll` + `d2rl-charm-inv.mpq` do; `scrollable-waypoints.dll`, `better-automap.dll`, `multispec.dll` etc. do not (CMake `OUTPUT_NAME` = id without prefix).
- An optional default config is embedded as `RCDATA 0x03EA`. On first load the loader writes `<scope>\d2rloader\config\<plugin-id>.toml` only if missing and never overwrites it. [SDK] Plugin logs go to `...\d2rloader\logs\`. [d2r-tools README; KB]

### Enable and disable controls that exist

- **No per-plugin enable/disable exists in the loader.** The only documented switches are two game-wide, restart-required General settings: *Allow Global Extensions* (all plugins and patches in `<game>\d2rloader\...`) and *Allow Mod Extensions* (all plugins and patches of the active mod; "can remove features that the mod expects"). [DOCS General settings] Their TOML key names are not documented.
- The owner's plugin READMEs define disabling as removing the file: "To uninstall, delete the DLL." [scrollable-waypoints README] "To update, replace the DLL." [d2r-tools README]
- Some plugins have their own `enabled = true` keys per feature in their TOML (better-automap `[minimap]`/`[overlay]`/`[zoom]`; the SDK example `[my-plugin] enabled`). These are plugin-specific, not a loader contract.
- The in-game Extensions tab and the `plugins` console command report what actually loaded, with source label. [DOCS Extensions; changelog] This is the only authoritative "is it on" signal; it is inside the game, not readable by the launcher except via `d2rloader.log` `[PLUGIN]` lines. [KB]

### Roles and why disabling is not free

- Every ABI 3+ plugin declares exactly one role: `Client`, `Server` or `Shared` (ABI 2 plugins count as `Shared`). Server and Shared plugins are recorded in each character's plugin history (`.d2rl` sidecar); Client plugins are not. [SDK; changelog 1.x "binary .d2rl character sidecars"]
- Characters saved with a missing or different plugin environment get a combined warning and reduced opacity on the character screen. If an offline character holds items from a missing inventory-page plugin, the loader backs up the save to `D2RLoader Backups\Recovery` and loads it without those items. [changelog; SDK extended-save section]
- In TCP/IP, Shared plugins must match between host and client by id, version, ABI, scope and flags; the host names missing/extra/changed Shared plugins. [SDK; changelog]
- A plugin is native code running with the game's rights. [SECURITY.md]

### How the owner's plugins ship

| Plugin | Ships as | Install location the README gives | Role (from source) |
|---|---|---|---|
| scrollable-waypoints | release asset `scrollable-waypoints.dll` + `.sha256` (v1.0.0); also inside the d2r-tools suite (1.0.1) | global `<game>\d2rloader\plugins\` | Client, NativeHooks |
| d2r-tools suite (better-xp-bar, multispec, player-bars, quick-npc, scrollable-waypoints) | per-plugin `<plugin>-X.Y.Z.zip` (DLL + README + CHANGELOG), an all-in-one bundle zip, `SHA256SUMS.txt` | global **or** `mods\<Mod>\d2rloader\plugins\`, "one location per plugin, never both" | multispec Shared; player-bars Client |
| d2rl-charm-inv | release assets `d2rl-charm-inv.dll` + `d2rl-charm-inv.mpq` | not documented (no README) | not inspected (binary only) |
| better-automap | source only, no release | not stated | Client, NativeHooks |
| monster-count | source only, no release | mod-scoped | Shared, ModScopedOnly, NativeHooks |
| PD2R `core-rules`, `core-maps`, `core-ui` | inside the PD2R release archive | `mods\PD2Resurrected\d2rloader\plugins\` | release-managed, required |

Sources: release listings, READMEs and `plugin.cpp` at the pinned commits; d2r-tools [`tools/release.ps1`](https://github.com/synpoox/d2r-tools/blob/24884d7340544637b82f2f9b77b53a0ff33becd0/tools/release.ps1); PD2R [`tools/release.py`](https://github.com/synpoox/pd2-resurrected/blob/341627184a92d234b5c9b95eadcf6a75d97beb60/tools/release.py). better-auras is an empty repo; its source lives in d2r-tools.

### Per product

- PD2R: mod folder `PD2Resurrected`; ships three required plugins in its mod-scoped folder. Optional player plugins can sit in the same folder or the global one.
- Forever and 09: both READMEs say "Status: planning"; no mod folder name, plugins or releases exist yet. Forever's repo has no `mod.json` at HEAD; the `ProjectForever` folder name in prior research came from a different, older repo.

## Inference (not verified, needs a real-game test)

1. **Discovery is probably `*.dll` in the top level of each plugins folder.** Nothing documents whether the loader also scans subfolders or files with other extensions. A rename like `x.dll.disabled` is very likely ignored, but untested.
2. **The safest off-state is outside the plugins folder** (for example a launcher-owned `<scope>\d2rloader\plugins-disabled\`), moving the DLL and its `d2rl-<id>.mpq` companion together. This avoids depending on (1). The plugin's `config\<id>.toml` and `input-bindings.toml` entries should stay put, so re-enabling keeps the player's settings (the loader never overwrites an existing config).
3. **Only mod-scoped plugins can be toggled per product by moving files.** A global plugin loads for every product and vanilla; disabling it "for PD2R only" would need a launch-time shuffle (move out, launch, move back after exit), which breaks on crash or a launch outside the launcher. Copying it into one mod folder cannot disable it there, it only replaces it. *Allow Global Extensions* is game-wide and coarse.
4. **The launcher cannot learn a plugin's id or role without the game.** Reading `PluginInfo` means loading the DLL (runs its code), which the launcher must not do. It can safely read the PE resources statically: manifest `0x03E9` (ABI, proves "looks like a plugin") and embedded config `0x03EA` (its top table name usually is the id). Otherwise identify by filename + SHA-256.
5. **Running game blocks file moves.** Windows usually lets a loaded DLL be renamed but not deleted; either way changes take effect only on next start, and the docs say "Close the game" before installing. The launcher should refuse toggles while `D2R.exe` (or `D2RLoader.exe`) from that installation is running.
6. **Disabling a Shared/Server plugin changes how existing characters look and play** (warning badge, possible item recovery for page plugins) and breaks TCP/IP matching with peers who keep it. Client plugins are safe to toggle in that respect.

## What a launcher toggle could change safely

- Move an optional, player-installed plugin DLL (plus same-id `.mpq` companion) between the active product's `mods\<Mod>\d2rloader\plugins\` and a launcher-owned disabled folder, only when that installation's game is not running.
- Never touch release-managed plugins (recognised by product release path + hash, per ADR 0004 and the #8 resolution), plugin configs, logs, input bindings, saves, `d2rloader.toml`, or patches.
- Show global plugins as "on for every product" and not toggleable per product (or offer a clearly game-wide toggle; owner decision).

## States the toggle must handle

| State | Evidence | Suggested behaviour |
|---|---|---|
| Game running from this installation | Fact 5 above | Disable the control; explain "close the game first" |
| Plugin present and enabled / disabled | file in plugins vs disabled folder | Normal toggle |
| File without D2RLoader manifest | SDK manifest rule | Show as "not a D2RLoader plugin / not compatible"; still toggleable or hidden (decide) |
| Unknown plugin (no hash match) | ADR 0004 | Show filename; id unknown; label as player-installed |
| Same plugin installed globally and mod-scoped, or twice in one folder | SDK replace rule; d2r-tools "never both"; KB stray-copy trap | Warn; mod copy wins for that product |
| Release-managed plugin | PD2R `core-*` | Locked, never toggleable |
| Shared/Server role (if detectable) | SDK roles | Warn about character badges, item recovery and co-op matching |
| Global plugin | Fact 3 | Not per-product; read-only or game-wide toggle |
| Allow Global/Mod Extensions turned off in the loader | DOCS | Launcher toggles have no effect; surface this if the key can be read |
| Disabled-folder file missing or plugins folder missing/denied | normal-user rule in #8 | Report, never recreate silently or elevate |
| Product not installed (Forever, 09 today) | READMEs | No plugin section |
| Move interrupted half-way (DLL moved, mpq not) | — | Treat DLL+mpq as one unit; repair on next scan |

## Conflicts to resolve

- ADR 0004 / the #8 resolution say the launcher "never silently removes, renames or disables" player plugins. A user-initiated toggle is not silent, but the ADR should be amended to record the toggle and its limits.
- scrollable-waypoints exists as both a standalone 1.0.0 release and a d2r-tools 1.0.1 build with the same id; players can end up with both.
