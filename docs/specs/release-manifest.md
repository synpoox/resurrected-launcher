# Release feed and release manifest, schema 3

Status: implemented by the Catalog module (`src-tauri/src/catalog/`, [#33](https://github.com/synpoox/resurrected-launcher/issues/33)). Semantics come from the [release manifest and safe-update contract](https://github.com/synpoox/resurrected-launcher/issues/6) and the [architecture resolution](https://github.com/synpoox/resurrected-launcher/issues/8); this page fixes the wire format. Fixtures: `fixtures/catalog/valid/` (a complete example of each document) and `fixtures/catalog/invalid/` (one file per rejection class).

Both documents are UTF-8 JSON with snake_case keys. Every object rejects unknown and duplicate keys. Metadata is declarative: no commands, scripts, hooks or launch arguments. The boundary that enforces this is the rejection of unknown keys. A key containing `command`, `cmd`, `script`, `execute`, `shell`, `hook` or `argument` (case-insensitive, at any depth) is reported as `ExecutableMetadata` instead of `Malformed`; that list only selects the error, it is not the guard. Schema 3 is new; the toolkit packager's current `schemaVersion: 2` output is rejected, not read as complete.

## Release feed

A public static file, initially a GitHub release asset. It says which products exist and which release the publisher recommends. Display names and descriptions are bundled with the launcher, not in the feed.

```json
{
  "schema_version": 3,
  "products": [
    { "product_id": "pd2r", "availability": "available",
      "recommended": { "release_id": "pd2r-0.4.0-alpha.1",
                       "manifest_url": "https://.../release-manifest.json",
                       "manifest_size": 3072, "manifest_sha256": "<64 hex>" } },
    { "product_id": "project-09", "availability": "upcoming" }
  ]
}
```

| Field | Rule |
| --- | --- |
| `product_id` | Stable machine ID: 1-64 of `a-z 0-9 . _ -`, starting with a letter or digit. Unique in the feed. Never a display name. |
| `availability` | `available` requires `recommended`. `upcoming` must not carry `recommended` (no artifacts). |
| `recommended.release_id` | Immutable release identity, same character rules as `product_id`. |
| `recommended.manifest_url` | An HTTPS URL as defined below. |
| `recommended.manifest_size`, `manifest_sha256` | Required, non-zero, not a placeholder. The downloader verifies the manifest bytes against them. |

**HTTPS URLs** (every URL in both documents): `https://` followed by a public DNS host name, meaning at least one dot and only letters, digits, `-` and `.`. User info, explicit ports, IP literals, `localhost`/`*.localhost`, whitespace and control characters are rejected. The path may not contain a `.` or `..` segment or `%2e` in any case.

The recommendation is the publisher's choice. The launcher never infers it from GitHub Latest or prerelease flags.

## Release manifest

One per release, published as a durable public asset next to the archive. See `fixtures/catalog/valid/release-manifest.json` for a complete example.

### Identity

| Field | Rule |
| --- | --- |
| `schema_version` | `3`. Higher: "a launcher update is required" (`launcher_update_required`). Lower or `schemaVersion` (schema 2): `metadata_invalid`. |
| `product_id`, `release_id` | As in the feed. The manifest must match the feed entry that pointed to it. |
| `display_version` | Non-empty text shown to players. |
| `release_date` | `YYYY-MM-DD`. |
| `status` | `alpha`, `beta` or `release`. |
| `release_notes` | Non-empty text, shown as Patch Notes. Inline so they work offline. |
| `min_launcher_version` | `MAJOR.MINOR.PATCH`. Newer than this launcher: a launcher update is required. |

### Payload and installation

| Field | Rule |
| --- | --- |
| `install_root` | `mods/<folder>`, relative to the selected D2R installation. No absolute or publisher-chosen destinations. Launch selects the product only as `-mod <folder>`. The schema-2 `launch.launch_arguments` are dropped on purpose: publisher-supplied arguments are not allowed. |
| `archive` | `{url, format, size, sha256, file_count, total_size}`. One complete archive; repairs and updates download all of it, no deltas. `url` is HTTPS; `format` is `7z`; `size` and `sha256` are required and real. `file_count` and `total_size` must equal the count and size sum of `files`, and a sum that overflows 64 bits is inconsistent. |
| `files` | Every file the archive unpacks into `install_root`: `{path, size, sha256, ownership}`. Must not be empty. |
| `empty_dirs` | Directories that must exist although they hold no file. |

Paths use `/`, are relative to `install_root`, and must pass the contract's `RelativePath` rules (no absolute, drive, `..`, `.`, stream, device name, trailing dot/space, forbidden character or 8.3 alias). The list must have no duplicates ignoring case and nothing may lie under a file or an empty dir (`ExpectedInventory`). Extraction later checks the unpacked tree against exactly this inventory.

A file `size` and `sha256` must always be present. `size: 0` is allowed only with the SHA-256 of empty input (`e3b0c442...b855`), and that hash only with `size: 0`; any other combination is a placeholder.

### Ownership

| `ownership` | Meaning |
| --- | --- |
| `managed` | Supplied by the release. Repair restores it; update replaces it. Required mod plugins and patches are managed and recognized by path and hash. |
| `shipped_default` | Initial configuration, including the loot filter and loader configuration. Update replaces it only while it still has the previous release's published hash; once edited it is a player file. |

Anything on disk that the manifest does not list is unknown (player files, optional plugins) and is never managed implicitly. `unknown` is not a valid value in the manifest.

### Compatibility

| Field | Rule |
| --- | --- |
| `compatibility.game_builds` | Non-empty. Each `{build, platform, executable, size, sha256}`: `platform` is `windows_x64`; `executable` is relative to the D2R installation; size and hash are real fingerprints of that file. A marketing label alone is not enough. |
| `compatibility.loader_versions` | Non-empty list of allowed loader versions: `MAJOR.MINOR.PATCH` with an optional `-prerelease` (`1.3.1-beta`), compared as exact strings. |
| `compatibility.required_settings` | `{file, key, value}` settings the product needs. `file` must be a `shipped_default` of this release, compared ignoring case as Windows does. An edited conflicting value pauses the update instead of being overwritten. |

Unknown game/loader combinations block Play.

### Saves

| Field | Rule |
| --- | --- |
| `saves.folder` | One folder name under D2R's per-user mod save directory. Saves are never part of the payload. |
| `saves.save_breaking` | Whether characters from the previous release may not load (ADR 0001). |
| `saves.incompatibility_notice` | Non-empty text exactly when `save_breaking` is true, otherwise `null`. Shown and acknowledged before updating. |
| `saves.loader_save_format` | Optional `{file, key, value}`, same rules as a required setting. |

### Loader pin

`loader: {installer_version, installs_loader_version, url, size, sha256, signed}`. The installer and the loader it installs are versioned separately.

- `installer_version` is the exact upstream release tag, such as `v0.1.0`.
- `url` must be exactly `https://github.com/D2RLoader/Installer/releases/download/<installer_version>/<file>`, where `<file>` uses only `A-Z a-z 0-9 . _ + -` and does not start with `.`. No extra segments, percent-encoding, query or fragment are allowed, so the official prefix cannot be escaped. The installer is never rehosted or bundled.
- `installs_loader_version` is the loader version that installer installs. It must be in `compatibility.loader_versions`.

 `size` and `sha256` are real values of the upstream file. `signed` states whether that installer is Authenticode-signed and is disclosed to the player when false.

## Immutability

A release ID and its bytes never change. The launcher compares new metadata with what it cached for the same `release_id` (State, #34): a feed pointer with different manifest size/hash, or a manifest with any different field, is `release_changed` and is not used. Corrected content needs a new `release_id`.

## Rejection classes

Each maps to a `CatalogError` variant and, over IPC, to an `ErrorCode`.

| Class | `CatalogError` | `ErrorCode` |
| --- | --- | --- |
| Not JSON, wrong type, missing non-verification field, unknown field | `Malformed` | `metadata_invalid` |
| Newer schema / too-new `min_launcher_version` | `UnsupportedSchema` / `LauncherTooOld` | `launcher_update_required` |
| Older schema | `UnsupportedSchema` | `metadata_invalid` |
| Command, script, hook or argument key | `ExecutableMetadata` | `metadata_invalid` |
| Missing, null, zero or placeholder size/hash | `PlaceholderVerification` | `metadata_invalid` |
| Non-HTTPS URL | `NotHttps` | `metadata_invalid` |
| Absolute, escaping or Windows-unsafe path | `UnsafePath` | `metadata_invalid` |
| Duplicate or Windows-conflicting paths | `Inventory` | `metadata_invalid` |
| Archive counts disagree with the file list | `ArchiveInconsistent` | `metadata_invalid` |
| Loader installer not from the official source | `LoaderSourceNotOfficial` | `metadata_invalid` |
| Upcoming product with a release | `UpcomingHasArtifacts` | `metadata_invalid` |
| Manifest is not the release the feed recommends | `ManifestMismatch` | `metadata_invalid` |
| Same release ID, different metadata | `ReleaseChanged` | `release_changed` |
| Any other unusable field | `InvalidField` | `metadata_invalid` |

## Not covered here

- Byte verification of downloaded manifests and archives, HTTPS enforcement on redirects, and the extracted-tree check (download/extract, #35).
- Caching validated metadata and offline reuse (State, #34).
- Publisher signatures on game-release metadata: deferred by the owner (#6). HTTPS and hashes detect mismatches, not a malicious publisher.
- Open question: if D2RLoader reads a required or save-format setting from a file outside the product's `install_root`, the setting needs a scope field in a later schema.
