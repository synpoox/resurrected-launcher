# Handoff: d2rAI-toolkit packager changes for schema 3

For the maintainers of [d2rAI-toolkit](https://github.com/synpoox/d2rAI-toolkit) (`release/package_release.py`, `release/release_config.py` and the publish step). This repo does not edit the toolkit; this note lists what its output must become so the launcher can read it. Wire format: [release-manifest.md](../specs/release-manifest.md). Example output: `fixtures/catalog/valid/`. Decision: [#6](https://github.com/synpoox/resurrected-launcher/issues/6), slice 1 of [#8](https://github.com/synpoox/resurrected-launcher/issues/8), implemented launcher-side by [#33](https://github.com/synpoox/resurrected-launcher/issues/33).

Extend the existing packager; do not add a second pipeline.

## Manifest output (schema 3)

Compared with today's `schemaVersion: 2` output (reviewed at [707f9df](https://github.com/synpoox/d2rAI-toolkit/blob/707f9df317021dad885ddd44329588742723d259/release/package_release.py)):

1. Write `schema_version: 3` and switch all keys to snake_case (`emptyDirs` -> `empty_dirs`, `launcherMin` -> `min_launcher_version`, ...). Unknown keys are rejected, so drop `name`, `modFolder`, `complete`, `pluginVersion` and `fileCount`/`totalSize` at top level.
2. Identity: add `product_id` (stable, e.g. `pd2r`) and `release_id` (immutable, unique per published bytes, e.g. `pd2r-0.4.0-alpha.1`), `display_version`, `release_date` (`released`), `status` (`alpha`/`beta`/`release`), inline `release_notes` text, and a required `min_launcher_version`.
3. Archive: `archive.url` is the final HTTPS download URL; add `file_count` and `total_size` inside `archive`, computed from `files`. Keep `format: "7z"`, `size` and `sha256` generated from the actual output.
4. Files: add `ownership` to every entry: `managed`, or `shipped_default` for files listed in `package.preserve` (loot filter, loader configuration). Keep sizes and hashes generated from the actual staged files. Keep `empty_dirs`.
5. Replace `modFolder` with `install_root: "mods/<folder>"`.
6. Replace `d2r` with `compatibility.game_builds`: `{build, platform: "windows_x64", executable, size, sha256}` per verified D2R build, from the real executable. A version label alone is rejected.
7. Add `compatibility.loader_versions` and move the loader settings from `launch` into `compatibility.required_settings` as `{file, key, value}` pointing at a `shipped_default` file. Remove `launch` and any launch arguments: metadata may not carry commands or arguments.
8. Replace `saveFolder` with `saves: {folder, save_breaking, incompatibility_notice, loader_save_format}`. Require the notice exactly when the release is save-breaking.
9. Replace `d2rloader` with `loader: {installer_version, installs_loader_version, url, size, sha256, signed}`.
   - `installer_version` is the upstream installer's release tag, such as `v0.1.0`.
   - `installs_loader_version` is the loader version that installer installs, such as `1.3.1-beta`. It must be in `compatibility.loader_versions`. Today's pin mixes the two versions.
   - `url` is exactly `https://github.com/D2RLoader/Installer/releases/download/<installer_version>/<file>`.
   Fill size and hash from the real upstream file and record whether it is signed. Never rehost or bundle it.

## Validation before anything is written

The packager must refuse to produce or publish a manifest that the launcher would reject:

- No missing, zero, empty or all-zero sizes/hashes (today's loader pin has zero placeholders; those must go).
- Only `https://` URLs on public DNS host names, with no port, no IP literal and no dot segments.
- No absolute, drive, `..`, Windows-reserved, trailing dot/space or case-duplicate paths; nothing under a file.
- `archive.file_count`/`total_size` match `files`, and the 7z's actual entry list matches `files` + `empty_dirs` exactly.
- No command, script, hook or argument keys.
- `--incomplete` builds are never published to the recommended track.

Running the launcher's validator against the output is the simplest check: `ReleaseManifest::parse` and `Feed::parse` in `src-tauri/src/catalog/`.

## Feed

Add a feed file (`schema_version: 3`, see the spec) listing every product with `available` or `upcoming`. `available` products carry `recommended: {release_id, manifest_url, manifest_size, manifest_sha256}` for the published manifest bytes. Upcoming products carry no artifacts. The recommendation is chosen by the publisher, not derived from GitHub Latest or prerelease flags.

## Publication order

1. Upload the archive and the manifest as durable public release assets (not short-lived Actions artifacts). Never overwrite an asset of an existing `release_id`; corrected content gets a new `release_id`.
2. Download them back and verify size and SHA-256 against what was generated; validate the manifest.
3. Only then update the feed's `recommended` pointer, as the last step.

If publication stops at any point before step 3, the old feed still points at the old, complete release, so players keep a usable recommended track. The launcher rejects a known `release_id` whose pointer or manifest changed (`release_changed`), so republishing in place breaks updates rather than silently changing installed releases.

## Not established

No packager change has been made or run. The real D2R executable fingerprints, the loader installer's size/hash/signature status and the 7z reproducibility with the bundled 7-Zip are still to be populated and proven before promotion.
