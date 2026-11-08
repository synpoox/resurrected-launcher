# Release feed and release manifest, schema 3

Status: implemented by the Catalog module (`src-tauri/src/catalog/`, [#33](https://github.com/synpoox/resurrected-launcher/issues/33)). Semantics come from the [release manifest and safe-update contract](https://github.com/synpoox/resurrected-launcher/issues/6) and the [architecture resolution](https://github.com/synpoox/resurrected-launcher/issues/8); this page fixes the wire format. Fixtures: `fixtures/catalog/valid/` (a complete example of each document), `fixtures/catalog/invalid/` (one file per rejection class) and `fixtures/catalog/real/pd2r-0.1.0-alpha/` (the documents for the published PD2R 0.1.0-alpha, with a listing of its archive).

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
| `recommended.manifest_size`, `manifest_sha256` | Required, non-zero, not a placeholder. `manifest_size` is at most 8 MiB (`MAX_MANIFEST_SIZE`), checked before anything is fetched. The downloader verifies the manifest bytes against them. |

**HTTPS URLs** (every URL in both documents): `https://` followed by a public DNS host name, meaning at least one dot and only letters, digits, `-` and `.`. User info, explicit ports, IP literals, `localhost`/`*.localhost`, whitespace and control characters are rejected. The path may not contain a `.` or `..` segment or `%2e` in any case.

The recommendation is the publisher's choice. The launcher never infers it from GitHub Latest or prerelease flags.

**Location** ([#62](https://github.com/synpoox/resurrected-launcher/issues/62)): the default feed URL is `https://github.com/synpoox/resurrected-launcher/releases/download/catalog/feed.json` (`catalog::DEFAULT_FEED_URL`, [#119](https://github.com/synpoox/resurrected-launcher/issues/119)). One feed serves every product. It is mutable: the publisher replaces the `feed.json` asset of the launcher repo's `catalog` release in place, and GitHub redirects the download to its object storage. Each product release, in that product's own repository, carries its durable `release-manifest.json` (which the feed points at by its tag URL), its archive and its checksums; the `catalog` release holds only the feed. Never mark the `catalog` release as GitHub Latest: the launcher updater reads `releases/latest/download/latest.json` (and the news feed `releases/latest/download/news.json`) from this repository, so Latest must stay on the newest `launcher-v*` release.

**Refresh** (`catalog::refresh`, [#56](https://github.com/synpoox/resurrected-launcher/issues/56)) downloads the feed (at most 1 MiB, `MAX_FEED_SIZE`), then each recommended manifest with its exact size and SHA-256, validates everything, and only then caches it in State: manifests as `manifest/<product_id>/<release_id>`, then the feed as `feed`. It is all or nothing. Offline, on any failed download or invalid document, or on `release_changed`, the cache keeps the last valid metadata.

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
| `release_notes` | Non-empty text, shown as Patch Notes. Inline so they work offline. A Markdown subset: ATX headings (the shallowest level used starts a section, deeper ones are sub-headings), `-`/`*`/`+` and numbered lists nested by indent (up to three levels), `>` quotes, paragraphs, fenced code, `**strong**`, `*em*`/`_em_` and `` `code` ``. An opening paragraph is the summary. HTML is shown as text; links and images show only their text. Patch Notes parse at most the first 100,000 characters (cut at a line break) and then say the notes were too long to show in full. |
| `min_launcher_version` | `MAJOR.MINOR.PATCH`. Newer than this launcher: a launcher update is required. |

### Payload and installation

| Field | Rule |
| --- | --- |
| `install_root` | `mods/<folder>`, relative to the selected D2R installation, for example `mods/PD2Resurrected`. The folder may not start with `-`, so `-mod <folder>` cannot read as a loader switch. No absolute or publisher-chosen destinations. Launch selects the product only as `-mod <folder>`, so no `default_mod` loader setting is required. The schema-2 `launch.launch_arguments` are dropped on purpose: publisher-supplied arguments are not allowed. |
| `archive` | `{url, format, size, sha256, file_count, total_size}`. One complete archive; repairs and updates download all of it, no deltas. `url` is HTTPS; `format` is `7z`; `size` and `sha256` are required and real. `file_count` and `total_size` must equal the count and size sum of `files`, and a sum that overflows 64 bits is inconsistent. |
| `files` | Every file the archive unpacks into `install_root`: `{path, size, sha256, ownership}`. Must not be empty. |
| `empty_dirs` | Directories that must exist although they hold no file. |

Paths use `/`, are relative to `install_root`, and must pass the contract's `RelativePath` rules (no absolute, drive, `..`, `.`, stream, device name, trailing dot/space, forbidden character or 8.3 alias). The list must have no duplicates ignoring case and nothing may lie under a file or an empty dir (`ExpectedInventory`). Extraction later checks the unpacked tree against exactly this inventory.

**Archive layout.** The published archive holds one top-level folder, the `install_root` folder (`PD2Resurrected/`), so a player can unpack it into `mods/`; `files` and `empty_dirs` are relative to that folder. One check, `ExpectedInventory::listing_mismatches`, compares a 7-Zip listing with this layout; `ReleaseManifest::archive_mismatches` (used by the `validate-release` tool) and launcher-side extraction (`install::extract`) both call it. Extraction refuses anything else at the top level (a stray entry, a second folder, or the tree at the archive root), then strips the folder so staging mirrors the product root, and checks the unpacked tree against the inventory again ([#68](https://github.com/synpoox/resurrected-launcher/issues/68)).

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
| `compatibility.required_settings` | `{file, key, value}` settings the product needs. `file` must be a `shipped_default` of this release, compared ignoring case as Windows does. A plain `key` is read INI-style (the first `key=value` line in any section, value as written). A dotted `key` such as `d2rcore.save_format.mode` is read as TOML: the enclosing `[table]` plus the line's key must spell it, and a quoted string value is compared unquoted. An edited conflicting value pauses the update instead of being overwritten. |

Unknown game/loader combinations block Play.

### Saves

| Field | Rule |
| --- | --- |
| `saves.folder` | One folder name under D2R's per-user mod save directory. Saves are never part of the payload. |
| `saves.save_breaking` | Whether characters from the previous release may not load (ADR 0001). |
| `saves.incompatibility_notice` | Non-empty text exactly when `save_breaking` is true, otherwise `null`. Shown and acknowledged before updating. |
| `saves.loader_save_format` | Optional `{file, key, value}`, same rules as a required setting. |

### Loader

There is no loader installer pin: the launcher never downloads or runs the D2RLoader installer ([#59](https://github.com/synpoox/resurrected-launcher/issues/59)). It reads the installed loader's version, checks it against `compatibility.loader_versions`, and when it is missing or unsupported links the player to the official download page. A manifest that still carries the earlier `loader` block is rejected as an unknown field.

## Immutability

A release ID and its bytes never change. Refresh compares new metadata with what it cached for the same `release_id` (State, #34): a feed pointer with a different manifest size/hash, or a cached manifest whose bytes differ from the new pointer, is `release_changed`. The new metadata is not used and the cache is kept. Corrected content needs a new `release_id`.

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
| Upcoming product with a release | `UpcomingHasArtifacts` | `metadata_invalid` |
| Manifest is not the release the feed recommends | `ManifestMismatch` | `metadata_invalid` |
| Same release ID, different metadata | `ReleaseChanged` | `release_changed` |
| Any other unusable field | `InvalidField` | `metadata_invalid` |

## Not covered here

- Byte verification of downloaded manifests and archives, HTTPS and the public-host rule above on every redirect hop, and the extracted-tree check (download/extract, #35).
- The cache files themselves and their integrity checks (State, #34). Refresh fills them (#56).
- Publisher signatures on game-release metadata: deferred by the owner (#6). HTTPS and hashes detect mismatches, not a malicious publisher.
- Open question: if D2RLoader reads a required or save-format setting from a file outside the product's `install_root`, the setting needs a scope field in a later schema.
