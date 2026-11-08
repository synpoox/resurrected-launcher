//! Preservation rules (#6 step 4): what an operation may do with each file it finds
//! in the live product root.
//!
//! * A shipped default still holding the previous release's published hash (the
//!   baseline) is untouched and is updated; any other content is an edited player
//!   file and stays.
//! * A managed path is replaced only when the previous release owned what is there;
//!   a file the launcher did not put there is an unknown-file conflict.
//! * A file is removed only when its bytes match the release that supplied it.
//! * Everything no manifest lists (saves, optional plugins, other additions) is
//!   never touched; plans only ever name release paths.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use super::download::hash_file;
use super::journal::{io_error, real_dir};
use crate::catalog::{Ownership, ReleaseManifest, Setting};
use crate::contract::{ExpectedFile, LauncherError};

/// Size and SHA-256 of a regular file.
pub(crate) type Digest = (u64, [u8; 32]);

/// What is at a path in the live root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OnDisk {
    Absent,
    File(Digest),
    /// A directory, link or other non-regular entry.
    Other,
}

pub(crate) fn on_disk(path: &Path) -> Result<OnDisk, LauncherError> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(OnDisk::Absent),
        Err(e) => Err(io_error("reading", path, e)),
        Ok(m) if m.is_file() => Ok(OnDisk::File(hash_file(path, &AtomicBool::new(false))?)),
        Ok(_) => Ok(OnDisk::Other),
    }
}

pub(crate) fn is(d: &Digest, f: &ExpectedFile) -> bool {
    d.0 == f.size && &d.1 == f.sha256.as_bytes()
}

/// What happens to one file of the new release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fate {
    /// Already the new bytes; left in place.
    Keep,
    /// Missing; the new file is moved in.
    Add,
    /// Owned by the previous release; moved to backup and replaced.
    Replace,
    /// An edited shipped default: a player file, kept.
    KeepEdited,
    /// Something the launcher did not put there occupies the path.
    Conflict,
}

/// The fate of new-release file `new` (`ownership` in the new release) given what
/// is on disk and the file at the same path in the baseline release, if any.
pub(crate) fn fate(
    disk: OnDisk,
    new: &ExpectedFile,
    ownership: Ownership,
    baseline: Option<(&ExpectedFile, Ownership)>,
) -> Fate {
    let d = match disk {
        OnDisk::Absent => return Fate::Add,
        OnDisk::Other => return Fate::Conflict,
        OnDisk::File(d) if is(&d, new) => return Fate::Keep,
        OnDisk::File(d) => d,
    };
    let untouched_baseline = baseline.is_some_and(|(f, _)| is(&d, f));
    match ownership {
        Ownership::ShippedDefault if untouched_baseline => Fate::Replace,
        Ownership::ShippedDefault => Fate::KeepEdited,
        // A damaged managed file of the previous release is still the release's.
        _ if matches!(baseline, Some((_, Ownership::Managed))) || untouched_baseline => {
            Fate::Replace
        }
        _ => Fate::Conflict,
    }
}

/// The value of `key` in a settings file, first match, trimmed.
///
/// * A plain key (`default_mod`) is INI-style: the first `key=value` line in any
///   section. Section headers and comment lines never match a key.
/// * A dotted key (`d2rcore.save_format.mode`) is TOML: the enclosing `[table]`
///   plus the line's key must spell it, and a quoted string comes back unquoted.
pub(crate) fn setting_value<'t>(text: &'t str, key: &str) -> Option<&'t str> {
    if !key.contains('.') {
        return text
            .lines()
            .filter_map(|l| l.split_once('='))
            .find(|(k, _)| k.trim() == key)
            .map(|(_, v)| v.trim());
    }
    // shortcut: no quoted keys, multi-line strings or escapes; PD2R's d2rloader.toml
    // uses none. Swap in a TOML parser if a release needs them.
    let dotted = |s: &str| s.split('.').map(str::trim).collect::<Vec<_>>().join(".");
    let mut table = String::new();
    for line in text.lines().map(str::trim) {
        if let Some(header) = line.strip_prefix('[') {
            table = dotted(header.split(']').next().unwrap_or_default());
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = dotted(k);
        let full = if table.is_empty() {
            k
        } else {
            format!("{table}.{k}")
        };
        if full == key {
            let v = v.trim();
            return Some(match v.chars().next() {
                Some(q @ ('"' | '\'')) => v[1..].split(q).next().unwrap_or_default(),
                _ => v.split('#').next().unwrap_or_default().trim(),
            });
        }
    }
    None
}

/// Settings the release needs: required settings and the loader save format.
pub(crate) fn mandatory_settings(m: &ReleaseManifest) -> impl Iterator<Item = &Setting> {
    m.compatibility
        .required_settings
        .iter()
        .chain(&m.saves.loader_save_format)
}

/// Every regular file under `dir` as (`/`-separated relative path, size). Links,
/// junctions and other reparse points are not followed or listed.
pub(crate) fn walk(dir: &Path) -> Result<Vec<(String, u64)>, LauncherError> {
    fn go(dir: &Path, prefix: &str, out: &mut Vec<(String, u64)>) -> Result<(), LauncherError> {
        for entry in fs::read_dir(dir).map_err(|e| io_error("reading", dir, e))? {
            let entry = entry.map_err(|e| io_error("reading", dir, e))?;
            let Ok(name) = entry.file_name().into_string() else {
                continue; // shortcut: non-Unicode names are left out of reports; they are never touched.
            };
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).map_err(|e| io_error("reading", &path, e))?;
            if real_dir(&meta) {
                go(&path, &rel, out)?;
            } else if meta.is_file() {
                out.push((rel, meta.len()));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    go(dir, "", &mut out)?;
    out.sort();
    Ok(out)
}

/// `base` joined with a `/`-separated relative path.
pub(crate) fn at(base: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(base.to_path_buf(), |p, c| p.join(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{RelativePath, Sha256Digest};

    fn file(bytes: &[u8]) -> (ExpectedFile, Digest) {
        use sha2::{Digest as _, Sha256};
        let h: [u8; 32] = Sha256::digest(bytes).into();
        let f = ExpectedFile {
            path: RelativePath::new("a.ini").unwrap(),
            size: bytes.len() as u64,
            sha256: Sha256Digest::from_bytes(h).unwrap(),
        };
        (f, (bytes.len() as u64, h))
    }

    #[test]
    fn shipped_default_updates_only_while_it_has_the_previous_published_hash() {
        let (old, old_d) = file(b"old default");
        let (new, _) = file(b"new default");
        let (_, edited) = file(b"edited");
        let base = Some((&old, Ownership::ShippedDefault));
        let sd = Ownership::ShippedDefault;
        assert_eq!(fate(OnDisk::File(old_d), &new, sd, base), Fate::Replace);
        assert_eq!(fate(OnDisk::File(edited), &new, sd, base), Fate::KeepEdited);
        assert_eq!(fate(OnDisk::File(edited), &new, sd, None), Fate::KeepEdited);
        assert_eq!(fate(OnDisk::Absent, &new, sd, base), Fate::Add);
    }

    #[test]
    fn managed_path_holding_an_unknown_file_is_a_conflict() {
        let (old, old_d) = file(b"old");
        let (new, new_d) = file(b"new");
        let (_, other) = file(b"player's own");
        let m = Ownership::Managed;
        assert_eq!(fate(OnDisk::File(new_d), &new, m, None), Fate::Keep);
        assert_eq!(fate(OnDisk::File(other), &new, m, None), Fate::Conflict);
        assert_eq!(
            fate(OnDisk::Other, &new, m, Some((&old, m))),
            Fate::Conflict
        );
        // Damaged managed file of the previous release: replaced.
        assert_eq!(
            fate(OnDisk::File(other), &new, m, Some((&old, m))),
            Fate::Replace
        );
        // An edited former shipped default is the player's, even if now managed.
        let sd = Ownership::ShippedDefault;
        assert_eq!(
            fate(OnDisk::File(other), &new, m, Some((&old, sd))),
            Fate::Conflict
        );
        assert_eq!(
            fate(OnDisk::File(old_d), &new, m, Some((&old, sd))),
            Fate::Replace
        );
    }

    #[test]
    fn setting_value_reads_ini_keys_not_comments_or_sections() {
        let text = "[loader]\n; default_mod=commented\n default_mod = PD2R \nsave_format=pd2r\n";
        assert_eq!(setting_value(text, "default_mod"), Some("PD2R"));
        assert_eq!(setting_value(text, "save_format"), Some("pd2r"));
        assert_eq!(setting_value(text, "loader"), None);
        assert_eq!(setting_value(text, "missing"), None);
    }

    #[test]
    fn setting_value_reads_dotted_keys_as_toml_table_plus_key() {
        let text = "# d2rcore.save_format.mode = \"commented\"\nmode = \"top\"\n\
                    [d2rcore.items]\nmode = \"items\"\n\
                    [ d2rcore . save_format ]\nmode = \"extended\" # the loader's\n\
                    [d2rcore]\nother.deep = 'single'\nbare = 42 # comment\n";
        assert_eq!(
            setting_value(text, "d2rcore.save_format.mode"),
            Some("extended")
        );
        assert_eq!(setting_value(text, "d2rcore.items.mode"), Some("items"));
        assert_eq!(setting_value(text, "d2rcore.other.deep"), Some("single"));
        assert_eq!(setting_value(text, "d2rcore.bare"), Some("42"));
        assert_eq!(setting_value(text, "d2rcore.mode"), None);
        assert_eq!(setting_value(text, "save_format.mode"), None);
        // A plain key keeps the INI rule: first match in any section, value as written.
        assert_eq!(setting_value(text, "mode"), Some("\"top\""));

        let real = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fixtures/products/pd2r-real-layout-1/payload/d2rloader/config/d2rloader.toml"
        ))
        .unwrap();
        assert_eq!(
            setting_value(&real, "d2rcore.save_format.mode"),
            Some("extended")
        );
    }
}
