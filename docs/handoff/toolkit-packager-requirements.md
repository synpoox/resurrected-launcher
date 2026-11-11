# Handoff: d2rAI-toolkit packager changes for schema 3

For the maintainers of [d2rAI-toolkit](https://github.com/synpoox/d2rAI-toolkit) (`release/package_release.py`, `release/release_config.py` and the publish step). This repo does not edit the toolkit; this note lists what its output must become so the launcher can read it. Wire format: [release-manifest.md](../specs/release-manifest.md). Example output: `fixtures/catalog/valid/`. Decision: [#6](https://github.com/synpoox/resurrected-launcher/issues/6), slice 1 of [#8](https://github.com/synpoox/resurrected-launcher/issues/8), implemented launcher-side by [#33](https://github.com/synpoox/resurrected-launcher/issues/33).

Extend the existing packager; do not add a second pipeline.

## Manifest output (schema 3)

Compared with today's `schemaVersion: 2` output (reviewed at [707f9df](https://github.com/synpoox/d2rAI-toolkit/blob/707f9df317021dad885ddd44329588742723d259/release/package_release.py)):

1. Write `schema_version: 3` and switch all keys to snake_case (`emptyDirs` -> `empty_dirs`, `launcherMin` -> `min_launcher_version`, ...). Unknown keys are rejected, so drop `name`, `modFolder`, `complete`, `pluginVersion` and `fileCount`/`totalSize` at top level.
2. Identity: add `product_id` (stable, e.g. `pd2r`) and `release_id` (immutable, unique per published bytes, e.g. `pd2r-0.4.0-alpha.1`), `display_version`, `release_date` (`released`), `status` (`alpha`/`beta`/`release`), inline `release_notes` text, and a required `min_launcher_version`.
3. Archive: `archive.url` is the final HTTPS download URL; add `file_count` and `total_size` inside `archive`, computed from `files`. Keep `format: "7z"`, `size` and `sha256` generated from the actual output.
4. Files: add `ownership` to every entry: `managed`, or `shipped_default` for files listed in `package.preserve` (loot filter, loader configuration). Keep sizes and hashes generated from the actual staged files. Keep `empty_dirs`.
5. Replace `modFolder` with `install_root: "mods/<folder>"` (PD2R: `mods/PD2Resurrected`). Keep the archive's one top-level `<folder>/` directory; `files` paths are relative to it.
6. Replace `d2r` with `compatibility.game_builds`: `{build, platform: "windows_x64", executable, size, sha256}` per verified D2R build, from the real executable. A version label alone is rejected.
7. Add `compatibility.loader_versions` (PD2R 0.1.0-alpha: `["1.3.1-beta"]`) and move the loader settings from `launch` into `compatibility.required_settings` as `{file, key, value}` pointing at a `shipped_default` file. Use dotted TOML keys for `d2rloader.toml`: PD2R needs `{file: "d2rloader/config/d2rloader.toml", key: "d2rcore.save_format.mode", value: "extended"}`. Do not emit `default_mod`: Play passes `-mod <folder>`. Remove `launch` and any launch arguments: metadata may not carry commands or arguments.
   Also emit `compatibility.loader_builds` ([#121](https://github.com/synpoox/resurrected-launcher/issues/121)): `{version, file, size, sha256}` for each official loader file of each allowed version, read from the real files. PD2R: `{version: "1.3.1-beta", file: "D2RLoader.exe", size: 23331160, sha256: "93021dad48533bcfca8a95c69a0cc00cdb9008a3ce6ceba1d04c7aca71985a10"}`. Every `loader_versions` entry needs at least one build.
8. Replace `saveFolder` with `saves: {folder, save_breaking, incompatibility_notice, loader_save_format}`. Require the notice exactly when the release is save-breaking.
   Also emit `saves.save_format_version` ([#213](https://github.com/synpoox/resurrected-launcher/issues/213)): an integer >= 1 on every release, the version of the character-save format that release reads and writes. Start at 1 and raise it by one in the release that breaks existing saves (the same release that sets `save_breaking: true`); keep the value unchanged in later releases that load the same saves. The launcher compares the installed and target values, so an update that skips the breaking release still warns. Never lower it, and never omit it once a product has emitted it (a missing value is treated as compatible). Validation: `save_format_version >= 1`; it must be greater than the previous release's exactly when `save_breaking` is true.
   **Rollout gate: do NOT emit `saves.save_format_version` yet.** Every manifest object is `deny_unknown_fields`, so launchers released before this change (0.1.x, `schema_version` 3) reject any manifest carrying the key as `metadata_invalid` and players on them could no longer see or update that product. The schema version is not bumped (the field is optional), so there is no built-in fallback. Emit it only once the minimum supported launcher is the first release that parses it; until then, rely on `save_breaking` alone. Ship that launcher first, then raise the feed's minimum supported launcher, then start emitting.
9. Drop `d2rloader` and write no `loader` block. The launcher never downloads or runs the loader installer ([#59](https://github.com/synpoox/resurrected-launcher/issues/59)); a `loader` key is rejected as unknown.

## Validation before anything is written

The packager must refuse to produce or publish a manifest that the launcher would reject:

- No missing, zero, empty or all-zero sizes/hashes.
- Only `https://` URLs on public DNS host names, with no port, no IP literal and no dot segments.
- No absolute, drive, `..`, Windows-reserved, trailing dot/space or case-duplicate paths; nothing under a file.
- `archive.file_count`/`total_size` match `files`, and the 7z's actual entry list matches `files` + `empty_dirs` exactly.
- No command, script, hook or argument keys.
- The manifest is at most 8 MiB and the feed at most 1 MiB.
- `--incomplete` builds are never published to the recommended track.

### `validate-release`

Run the launcher's own validator on the output before publishing. From a checkout of this repo (Windows, Rust toolchain):

```text
cd src-tauri
cargo run --locked --example validate-release -- <release-manifest.json> <feed.json> [<archive.7z>]
```

It parses both documents with `ReleaseManifest::parse` and `Feed::parse`, and checks that the feed recommends this manifest with its exact `manifest_size` and `manifest_sha256`. Given the archive, it also checks `archive.size` and `archive.sha256` and lists the archive with the bundled 7-Zip (`src-tauri/resources/7zip/7za.exe`): the entries must be exactly `files` + `empty_dirs` under the one top-level `<folder>/`, with matching sizes. It prints `valid: ...` and exits 0, or prints one `invalid: ...` line per problem and exits 1. Wrong arguments exit 2.

## Feed

Add a feed file (`schema_version: 3`, see the spec) listing every product with `available` or `upcoming`. `available` products carry `recommended: {release_id, manifest_url, manifest_size, manifest_sha256}` for the published manifest bytes. Upcoming products carry no artifacts. The recommendation is chosen by the publisher, not derived from GitHub Latest or prerelease flags.

The launcher's default feed URL is `https://github.com/synpoox/resurrected-launcher/releases/download/catalog/feed.json` ([#62](https://github.com/synpoox/resurrected-launcher/issues/62), [#119](https://github.com/synpoox/resurrected-launcher/issues/119)): one feed for every product, an asset of the launcher repo's `catalog` release (which also holds the owner's `news.json`; leave it alone). Publishing replaces `feed.json` in place; do not attach `feed.json` to product releases. Each product release keeps its own archive, manifest and `SHA256SUMS`. `manifest_url` is the tag's durable asset, for example `https://github.com/synpoox/pd2-resurrected/releases/download/v0.1.0-alpha/release-manifest.json`. `fixtures/catalog/real/pd2r-0.1.0-alpha/` holds the generated pair for the published 0.1.0-alpha, which the packager's output should reproduce.

## Publication order

1. Upload the archive and the manifest as durable public release assets (not short-lived Actions artifacts). Never overwrite an asset of an existing `release_id`; corrected content gets a new `release_id`. The 0.1.0-alpha `SHA256SUMS` listed a `release-manifest.json` and `RELEASE_NOTES` that were never uploaded; list only uploaded assets.
2. Download them back and verify size and SHA-256 against what was generated; run `validate-release` on them.
3. Only then replace `feed.json` on the launcher repo's `catalog` release with the new `recommended` pointer, as the last step.

If publication stops at any point before step 3, the old feed still points at the old, complete release, so players keep a usable recommended track. The launcher rejects a known `release_id` whose pointer or manifest changed (`release_changed`), so republishing in place breaks updates rather than silently changing installed releases.

## Not established

No packager change has been made or run. The 0.1.0-alpha manifest and feed in `fixtures/catalog/real/` were generated launcher-side from the published archive, with D2R.exe 3.3.93847 fingerprinted from the owner's copy (#62). 7z reproducibility with the bundled 7-Zip is still to be proven before promotion.
