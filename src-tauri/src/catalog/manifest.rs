//! Release manifest, schema 3 (`docs/specs/release-manifest.md`).

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::contract::{ExpectedFile, ExpectedInventory, ProductId, RelativePath, Sha256Digest};

use super::check::{self, each};
use super::CatalogError;

/// SHA-256 of zero bytes: the only valid hash of an empty file, and a placeholder
/// for any non-empty one.
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseStatus {
    Alpha,
    Beta,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ArchiveFormat {
    #[serde(rename = "7z")]
    SevenZip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    WindowsX64,
}

/// Who owns a file in the product root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    /// Supplied by the release; repair restores it, update replaces it.
    Managed,
    /// Initial configuration (loot filter, loader config). Becomes a player file once
    /// edited: update replaces it only while it still has the previous release's hash.
    ShippedDefault,
    /// Not in the manifest (player files, optional plugins). Never touched implicitly.
    Unknown,
}

/// The complete product archive. Repair and update download all of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archive {
    pub url: String,
    pub format: ArchiveFormat,
    pub size: u64,
    pub sha256: Sha256Digest,
}

/// A verified game build, identified by its executable's bytes, not a marketing label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameBuild {
    pub build: String,
    pub platform: Platform,
    /// Relative to the D2R installation root.
    pub executable: RelativePath,
    pub size: u64,
    pub sha256: Sha256Digest,
}

/// A `key = value` the product needs, in a shipped-default file of this release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    /// Relative to the install root; always a [`Ownership::ShippedDefault`] file.
    pub file: RelativePath,
    pub key: String,
    pub value: String,
}

/// Game builds and loader versions this release is verified with. Anything else
/// blocks Play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compatibility {
    pub game_builds: Vec<GameBuild>,
    pub loader_versions: Vec<String>,
    pub required_settings: Vec<Setting>,
}

/// Saves are never part of the payload; this only says where they are and whether
/// the release breaks them (ADR 0001).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saves {
    /// One folder name under D2R's per-user mod save directory.
    pub folder: RelativePath,
    /// Present exactly when the release is save-breaking; shown before updating.
    pub incompatibility_notice: Option<String>,
    pub loader_save_format: Option<Setting>,
}

/// A validated release manifest. Exists only if every rule in the spec held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseManifest {
    pub product_id: ProductId,
    pub release_id: String,
    pub display_version: String,
    pub release_date: String,
    pub status: ReleaseStatus,
    /// Shown as Patch Notes.
    pub release_notes: String,
    pub min_launcher_version: String,
    /// `mods/<folder>`, relative to the D2R installation root.
    pub install_root: RelativePath,
    pub archive: Archive,
    /// Paths relative to `install_root`.
    pub inventory: ExpectedInventory,
    shipped_defaults: Vec<RelativePath>,
    /// Upper-cased path -> `previous_default_sha256` of that shipped default.
    previous_defaults: BTreeMap<String, Vec<Sha256Digest>>,
    pub compatibility: Compatibility,
    pub saves: Saves,
}

impl ReleaseManifest {
    /// Parses and validates manifest bytes for this launcher version.
    pub fn parse(bytes: &[u8]) -> Result<Self, CatalogError> {
        check::parse::<RawManifest>(bytes)?.validate(env!("CARGO_PKG_VERSION"))
    }

    /// The mod folder name the game is launched with (`-mod <folder>`).
    pub fn mod_folder(&self) -> &str {
        self.install_root
            .as_str()
            .split('/')
            .nth(1)
            .unwrap_or_default()
    }

    pub fn is_save_breaking(&self) -> bool {
        self.saves.incompatibility_notice.is_some()
    }

    /// Ownership of a path relative to `install_root`, compared as Windows does.
    pub fn ownership_of(&self, path: &RelativePath) -> Ownership {
        let same = |p: &RelativePath| p.as_str().to_uppercase() == path.as_str().to_uppercase();
        if self.shipped_defaults.iter().any(same) {
            Ownership::ShippedDefault
        } else if self.inventory.files().iter().any(|f| same(&f.path)) {
            Ownership::Managed
        } else {
            Ownership::Unknown
        }
    }

    /// The SHA-256 values earlier releases published for the shipped default at
    /// `path` (`previous_default_sha256`), compared ignoring case as Windows does.
    /// Bytes with one of them are an older release's untouched default, not a
    /// player edit (#134). Empty for managed and unlisted paths.
    pub fn previous_default_hashes(&self, path: &RelativePath) -> &[Sha256Digest] {
        self.previous_defaults
            .get(&path.as_str().to_uppercase())
            .map_or(&[], Vec::as_slice)
    }

    /// How a 7-Zip listing of the release archive differs from `files` and
    /// `empty_dirs`, under one top-level [`Self::mod_folder`] directory as published
    /// for players to unpack into `mods/`; empty when it matches. See
    /// [`ExpectedInventory::listing_mismatches`], which extraction uses too.
    pub fn archive_mismatches(&self, entries: &[(&str, Option<u64>)]) -> Vec<String> {
        self.inventory
            .listing_mismatches(self.mod_folder(), entries)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// Immutability: a release ID seen before must have exactly the same metadata.
    pub fn check_unchanged(&self, cached: &ReleaseManifest) -> Result<(), CatalogError> {
        if self.release_id == cached.release_id && self != cached {
            return Err(CatalogError::ReleaseChanged {
                release_id: self.release_id.clone(),
            });
        }
        Ok(())
    }
}

// Raw wire shapes. Verification fields are `Option` so a missing one is reported as a
// placeholder rather than a generic parse error.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    #[allow(dead_code)] // checked by check::parse before this struct is built
    schema_version: u32,
    product_id: String,
    release_id: String,
    display_version: String,
    release_date: String,
    status: ReleaseStatus,
    release_notes: String,
    min_launcher_version: String,
    install_root: String,
    archive: RawArchive,
    files: Vec<RawFile>,
    empty_dirs: Vec<String>,
    compatibility: RawCompatibility,
    saves: RawSaves,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawArchive {
    url: String,
    format: ArchiveFormat,
    size: Option<u64>,
    sha256: Option<String>,
    file_count: u64,
    total_size: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    path: String,
    size: Option<u64>,
    sha256: Option<String>,
    ownership: Ownership,
    /// Optional: hashes earlier releases shipped for this `shipped_default`.
    #[serde(default)]
    previous_default_sha256: Option<Vec<Option<String>>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCompatibility {
    game_builds: Vec<RawGameBuild>,
    loader_versions: Vec<String>,
    required_settings: Vec<RawSetting>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGameBuild {
    build: String,
    platform: Platform,
    executable: String,
    size: Option<u64>,
    sha256: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSetting {
    file: String,
    key: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSaves {
    folder: String,
    save_breaking: bool,
    incompatibility_notice: Option<String>,
    loader_save_format: Option<RawSetting>,
}

fn invalid(field: impl Into<String>, reason: &'static str) -> CatalogError {
    CatalogError::InvalidField {
        field: field.into(),
        reason,
    }
}

impl RawManifest {
    fn validate(self, launcher_version: &str) -> Result<ReleaseManifest, CatalogError> {
        let min = check::version("min_launcher_version", &self.min_launcher_version)?;
        let current = check::version("launcher version", launcher_version)?;
        if min > current {
            return Err(CatalogError::LauncherTooOld {
                required: self.min_launcher_version,
            });
        }

        let install_root = check::path("install_root", self.install_root)?;
        match install_root.as_str().split_once('/') {
            Some((mods, folder)) if mods.eq_ignore_ascii_case("mods") && !folder.contains('/') => {
                // Launch passes it as `-mod <folder>`; a leading `-` would read as a switch.
                if folder.starts_with('-') {
                    return Err(invalid("install_root", "folder must not start with -"));
                }
            }
            _ => return Err(invalid("install_root", "must be mods/<folder>")),
        }

        if self.files.is_empty() {
            return Err(invalid("files", "must not be empty"));
        }
        let mut shipped_defaults = Vec::new();
        let mut previous_defaults = BTreeMap::new();
        let files = each("files", self.files, |field, f| {
            let file = ExpectedFile {
                path: check::path(&format!("{field}.path"), f.path)?,
                size: f
                    .size
                    .ok_or_else(|| CatalogError::PlaceholderVerification {
                        field: format!("{field}.size"),
                    })?,
                sha256: check::digest(&format!("{field}.sha256"), f.sha256)?,
            };
            // Size and hash must agree on emptiness, or one of them is a placeholder.
            if (file.size == 0) != (file.sha256.to_string() == EMPTY_SHA256) {
                let which = if file.size == 0 { "size" } else { "sha256" };
                return Err(CatalogError::PlaceholderVerification {
                    field: format!("{field}.{which}"),
                });
            }
            if let Some(prev) = f.previous_default_sha256 {
                if f.ownership != Ownership::ShippedDefault {
                    return Err(invalid(
                        format!("{field}.previous_default_sha256"),
                        "only a shipped_default lists previous hashes",
                    ));
                }
                let prev = each(&format!("{field}.previous_default_sha256"), prev, |f, h| {
                    check::digest(f, h)
                })?;
                previous_defaults.insert(file.path.as_str().to_uppercase(), prev);
            }
            match f.ownership {
                Ownership::Managed => {}
                Ownership::ShippedDefault => shipped_defaults.push(file.path.clone()),
                Ownership::Unknown => {
                    return Err(invalid(
                        format!("{field}.ownership"),
                        "listed files are managed or shipped_default",
                    ))
                }
            }
            Ok(file)
        })?;
        let empty_dirs = each("empty_dirs", self.empty_dirs, check::path)?;
        let inventory =
            ExpectedInventory::new(files, empty_dirs).map_err(CatalogError::Inventory)?;
        let files = inventory.files();

        let raw = self.archive;
        if raw.file_count != files.len() as u64 {
            return Err(CatalogError::ArchiveInconsistent {
                field: "archive.file_count".to_owned(),
            });
        }
        // checked: a wrapping sum would let a crafted total_size pass.
        let sum = files
            .iter()
            .try_fold(0u64, |acc, f| acc.checked_add(f.size));
        if sum != Some(raw.total_size) {
            return Err(CatalogError::ArchiveInconsistent {
                field: "archive.total_size".to_owned(),
            });
        }
        let archive = Archive {
            url: check::https("archive.url", raw.url)?,
            format: raw.format,
            size: check::size("archive.size", raw.size)?,
            sha256: check::digest("archive.sha256", raw.sha256)?,
        };

        let setting = |field: &str, s: RawSetting| -> Result<Setting, CatalogError> {
            let file = check::path(&format!("{field}.file"), s.file)?;
            let key = file.as_str().to_uppercase();
            if !shipped_defaults
                .iter()
                .any(|d: &RelativePath| d.as_str().to_uppercase() == key)
            {
                return Err(invalid(
                    format!("{field}.file"),
                    "must be a shipped_default file of this release",
                ));
            }
            Ok(Setting {
                file,
                key: check::text(&format!("{field}.key"), s.key)?,
                value: s.value,
            })
        };

        let c = self.compatibility;
        if c.game_builds.is_empty() {
            return Err(invalid("compatibility.game_builds", "must not be empty"));
        }
        if c.loader_versions.is_empty() {
            return Err(invalid(
                "compatibility.loader_versions",
                "must not be empty",
            ));
        }
        let compatibility = Compatibility {
            game_builds: each("compatibility.game_builds", c.game_builds, |field, g| {
                Ok(GameBuild {
                    build: check::text(&format!("{field}.build"), g.build)?,
                    platform: g.platform,
                    executable: check::path(&format!("{field}.executable"), g.executable)?,
                    size: check::size(&format!("{field}.size"), g.size)?,
                    sha256: check::digest(&format!("{field}.sha256"), g.sha256)?,
                })
            })?,
            loader_versions: each(
                "compatibility.loader_versions",
                c.loader_versions,
                |f, v| {
                    check::semver(f, &v)?;
                    Ok(v)
                },
            )?,
            required_settings: each(
                "compatibility.required_settings",
                c.required_settings,
                setting,
            )?,
        };

        let s = self.saves;
        let folder = check::path("saves.folder", s.folder)?;
        if folder.as_str().contains('/') {
            return Err(invalid("saves.folder", "must be one folder name"));
        }
        let incompatibility_notice = match (s.save_breaking, s.incompatibility_notice) {
            (true, Some(n)) => Some(check::text("saves.incompatibility_notice", n)?),
            (false, None) => None,
            _ => {
                return Err(invalid(
                    "saves.incompatibility_notice",
                    "required exactly when save_breaking is true",
                ))
            }
        };
        let saves = Saves {
            folder,
            incompatibility_notice,
            loader_save_format: s
                .loader_save_format
                .map(|l| setting("saves.loader_save_format", l))
                .transpose()?,
        };

        Ok(ReleaseManifest {
            product_id: check::product_id("product_id", self.product_id)?,
            release_id: check::ident("release_id", self.release_id)?,
            display_version: check::text("display_version", self.display_version)?,
            release_date: check::date("release_date", self.release_date)?,
            status: self.status,
            release_notes: check::text("release_notes", self.release_notes)?,
            min_launcher_version: self.min_launcher_version,
            install_root,
            archive,
            inventory,
            shipped_defaults,
            previous_defaults,
            compatibility,
            saves,
        })
    }
}
