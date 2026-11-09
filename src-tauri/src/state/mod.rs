//! State/diagnostics module: versioned state files with atomic replacement for
//! preferences, cached validated metadata and installed-release records, plus local
//! redacted diagnostics. No database. Transaction journals stay owned by Install.
//!
//! Every state file is `{"schema_version": N, "data": ...}` with its own schema
//! version (not [`crate::contract::CONTRACT_VERSION`], so an IPC contract bump does
//! not discard a player's preferences). A file that is corrupt, fails validation or
//! has another schema version is renamed aside and reported in
//! [`StateStore::issues`]; the store then starts from the empty/default value, so
//! startup never crashes and never reports a release as installed that it cannot
//! read back.

mod diagnostics;
mod file;
pub mod log;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::contract::{ErrorCode, LauncherError, ProductId, Sha256Digest};

pub use diagnostics::{DiagnosticsContext, DiagnosticsReport, Redactor, NEVER_INCLUDED};
pub use file::{StateIssue, StateProblem};

const PREFERENCES_FILE: &str = "preferences.json";
const PREFERENCES_SCHEMA: u32 = 1;
const INSTALLED_FILE: &str = "installed-releases.json";
const INSTALLED_SCHEMA: u32 = 1;
const CACHE_INDEX_FILE: &str = "metadata-cache.json";
const CACHE_INDEX_SCHEMA: u32 = 1;
const CACHE_DIR: &str = "metadata-cache";

/// Player preferences (Settings, plus the remembered product).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    /// The selected D2R folder. Game validates that it holds D2R; State only
    /// requires an absolute path.
    pub d2r_location: Option<PathBuf>,
    /// Off by default (launcher-ui.md, Settings → Launcher).
    pub start_with_windows: bool,
    /// On by default (#28).
    pub download_launcher_updates_automatically: bool,
    /// The window opens on this product, or PD2R when unset.
    pub last_product: Option<ProductId>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            d2r_location: None,
            start_with_windows: false,
            download_launcher_updates_automatically: true,
            last_product: None,
        }
    }
}

fn check_d2r_location(p: &Path) -> Result<(), String> {
    let s = p.to_string_lossy();
    // Allow `\\?\X:\` and `\\?\UNC\`; refuse the device namespace (`\\.\`, `\\?\GLOBALROOT`, ...).
    let device = s.starts_with(r"\\.\")
        || s.starts_with("//./")
        || s.starts_with("//?/")
        || (s.starts_with(r"\\?\") && {
            let rest = s[4..].as_bytes();
            let drive = rest.len() >= 3 && rest[0].is_ascii_alphabetic() && &rest[1..3] == br":\";
            !(drive || s[4..].to_ascii_uppercase().starts_with(r"UNC\"))
        });
    // On the string: `Path::components` silently drops inner `.` components.
    let dots = s.split(['\\', '/']).any(|c| c == "." || c == "..");
    if !p.is_absolute() || s.contains('\0') || device || dots {
        Err("D2R location must be an absolute folder path".into())
    } else {
        Ok(())
    }
}

impl Preferences {
    fn validate(&self) -> Result<(), String> {
        if let Some(p) = &self.d2r_location {
            check_d2r_location(p)?;
        }
        if let Some(id) = &self.last_product {
            check_text("last product", &id.0)?;
        }
        Ok(())
    }
}

/// The record that a release of a product is installed. Written only through
/// [`StateStore::mark_installed`], after verified activation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledRelease {
    pub product_id: ProductId,
    pub release_id: String,
    pub display_version: String,
    pub manifest_sha256: Sha256Digest,
    /// Seconds since the Unix epoch.
    pub installed_at: u64,
    /// The installed files matched the release manifest's inventory when the record
    /// was written. [`StateStore::mark_installed`] only accepts `true` (the record is
    /// written after `LiveVerified`); `false` is reserved for a later Repair check.
    pub verified: bool,
}

fn check_text(what: &str, s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 200 || s.chars().any(char::is_control) {
        Err(format!("{what} must be 1-200 printable characters"))
    } else {
        Ok(())
    }
}

impl InstalledRelease {
    fn validate(&self) -> Result<(), String> {
        check_text("product ID", &self.product_id.0)?;
        check_text("release ID", &self.release_id)?;
        check_text("display version", &self.display_version)
    }
}

fn validate_installed(records: &[InstalledRelease]) -> Result<(), String> {
    for (i, r) in records.iter().enumerate() {
        r.validate()?;
        if records[..i].iter().any(|o| o.product_id == r.product_id) {
            return Err(format!("product {} recorded twice", r.product_id.0));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheEntry {
    identity: String,
    sha256: Sha256Digest,
    size: u64,
}

fn validate_cache_index(entries: &[CacheEntry]) -> Result<(), String> {
    for (i, e) in entries.iter().enumerate() {
        check_text("cache identity", &e.identity)?;
        if entries[..i].iter().any(|o| o.identity == e.identity) {
            return Err(format!("cache identity {} recorded twice", e.identity));
        }
    }
    Ok(())
}

/// Metadata bytes the Catalog validated earlier, returned only if they still hash
/// to the SHA-256 recorded when they were cached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedMetadata {
    pub bytes: Vec<u8>,
    pub sha256: Sha256Digest,
}

fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
        .expect("a real SHA-256 is never the all-zero placeholder")
}

fn invalid(message: String) -> LauncherError {
    LauncherError::new(ErrorCode::InvalidRequest, message)
}

/// The launcher's persisted state in one directory (the app passes a per-user
/// folder under `%LOCALAPPDATA%`; tests pass a temp dir). Single-instance: the app
/// holds one store behind a lock.
#[derive(Debug)]
pub struct StateStore {
    dir: PathBuf,
    preferences: Preferences,
    installed: Vec<InstalledRelease>,
    cache: Vec<CacheEntry>,
    issues: Vec<StateIssue>,
    /// State files that could not be trusted and are still in place (unreadable, or
    /// the quarantine rename failed). They are rechecked before any write to them,
    /// and the write is refused while they stay, so their bytes are never overwritten.
    blocked: Vec<&'static str>,
}

type Found = (Vec<StateIssue>, Vec<&'static str>);

fn note_issue(issue: StateIssue, name: &'static str, found: &mut Found) {
    if issue.quarantined_as.is_none() && !found.1.contains(&name) {
        found.1.push(name);
    }
    found.0.push(issue);
}

impl StateStore {
    /// Loads every state file in `dir`. Never fails: missing files give defaults,
    /// and untrusted files are quarantined and listed in [`Self::issues`].
    pub fn open(dir: &Path) -> Self {
        fn load<T: serde::de::DeserializeOwned + Default>(
            dir: &Path,
            name: &'static str,
            schema: u32,
            validate: fn(&T) -> Result<(), String>,
            found: &mut Found,
        ) -> T {
            file::load(&dir.join(name), schema, validate)
                .unwrap_or_else(|issue| {
                    note_issue(issue, name, found);
                    None
                })
                .unwrap_or_default()
        }
        let mut issues = (Vec::new(), Vec::new());
        let preferences = load(
            dir,
            PREFERENCES_FILE,
            PREFERENCES_SCHEMA,
            Preferences::validate,
            &mut issues,
        );
        let installed = load(
            dir,
            INSTALLED_FILE,
            INSTALLED_SCHEMA,
            |r: &Vec<_>| validate_installed(r),
            &mut issues,
        );
        let cache = load(
            dir,
            CACHE_INDEX_FILE,
            CACHE_INDEX_SCHEMA,
            |c: &Vec<_>| validate_cache_index(c),
            &mut issues,
        );
        Self {
            dir: dir.to_owned(),
            preferences,
            installed,
            cache,
            issues: issues.0,
            blocked: issues.1,
        }
    }

    /// Before writing a blocked file: re-reads it. `Ok(Some(data))` when it is now
    /// readable (the caller adopts `data` before applying its change), `Ok(None)` when
    /// there is nothing to adopt, and an error, with nothing written, while the
    /// untrusted bytes are still in place.
    fn recheck<T: serde::de::DeserializeOwned>(
        &mut self,
        name: &'static str,
        schema: u32,
        validate: fn(&T) -> Result<(), String>,
    ) -> Result<Option<T>, LauncherError> {
        if !self.blocked.contains(&name) {
            return Ok(None);
        }
        self.blocked.retain(|n| *n != name);
        let issue = match file::load(&self.dir.join(name), schema, validate) {
            Ok(data) => return Ok(data),
            Err(issue) => issue,
        };
        let detail = issue.detail.clone();
        let mut found = (
            std::mem::take(&mut self.issues),
            std::mem::take(&mut self.blocked),
        );
        note_issue(issue, name, &mut found);
        (self.issues, self.blocked) = found;
        if self.blocked.contains(&name) {
            Err(LauncherError::new(
                ErrorCode::AccessDenied,
                format!("state file {name} cannot be read or moved aside, so nothing was written: {detail}"),
            ))
        } else {
            Ok(None)
        }
    }

    /// Problems found with state files since [`Self::open`], oldest first.
    pub fn issues(&self) -> &[StateIssue] {
        &self.issues
    }

    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    pub fn set_preferences(&mut self, preferences: Preferences) -> Result<(), LauncherError> {
        preferences.validate().map_err(invalid)?;
        self.recheck::<Preferences>(PREFERENCES_FILE, PREFERENCES_SCHEMA, Preferences::validate)?;
        let path = self.dir.join(PREFERENCES_FILE);
        file::save(&path, PREFERENCES_SCHEMA, &preferences)?;
        self.preferences = preferences;
        Ok(())
    }

    pub fn installed_release(&self, product: &ProductId) -> Option<&InstalledRelease> {
        self.installed.iter().find(|r| &r.product_id == product)
    }

    pub fn installed_releases(&self) -> &[InstalledRelease] {
        &self.installed
    }

    fn installed_for_write(&mut self) -> Result<Vec<InstalledRelease>, LauncherError> {
        let validate = |r: &Vec<_>| validate_installed(r);
        if let Some(fresh) = self.recheck(INSTALLED_FILE, INSTALLED_SCHEMA, validate)? {
            self.installed = fresh;
        }
        Ok(self.installed.clone())
    }

    fn save_installed(&mut self, installed: Vec<InstalledRelease>) -> Result<(), LauncherError> {
        file::save(&self.dir.join(INSTALLED_FILE), INSTALLED_SCHEMA, &installed)?;
        self.installed = installed;
        Ok(())
    }

    /// The only way to mark a release installed: replaces the product's record.
    /// Install calls this only after verified activation, never during rollback.
    pub fn mark_installed(&mut self, record: InstalledRelease) -> Result<(), LauncherError> {
        record.validate().map_err(invalid)?;
        if !record.verified {
            return Err(invalid(
                "a release is recorded installed only after verified activation".into(),
            ));
        }
        let mut installed = self.installed_for_write()?;
        installed.retain(|r| r.product_id != record.product_id);
        installed.push(record);
        self.save_installed(installed)
    }

    /// Forgets the product's installed release (after uninstall).
    pub fn remove_installed(&mut self, product: &ProductId) -> Result<(), LauncherError> {
        let mut installed = self.installed_for_write()?;
        installed.retain(|r| &r.product_id != product);
        self.save_installed(installed)
    }

    fn blob_path(&self, sha256: &Sha256Digest) -> PathBuf {
        self.dir.join(CACHE_DIR).join(format!("{sha256}.bin"))
    }

    fn cache_for_write(&mut self) -> Result<Vec<CacheEntry>, LauncherError> {
        let validate = |c: &Vec<_>| validate_cache_index(c);
        if let Some(fresh) = self.recheck(CACHE_INDEX_FILE, CACHE_INDEX_SCHEMA, validate)? {
            self.cache = fresh;
        }
        Ok(self.cache.clone())
    }

    fn save_cache(&mut self, cache: Vec<CacheEntry>) -> Result<(), LauncherError> {
        file::save(&self.dir.join(CACHE_INDEX_FILE), CACHE_INDEX_SCHEMA, &cache)?;
        let old = std::mem::replace(&mut self.cache, cache);
        for e in old {
            if !self.cache.iter().any(|n| n.sha256 == e.sha256) {
                // Best effort: an orphan blob is harmless and unreferenced.
                let _ = fs::remove_file(self.blob_path(&e.sha256));
            }
        }
        Ok(())
    }

    /// Caches metadata the Catalog has validated, under a Catalog-chosen identity
    /// (for example `feed` or `manifest/pd2r/<release>`). Returns its SHA-256.
    pub fn put_cached(
        &mut self,
        identity: &str,
        bytes: &[u8],
    ) -> Result<Sha256Digest, LauncherError> {
        check_text("cache identity", identity).map_err(invalid)?;
        let digest = sha256(bytes);
        let mut cache = self.cache_for_write()?;
        // Blob first, then the index that points at it: a crash between leaves an
        // unreferenced blob, never an index entry without its bytes.
        file::write_atomic(&self.blob_path(&digest), bytes)?;
        cache.retain(|e| e.identity != identity);
        cache.push(CacheEntry {
            identity: identity.to_owned(),
            sha256: digest,
            size: bytes.len() as u64,
        });
        self.save_cache(cache)?;
        Ok(digest)
    }

    /// The cached bytes for `identity`, if present and still matching their
    /// SHA-256. A damaged blob is quarantined, a missing one forgotten, and both
    /// reported; a blob that cannot be read right now is reported and kept.
    pub fn cached(&mut self, identity: &str) -> Option<CachedMetadata> {
        let entry = self.cache.iter().find(|e| e.identity == identity)?.clone();
        let path = self.blob_path(&entry.sha256);
        let file = format!("{}.bin", entry.sha256);
        let issue = match fs::read(&path) {
            Ok(bytes) if bytes.len() as u64 == entry.size && sha256(&bytes) == entry.sha256 => {
                return Some(CachedMetadata {
                    bytes,
                    sha256: entry.sha256,
                })
            }
            Ok(_) => file::quarantine(
                &path,
                StateProblem::Corrupt,
                "cached bytes do not match their SHA-256".into(),
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => StateIssue {
                file,
                problem: StateProblem::Corrupt,
                detail: "cached bytes are missing".into(),
                quarantined_as: None,
            },
            Err(e) => {
                self.issues.push(StateIssue {
                    file,
                    problem: StateProblem::Unreadable,
                    detail: e.to_string(),
                    quarantined_as: None,
                });
                return None;
            }
        };
        self.issues.push(issue);
        // If the index cannot be saved the entry stays; the next read fails the same way.
        if let Ok(mut cache) = self.cache_for_write() {
            cache.retain(|e| e.identity != identity);
            let _ = self.save_cache(cache);
        }
        None
    }

    /// Where the app writes the launcher log that diagnostics may include.
    pub fn log_path(&self) -> PathBuf {
        self.dir.join(log::LOG_FILE)
    }

    /// Appends one line to `launcher.log` ([`log::append`]).
    pub fn log(&self, line: &str) {
        log::append(&self.dir, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pd2r() -> ProductId {
        ProductId("pd2r".into())
    }

    fn record(release: &str) -> InstalledRelease {
        InstalledRelease {
            product_id: pd2r(),
            release_id: release.into(),
            display_version: format!("alpha {release}"),
            manifest_sha256: sha256(release.as_bytes()),
            installed_at: 1_760_000_000,
            verified: true,
        }
    }

    fn absolute(p: &str) -> PathBuf {
        std::env::temp_dir().join(p)
    }

    #[test]
    fn fresh_state_directory_has_default_preferences_and_no_installed_release() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::open(&dir.path().join("not-yet-created"));
        assert_eq!(store.preferences(), &Preferences::default());
        assert!(store.preferences().download_launcher_updates_automatically);
        assert!(!store.preferences().start_with_windows);
        assert!(store.installed_releases().is_empty());
        assert!(store.issues().is_empty());
    }

    #[test]
    fn preferences_persist_across_restart() {
        let dir = tempfile::tempdir().unwrap();
        let prefs = Preferences {
            d2r_location: Some(absolute("Diablo II Resurrected")),
            start_with_windows: true,
            download_launcher_updates_automatically: false,
            last_product: Some(pd2r()),
        };
        StateStore::open(dir.path())
            .set_preferences(prefs.clone())
            .unwrap();
        let store = StateStore::open(dir.path());
        assert_eq!(store.preferences(), &prefs);
        assert!(store.issues().is_empty());
    }

    #[test]
    fn unsafe_d2r_location_or_last_product_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(dir.path());
        let bad_locations = [
            "Games/D2R",
            r"C:\Games\..\Windows",
            r"C:\Games\.\D2R",
            "C:\\Games\0D2R",
            r"\\.\PhysicalDrive0",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1",
            "//./C:/Games",
        ];
        for bad in bad_locations {
            let prefs = Preferences {
                d2r_location: Some(bad.into()),
                ..Preferences::default()
            };
            let err = store.set_preferences(prefs).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidRequest, "{bad}");
        }
        let forged = Preferences {
            last_product: Some(ProductId("pd2r\nInstalled: yes".into())),
            ..Preferences::default()
        };
        assert!(store.set_preferences(forged).is_err());
        assert!(!dir.path().join(PREFERENCES_FILE).exists());
        for good in [
            r"C:\Games\D2R",
            r"\\?\C:\Games\D2R",
            r"\\?\UNC\nas\games\D2R",
            r"\\nas\games\D2R",
        ] {
            let prefs = Preferences {
                d2r_location: Some(good.into()),
                ..Preferences::default()
            };
            store.set_preferences(prefs).unwrap();
        }
    }

    #[test]
    fn unverified_installed_release_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(dir.path());
        let unverified = InstalledRelease {
            verified: false,
            ..record("0.4.2")
        };
        assert_eq!(
            store.mark_installed(unverified).unwrap_err().code,
            ErrorCode::InvalidRequest
        );
        assert!(!dir.path().join(INSTALLED_FILE).exists());
    }

    /// Opens `path` the way antivirus or another process might hold it.
    #[cfg(windows)]
    fn hold(path: &Path, share: u32) -> fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(share)
            .open(path)
            .unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn locked_installed_release_records_are_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(INSTALLED_FILE);
        let forever = InstalledRelease {
            product_id: ProductId("forever".into()),
            ..record("1.0")
        };
        let mut store = StateStore::open(dir.path());
        store.mark_installed(record("0.4.1")).unwrap();
        store.mark_installed(forever.clone()).unwrap();
        let original = fs::read(&path).unwrap();

        let lock = hold(&path, 0);
        let mut store = StateStore::open(dir.path());
        assert_eq!(store.issues()[0].problem, StateProblem::Unreadable);
        assert!(store.installed_release(&pd2r()).is_none());
        let err = store.mark_installed(record("0.4.2")).unwrap_err();
        assert_eq!(err.code, ErrorCode::AccessDenied);
        assert!(store.remove_installed(&pd2r()).is_err());
        drop(lock);
        assert_eq!(fs::read(&path).unwrap(), original, "original bytes survive");

        // Once readable, the write applies on top of the records on disk.
        store.mark_installed(record("0.4.2")).unwrap();
        let reopened = StateStore::open(dir.path());
        assert_eq!(reopened.installed_release(&pd2r()), Some(&record("0.4.2")));
        assert_eq!(
            reopened.installed_release(&ProductId("forever".into())),
            Some(&forever)
        );
    }

    #[cfg(windows)]
    #[test]
    fn corrupt_preferences_that_cannot_be_quarantined_are_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PREFERENCES_FILE);
        fs::write(&path, b"{ newer launcher, half-parsed").unwrap();
        // Readable, but no FILE_SHARE_DELETE, so the quarantine rename fails.
        let lock = hold(&path, 1);
        let mut store = StateStore::open(dir.path());
        assert_eq!(store.issues()[0].quarantined_as, None);
        assert!(store.set_preferences(Preferences::default()).is_err());
        drop(lock);
        assert_eq!(fs::read(&path).unwrap(), b"{ newer launcher, half-parsed");
        // Unlocked: the next write quarantines it first, then writes.
        store.set_preferences(Preferences::default()).unwrap();
        let aside = store
            .issues()
            .last()
            .unwrap()
            .quarantined_as
            .clone()
            .unwrap();
        assert_eq!(
            fs::read(dir.path().join(aside)).unwrap(),
            b"{ newer launcher, half-parsed"
        );
    }

    #[test]
    fn directory_at_preferences_path_refuses_the_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PREFERENCES_FILE);
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), b"x").unwrap();
        let mut store = StateStore::open(dir.path());
        assert_eq!(store.issues().len(), 1);
        assert!(store.set_preferences(Preferences::default()).is_err());
        assert!(path.join("keep").exists());
    }

    #[cfg(windows)]
    #[test]
    fn transiently_unreadable_cached_metadata_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(dir.path());
        let digest = store.put_cached("feed", b"validated").unwrap();
        let lock = hold(&store.blob_path(&digest), 0);
        assert!(store.cached("feed").is_none());
        assert_eq!(store.issues()[0].problem, StateProblem::Unreadable);
        drop(lock);
        assert_eq!(store.cached("feed").unwrap().bytes, b"validated");
    }

    #[test]
    fn crash_mid_write_of_preferences_keeps_previous_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(dir.path());
        let before = Preferences {
            start_with_windows: true,
            ..Preferences::default()
        };
        store.set_preferences(before.clone()).unwrap();
        let path = dir.path().join(PREFERENCES_FILE);
        let next = br#"{"schema_version":1,"data":{"d2r_location":null,"start_with_windows":false"#;
        file::write_crashing(&path, next, next.len() / 2);
        let store = StateStore::open(dir.path());
        assert_eq!(store.preferences(), &before);
        assert!(store.issues().is_empty());
    }

    #[test]
    fn crash_mid_write_of_installed_release_record_keeps_previous_record() {
        let dir = tempfile::tempdir().unwrap();
        StateStore::open(dir.path())
            .mark_installed(record("0.4.1"))
            .unwrap();
        let path = dir.path().join(INSTALLED_FILE);
        let next = serde_json::to_vec(&serde_json::json!({
            "schema_version": INSTALLED_SCHEMA, "data": [record("0.4.2")]
        }))
        .unwrap();
        file::write_crashing(&path, &next, next.len() - 1);
        let store = StateStore::open(dir.path());
        assert_eq!(store.installed_release(&pd2r()), Some(&record("0.4.1")));
    }

    #[test]
    fn corrupt_preferences_are_quarantined_and_defaults_used() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(PREFERENCES_FILE), b"\0\0garbage").unwrap();
        let store = StateStore::open(dir.path());
        assert_eq!(store.preferences(), &Preferences::default());
        let [issue] = store.issues() else {
            panic!("{:?}", store.issues())
        };
        assert_eq!(issue.file, PREFERENCES_FILE);
        assert_eq!(issue.problem, StateProblem::Corrupt);
        assert!(dir
            .path()
            .join(issue.quarantined_as.as_ref().unwrap())
            .exists());
    }

    #[test]
    fn installed_release_record_round_trips_and_is_replaced_per_product() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(dir.path());
        store.mark_installed(record("0.4.1")).unwrap();
        store.mark_installed(record("0.4.2")).unwrap();
        let store_after_restart = StateStore::open(dir.path());
        assert_eq!(store_after_restart.installed_releases(), &[record("0.4.2")]);
        store.remove_installed(&pd2r()).unwrap();
        assert!(StateStore::open(dir.path())
            .installed_release(&pd2r())
            .is_none());
    }

    #[test]
    fn invalid_installed_release_record_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(dir.path());
        for bad in [
            InstalledRelease {
                release_id: " ".into(),
                ..record("0.4.2")
            },
            InstalledRelease {
                display_version: "a\nb".into(),
                ..record("0.4.2")
            },
            InstalledRelease {
                product_id: ProductId(String::new()),
                ..record("0.4.2")
            },
        ] {
            assert_eq!(
                store.mark_installed(bad).unwrap_err().code,
                ErrorCode::InvalidRequest
            );
        }
        assert!(store.installed_releases().is_empty());
    }

    #[test]
    fn unreadable_installed_release_records_never_fabricate_an_installed_release() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(INSTALLED_FILE);
        let duplicate = serde_json::json!({
            "schema_version": INSTALLED_SCHEMA, "data": [record("0.4.1"), record("0.4.2")]
        });
        let newer = serde_json::json!({ "schema_version": 99, "data": [record("0.4.2")] });
        let placeholder_hash = serde_json::json!({ "schema_version": 1, "data": [{
            "product_id": "pd2r", "release_id": "0.4.2", "display_version": "alpha 0.4.2",
            "manifest_sha256": "0".repeat(64), "installed_at": 1, "verified": true
        }]});
        for (json, problem) in [
            (duplicate, StateProblem::Corrupt),
            (newer, StateProblem::UnknownVersion),
            (placeholder_hash, StateProblem::Corrupt),
        ] {
            fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
            let store = StateStore::open(dir.path());
            assert!(store.installed_release(&pd2r()).is_none());
            assert_eq!(store.issues()[0].problem, problem);
            assert!(!path.exists());
        }
    }

    #[test]
    fn cached_metadata_round_trips_and_replacing_drops_the_old_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(dir.path());
        let first = store.put_cached("feed", b"{\"v\":1}").unwrap();
        let second = store.put_cached("feed", b"{\"v\":2}").unwrap();
        assert_ne!(first, second);
        assert!(!store.blob_path(&first).exists());
        let mut store = StateStore::open(dir.path());
        let cached = store.cached("feed").unwrap();
        assert_eq!(
            (cached.bytes.as_slice(), cached.sha256),
            (&b"{\"v\":2}"[..], second)
        );
        assert!(store.cached("manifest/pd2r/0.4.2").is_none());
        assert!(store.issues().is_empty());
    }

    #[test]
    fn tampered_cached_metadata_is_quarantined_not_returned() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(dir.path());
        let digest = store.put_cached("feed", b"validated").unwrap();
        fs::write(store.blob_path(&digest), b"validatee").unwrap();
        let mut store = StateStore::open(dir.path());
        assert!(store.cached("feed").is_none());
        assert_eq!(store.issues()[0].problem, StateProblem::Corrupt);
        assert!(store.issues()[0].quarantined_as.is_some());
        assert!(StateStore::open(dir.path()).cached("feed").is_none());
    }

    #[test]
    fn deleted_cached_metadata_is_reported_and_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(dir.path());
        let digest = store.put_cached("feed", b"validated").unwrap();
        fs::remove_file(store.blob_path(&digest)).unwrap();
        assert!(store.cached("feed").is_none());
        assert_eq!(store.issues().len(), 1);
        assert!(store.cached("feed").is_none());
        assert_eq!(
            store.issues().len(),
            1,
            "forgotten entry is not re-reported"
        );
    }
}
