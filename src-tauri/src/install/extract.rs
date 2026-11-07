//! Confined extraction with the bundled, pinned 7-Zip console (#6 step 3, #8).
//!
//! There are exactly two fixed invocations, both on the verified archive at
//! [`TransactionPaths::archive`] with its format named explicitly: list
//! (`l -slt`) and extract (`x`) into fresh staging. No caller passes arguments,
//! and nothing here is reachable from the frontend.
//!
//! Every listed entry is checked before anything is written: the path must be a
//! [`RelativePath`] (traversal, absolute, drive, stream, device names, trailing
//! dot/space), no two entries may collide ignoring case, links/reparse points/
//! hard links and anti-items are refused, and every entry must be in the
//! [`ExpectedInventory`]. After extraction, [`verify_staging`] checks the exact
//! tree against the inventory, so whatever 7-Zip did on disk is checked again.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::download::{hash_file, io_error, verify_file};
use super::paths::{under, TransactionPaths};
use crate::catalog::ArchiveFormat;
use crate::contract::{ErrorCode, ExpectedInventory, LauncherError, RelativePath, Sha256Digest};

/// Pinned 7-Zip Extra console (`x64/7za.exe`), see `docs/third-party.md`.
pub const SEVEN_ZIP_VERSION: &str = "26.04";
const SEVEN_ZIP_SIZE: u64 = 1_335_808;
const SEVEN_ZIP_SHA256: &str = "15d4c788c148e3677e2fc1c4a01f191fb6669eba4f6acf8a465f0f1a6f1e1260";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// The 7-Zip `-t` switch for an archive. Production only knows the manifest's
/// [`ArchiveFormat`]; tests also feed hand-built zip and tar fixtures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    SevenZ,
    #[cfg(test)]
    Zip,
    /// The only fixture format that can express hard links.
    #[cfg(test)]
    Tar,
}

impl Kind {
    fn switch(self) -> &'static str {
        match self {
            Self::SevenZ => "-t7z",
            #[cfg(test)]
            Self::Zip => "-tzip",
            #[cfg(test)]
            Self::Tar => "-ttar",
        }
    }
}

impl From<ArchiveFormat> for Kind {
    fn from(format: ArchiveFormat) -> Self {
        match format {
            ArchiveFormat::SevenZip => Self::SevenZ,
        }
    }
}

fn unsafe_entry(message: impl Into<String>) -> LauncherError {
    LauncherError::new(ErrorCode::UnsafeArchiveEntry, message)
}

fn mismatch(message: impl Into<String>) -> LauncherError {
    LauncherError::new(ErrorCode::InventoryMismatch, message)
}

fn internal(message: impl Into<String>) -> LauncherError {
    LauncherError::new(ErrorCode::Internal, message)
}

/// The bundled 7-Zip. Its size and SHA-256 are checked against the pin when it is
/// located and again before every invocation.
#[derive(Debug)]
pub struct SevenZip {
    exe: PathBuf,
}

impl SevenZip {
    /// `exe` is the bundled `resources/7zip/7za.exe`; refused unless its size and
    /// SHA-256 match the pin.
    pub fn bundled(exe: &Path) -> Result<Self, LauncherError> {
        let seven_zip = Self {
            exe: exe.to_owned(),
        };
        seven_zip.check_pin()?;
        Ok(seven_zip)
    }

    fn check_pin(&self) -> Result<(), LauncherError> {
        let pin: Sha256Digest = SEVEN_ZIP_SHA256.parse().expect("pinned digest is hex");
        if verify_file(&self.exe, SEVEN_ZIP_SIZE, &pin, &AtomicBool::new(false))? {
            Ok(())
        } else {
            Err(internal(format!(
                "bundled 7-Zip at {} does not match the pinned {SEVEN_ZIP_VERSION}",
                self.exe.display()
            )))
        }
    }

    /// The only way a 7-Zip process is built; re-checks the pin first.
    fn command(&self, op: &str, kind: Kind) -> Result<Command, LauncherError> {
        // shortcut: a same-user process could swap the file between this check and the
        // spawn; out of scope for a normal-user launcher, revisit if 7-Zip moves to a
        // writable location.
        self.check_pin()?;
        let mut cmd = Command::new(&self.exe);
        cmd.args([op, kind.switch(), "-sccUTF-8", "-y"])
            .stdin(Stdio::null());
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, CREATE_NO_WINDOW);
        Ok(cmd)
    }

    fn list(&self, archive: &Path, kind: Kind) -> Result<Vec<Listed>, LauncherError> {
        let out = self
            .command("l", kind)?
            .args(["-slt", "--"])
            .arg(archive)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| internal(format!("running 7-Zip: {e}")))?;
        if !out.status.success() {
            return Err(internal(format!(
                "7-Zip could not list the archive ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let text =
            String::from_utf8(out.stdout).map_err(|_| internal("7-Zip listing is not UTF-8"))?;
        parse_listing(&text)
    }

    fn extract_all(
        &self,
        archive: &Path,
        kind: Kind,
        out: &Path,
        cancel: &AtomicBool,
    ) -> Result<(), LauncherError> {
        let mut out_switch = OsString::from("-o");
        out_switch.push(out);
        // shortcut: output is discarded and only the exit code is reported; capture
        // stderr on a reader thread if diagnostics need 7-Zip's message.
        let mut child = self
            .command("x", kind)?
            .args(["-bso0", "-bsp0"])
            .arg(out_switch)
            .arg("--")
            .arg(archive)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| internal(format!("running 7-Zip: {e}")))?;
        loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(LauncherError::new(
                    ErrorCode::Cancelled,
                    "cancelled by the user",
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => {
                    return Err(internal(format!("7-Zip extraction failed ({status})")))
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(internal(format!("waiting for 7-Zip: {e}"))),
            }
        }
    }
}

/// One `-slt` block: property name to raw value.
type Listed = HashMap<String, String>;

fn prop<'a>(entry: &'a Listed, key: &str) -> &'a str {
    entry.get(key).map_or("", String::as_str)
}

fn parse_listing(text: &str) -> Result<Vec<Listed>, LauncherError> {
    let text = text.replace("\r\n", "\n");
    // Archive-level properties come first; entries follow the dashed line.
    let Some((_, body)) = text.split_once("\n----------\n") else {
        return Ok(Vec::new());
    };
    let mut entries = Vec::new();
    let mut current = Listed::new();
    for line in body.lines() {
        if line.is_empty() {
            if !current.is_empty() {
                entries.push(std::mem::take(&mut current));
            }
            continue;
        }
        let (key, value) = line
            .split_once(" = ")
            .ok_or_else(|| unsafe_entry(format!("unreadable listing line {line:?}")))?;
        if current.is_empty() && key != "Path" {
            return Err(unsafe_entry(format!("listing entry starts with {key:?}")));
        }
        // A repeated property means a name smuggled listing lines in.
        if current.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(unsafe_entry(format!("listing repeats {key:?}")));
        }
    }
    if !current.is_empty() {
        entries.push(current);
    }
    Ok(entries)
}

/// A POSIX mode string such as `-rw-r--r--` or `lrwxrwxrwx`.
fn is_posix_mode(s: &str) -> bool {
    s.len() == 10 && s.chars().skip(1).all(|c| "rwxsStT-".contains(c))
}

/// Every directory a valid tree may contain: ancestors of every leaf, plus the
/// required empty dirs.
fn allowed_dirs(inventory: &ExpectedInventory) -> HashSet<&str> {
    let leaves = inventory.files().iter().map(|f| &f.path);
    let mut dirs: HashSet<&str> = inventory
        .empty_dirs()
        .iter()
        .map(RelativePath::as_str)
        .collect();
    for leaf in leaves.chain(inventory.empty_dirs()) {
        let s = leaf.as_str();
        dirs.extend(s.match_indices('/').map(|(i, _)| &s[..i]));
    }
    dirs
}

fn validate(entries: &[Listed], inventory: &ExpectedInventory) -> Result<(), LauncherError> {
    let files: HashMap<&str, u64> = inventory
        .files()
        .iter()
        .map(|f| (f.path.as_str(), f.size))
        .collect();
    let dirs = allowed_dirs(inventory);
    let mut entry_keys = HashSet::new();
    // Case-folded path (entry or ancestor) -> first spelling seen.
    let mut spellings: HashMap<String, String> = HashMap::new();
    for entry in entries {
        let raw = prop(entry, "Path");
        let bad = |why: &str| unsafe_entry(format!("archive entry {raw:?}: {why}"));
        if prop(entry, "Anti") == "+" {
            return Err(bad("anti-item"));
        }
        if ["Symbolic Link", "Hard Link", "Link"]
            .iter()
            .any(|k| !prop(entry, k).is_empty())
        {
            return Err(bad("link entry"));
        }
        // `Attributes` is `<Windows letters>[ <POSIX mode or hex>]`; L is a reparse point.
        let attrs = prop(entry, "Attributes");
        let (letters, posix) = attrs.split_once(' ').unwrap_or((attrs, ""));
        if letters.contains('L') {
            return Err(bad("reparse point"));
        }
        if [posix, prop(entry, "Mode")]
            .iter()
            .any(|m| is_posix_mode(m) && !m.starts_with(['-', 'd']))
        {
            return Err(bad("link or special file"));
        }
        let path = RelativePath::new(raw.replace('\\', "/"))
            .map_err(|e| bad(&format!("unsafe path ({e})")))?;
        let s = path.as_str();
        if !entry_keys.insert(s.to_uppercase()) {
            return Err(bad("duplicate entry, possibly differing only in case"));
        }
        let prefixes = s.match_indices('/').map(|(i, _)| &s[..i]).chain([s]);
        for prefix in prefixes {
            let first = spellings
                .entry(prefix.to_uppercase())
                .or_insert_with(|| prefix.to_owned());
            if first != prefix {
                return Err(bad(&format!("differs only in case from {first:?}")));
            }
        }
        let is_dir =
            prop(entry, "Folder") == "+" || letters.contains('D') || posix.starts_with('d');
        if is_dir {
            if !dirs.contains(s) {
                return Err(bad("directory not in the file list"));
            }
        } else {
            let Some(&size) = files.get(s) else {
                return Err(bad("not in the file list"));
            };
            if prop(entry, "Size") != size.to_string() {
                return Err(mismatch(format!(
                    "archive entry {raw:?} declares size {:?}, expected {size}",
                    prop(entry, "Size")
                )));
            }
        }
    }
    Ok(())
}

/// Validates every entry of the verified archive, then extracts it with the pinned
/// 7-Zip into fresh staging [`TransactionPaths::staging`] (a sibling of the
/// product root, so on its filesystem). The live root is never touched. Call
/// [`verify_staging`] next.
///
/// # Staging left behind (for the transaction, #36)
///
/// * Rejected entries or a failed list: staging is never created.
/// * 7-Zip failure or cancel: `extract` removes staging itself, best effort. If that
///   removal fails (a file held open by a scanner, say) staging stays.
/// * Success, or [`verify_staging`] failing afterwards: staging stays; it is the
///   transaction's to remove (rollback or [`crate::contract::Boundary::CleanedUp`]).
/// * A crash at any point: staging may exist, partly written.
///
/// `extract` never reuses or merges into an existing staging directory: it fails
/// with [`ErrorCode::Internal`] instead. So before extracting, the transaction must
/// remove any staging its journal planned (staging only ever holds archive output,
/// never player or unknown files, so deleting it whole is safe), and must not delete
/// a staging path its journal does not own.
///
/// Errors: [`ErrorCode::UnsafeArchiveEntry`] (nothing written),
/// [`ErrorCode::InventoryMismatch`] (declared size differs; nothing written),
/// [`ErrorCode::Cancelled`], [`ErrorCode::Internal`] (7-Zip failure, pin mismatch,
/// staging already exists).
pub fn extract(
    seven_zip: &SevenZip,
    format: ArchiveFormat,
    inventory: &ExpectedInventory,
    paths: &TransactionPaths,
    cancel: &AtomicBool,
) -> Result<(), LauncherError> {
    extract_kind(seven_zip, format.into(), inventory, paths, cancel)
}

fn extract_kind(
    seven_zip: &SevenZip,
    kind: Kind,
    inventory: &ExpectedInventory,
    paths: &TransactionPaths,
    cancel: &AtomicBool,
) -> Result<(), LauncherError> {
    let archive = paths.archive();
    validate(&seven_zip.list(archive, kind)?, inventory)?;
    let staging = paths.staging();
    // create_dir, not create_dir_all: never extract into an existing directory.
    fs::create_dir(staging).map_err(|e| io_error("creating staging", staging, e))?;
    let result = seven_zip.extract_all(archive, kind, staging, cancel);
    if result.is_err() {
        // std's remove_dir_all does not follow links or junctions.
        let _ = fs::remove_dir_all(staging);
    }
    result
}

fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

fn walk(
    dir: &Path,
    prefix: &str,
    files: &mut HashSet<String>,
    dirs: &mut HashSet<String>,
) -> Result<(), LauncherError> {
    for entry in fs::read_dir(dir).map_err(|e| io_error("reading", dir, e))? {
        let entry = entry.map_err(|e| io_error("reading", dir, e))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|n| mismatch(format!("non-Unicode name {n:?} in staging")))?;
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| io_error("reading", &path, e))?;
        if is_link(&meta) {
            return Err(unsafe_entry(format!("{rel} is a link or reparse point")));
        } else if meta.is_dir() {
            walk(&path, &rel, files, dirs)?;
            dirs.insert(rel);
        } else if meta.is_file() {
            files.insert(rel);
        } else {
            return Err(unsafe_entry(format!("{rel} is not a regular file")));
        }
    }
    Ok(())
}

/// Checks that staging holds exactly `inventory`: every file present with its size
/// and SHA-256 and spelled exactly as listed, every required empty dir present, no
/// extra file or directory, and no link or reparse point anywhere.
///
/// Errors: [`ErrorCode::InventoryMismatch`], [`ErrorCode::UnsafeArchiveEntry`]
/// (a link), [`ErrorCode::Cancelled`].
pub fn verify_staging(
    paths: &TransactionPaths,
    inventory: &ExpectedInventory,
    cancel: &AtomicBool,
) -> Result<(), LauncherError> {
    let staging = paths.staging();
    let (mut files, mut dirs) = (HashSet::new(), HashSet::new());
    walk(staging, "", &mut files, &mut dirs)?;
    for expected in inventory.files() {
        let rel = expected.path.as_str();
        if !files.remove(rel) {
            return Err(mismatch(format!("{rel} is missing from staging")));
        }
        let (size, digest) = hash_file(&under(staging, &expected.path), cancel)?;
        if size != expected.size || &digest != expected.sha256.as_bytes() {
            return Err(mismatch(format!(
                "{rel} differs from the release inventory"
            )));
        }
    }
    if let Some(extra) = files.iter().min() {
        return Err(mismatch(format!("unexpected file {extra} in staging")));
    }
    if let Some(missing) = inventory
        .empty_dirs()
        .iter()
        .find(|d| !dirs.contains(d.as_str()))
    {
        return Err(mismatch(format!("required directory {missing} is missing")));
    }
    let allowed = allowed_dirs(inventory);
    if let Some(extra) = dirs.iter().filter(|d| !allowed.contains(d.as_str())).min() {
        return Err(mismatch(format!("unexpected directory {extra} in staging")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ExpectedFile;
    use crate::install::download::tests::{disposable_root, sha256};

    fn bundled() -> SevenZip {
        let exe = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/7zip/7za.exe");
        SevenZip::bundled(&exe).expect("pinned 7-Zip is bundled")
    }

    fn inventory(files: &[(&str, &[u8])], empty_dirs: &[&str]) -> ExpectedInventory {
        ExpectedInventory::new(
            files
                .iter()
                .map(|(p, b)| ExpectedFile {
                    path: RelativePath::new(*p).unwrap(),
                    size: b.len() as u64,
                    sha256: sha256(b),
                })
                .collect(),
            empty_dirs
                .iter()
                .map(|d| RelativePath::new(*d).unwrap())
                .collect(),
        )
        .unwrap()
    }

    // ---- fixture archive writers (stored zip, ustar) ----

    struct ZipEntry<'a> {
        name: &'a str,
        data: &'a [u8],
        made_by: u16,
        attr: u32,
    }

    fn file<'a>(name: &'a str, data: &'a [u8]) -> ZipEntry<'a> {
        ZipEntry {
            name,
            data,
            made_by: 20,
            attr: 0,
        }
    }

    fn dir(name: &str) -> ZipEntry<'_> {
        ZipEntry {
            name,
            data: b"",
            made_by: 20,
            attr: 0x10,
        }
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut c = !0u32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    (c >> 1) ^ 0xEDB8_8320
                } else {
                    c >> 1
                };
            }
        }
        !c
    }

    fn zip(entries: &[ZipEntry]) -> Vec<u8> {
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for e in entries {
            let offset = out.len() as u32;
            let (crc, size, name) = (crc32(e.data), e.data.len() as u32, e.name.as_bytes());
            // version 20, UTF-8 names, stored, 1980-01-01 00:00.
            let common = |v: &mut Vec<u8>| {
                for x in [20u16, 0x0800, 0, 0, 0x21] {
                    v.extend(x.to_le_bytes());
                }
                for x in [crc, size, size] {
                    v.extend(x.to_le_bytes());
                }
                v.extend((name.len() as u16).to_le_bytes());
                v.extend(0u16.to_le_bytes());
            };
            out.extend(0x0403_4b50u32.to_le_bytes());
            common(&mut out);
            out.extend(name);
            out.extend(e.data);
            central.extend(0x0201_4b50u32.to_le_bytes());
            central.extend(e.made_by.to_le_bytes());
            common(&mut central);
            for x in [0u16, 0, 0] {
                central.extend(x.to_le_bytes());
            }
            central.extend(e.attr.to_le_bytes());
            central.extend(offset.to_le_bytes());
            central.extend(name);
        }
        let (cd_offset, cd_size, n) =
            (out.len() as u32, central.len() as u32, entries.len() as u16);
        out.extend(central);
        out.extend(0x0605_4b50u32.to_le_bytes());
        for x in [0u16, 0, n, n] {
            out.extend(x.to_le_bytes());
        }
        out.extend(cd_size.to_le_bytes());
        out.extend(cd_offset.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    fn tar(entries: &[(&str, u8, &str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, kind, link, data) in entries {
            let mut h = [0u8; 512];
            let mut put = |at: usize, s: &[u8]| h[at..at + s.len()].copy_from_slice(s);
            put(0, name.as_bytes());
            put(100, b"0000644\0");
            put(108, b"0000000\0");
            put(116, b"0000000\0");
            put(124, format!("{:011o}\0", data.len()).as_bytes());
            put(136, b"00000000000\0");
            put(148, b"        ");
            put(156, &[*kind]);
            put(157, link.as_bytes());
            put(257, b"ustar\0");
            put(263, b"00");
            let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
            h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
            out.extend(h);
            out.extend(*data);
            out.resize(out.len().div_ceil(512) * 512, 0);
        }
        out.resize(out.len() + 1024, 0);
        out
    }

    fn run(
        archive: &[u8],
        kind: Kind,
        inv: &ExpectedInventory,
    ) -> (
        tempfile::TempDir,
        TransactionPaths,
        Result<(), LauncherError>,
    ) {
        let (dir, _guard, paths) = disposable_root();
        fs::write(paths.archive(), archive).unwrap();
        let result = extract_kind(&bundled(), kind, inv, &paths, &AtomicBool::new(false));
        (dir, paths, result)
    }

    #[test]
    fn release_archive_extracts_into_staging_matching_inventory() {
        let inv = inventory(
            &[
                ("Data/global/excel/a.txt", b"alpha"),
                ("ünï/файл.txt", b"beta"),
                ("top.dll", b""),
            ],
            &["Saves"],
        );
        let archive = zip(&[
            dir("Data/"),
            dir("Data/global/"),
            file("Data/global/excel/a.txt", b"alpha"),
            file("ünï/файл.txt", b"beta"),
            file("top.dll", b""),
            dir("Saves/"),
        ]);
        let (dir, paths, result) = run(&archive, Kind::Zip, &inv);
        result.unwrap();
        verify_staging(&paths, &inv, &AtomicBool::new(false)).unwrap();
        let live: Vec<_> = fs::read_dir(dir.path().join("Product")).unwrap().collect();
        assert_eq!(live.len(), 1, "live product root holds only the sentinel");
    }

    #[test]
    fn release_archive_in_7z_format_extracts() {
        let src = tempfile::tempdir().unwrap();
        fs::create_dir_all(src.path().join("mod/data")).unwrap();
        fs::write(src.path().join("mod/data/x.txt"), b"seven").unwrap();
        let (dir, _guard, paths) = disposable_root();
        let status = Command::new(&bundled().exe)
            .current_dir(src.path())
            .args(["a", "-t7z", "-bso0", "-bsp0"])
            .arg(paths.archive())
            .arg("mod")
            .status()
            .unwrap();
        assert!(status.success());
        let inv = inventory(&[("mod/data/x.txt", b"seven")], &[]);
        extract(
            &bundled(),
            ArchiveFormat::SevenZip,
            &inv,
            &paths,
            &AtomicBool::new(false),
        )
        .unwrap();
        verify_staging(&paths, &inv, &AtomicBool::new(false)).unwrap();
        drop(dir);
    }

    #[test]
    fn malicious_release_archive_entries_are_rejected_before_extraction() {
        let symlink = ZipEntry {
            name: "link.txt",
            data: b"../../evil",
            made_by: 3 << 8 | 20,
            attr: 0o120_777 << 16,
        };
        let reparse = ZipEntry {
            name: "link.txt",
            data: b"x",
            made_by: 20,
            attr: 0x400,
        };
        let cases: Vec<(&str, Vec<ZipEntry>, &str)> = vec![
            ("traversal", vec![file("../up.txt", b"x")], "DotComponent"),
            (
                "nested traversal",
                vec![file("ok/../../up.txt", b"x")],
                "DotComponent",
            ),
            (
                "backslash traversal",
                vec![file("..\\up.txt", b"x")],
                "DotComponent",
            ),
            ("absolute", vec![file("/abs.txt", b"x")], "Absolute"),
            ("drive absolute", vec![file("C:/drive.txt", b"x")], "Colon"),
            ("drive relative", vec![file("C:rel.txt", b"x")], "Colon"),
            (
                "alternate data stream",
                vec![file("ok.txt:stream", b"x")],
                "Colon",
            ),
            (
                "reserved device name",
                vec![file("CON.txt", b"x")],
                "ReservedName",
            ),
            (
                "reserved device dir",
                vec![file("aux/a.txt", b"x")],
                "ReservedName",
            ),
            (
                "trailing dot",
                vec![file("dot.", b"x")],
                "TrailingDotOrSpace",
            ),
            (
                "trailing space",
                vec![file("space ", b"x")],
                "TrailingDotOrSpace",
            ),
            (
                "case-conflicting duplicate",
                vec![file("OK.TXT", b"x")],
                "duplicate",
            ),
            ("exact duplicate", vec![file("ok.txt", b"x")], "duplicate"),
            (
                "case-conflicting parent",
                vec![file("Data/a.txt", b"x"), file("data/b.txt", b"x")],
                "differs only in case",
            ),
            ("symlink", vec![symlink], "link or special"),
            ("reparse point", vec![reparse], "reparse point"),
            (
                "not in the file list",
                vec![file("extra.txt", b"x")],
                "not in the file list",
            ),
            (
                "directory not in the file list",
                vec![dir("stray/")],
                "not in the file list",
            ),
        ];
        let inv = inventory(
            &[("ok.txt", b"x"), ("Data/a.txt", b"x"), ("link.txt", b"x")],
            &[],
        );
        for (class, extra, why) in cases {
            let mut entries = vec![file("ok.txt", b"x")];
            entries.extend(extra);
            let (_dir, paths, result) = run(&zip(&entries), Kind::Zip, &inv);
            let err = result.expect_err(class);
            assert_eq!(err.code, ErrorCode::UnsafeArchiveEntry, "{class}: {err}");
            assert!(err.message.contains(why), "{class}: {err}");
            assert!(!paths.staging().exists(), "{class}: nothing written");
        }
    }

    #[test]
    fn release_archive_hard_and_symbolic_links_are_rejected() {
        let inv = inventory(&[("f.txt", b"x"), ("l.txt", b"")], &[]);
        for kind in [b'1', b'2'] {
            let archive = tar(&[("f.txt", b'0', "", b"x"), ("l.txt", kind, "f.txt", b"")]);
            let (_dir, paths, result) = run(&archive, Kind::Tar, &inv);
            let err = result.unwrap_err();
            assert_eq!(err.code, ErrorCode::UnsafeArchiveEntry, "{err}");
            assert!(err.message.contains("link entry"), "{err}");
            assert!(!paths.staging().exists());
        }
    }

    #[test]
    fn release_archive_entry_with_wrong_declared_size_is_rejected() {
        let inv = inventory(&[("ok.txt", b"x")], &[]);
        let (_dir, paths, result) = run(&zip(&[file("ok.txt", b"xx")]), Kind::Zip, &inv);
        assert_eq!(result.unwrap_err().code, ErrorCode::InventoryMismatch);
        assert!(!paths.staging().exists());
    }

    #[test]
    fn missing_file_after_extraction_fails_inventory_verification() {
        let inv = inventory(&[("a.txt", b"a"), ("b.txt", b"b")], &["empty"]);
        let (_dir, paths, result) = run(&zip(&[file("a.txt", b"a")]), Kind::Zip, &inv);
        result.unwrap();
        let err = verify_staging(&paths, &inv, &AtomicBool::new(false)).unwrap_err();
        assert_eq!(err.code, ErrorCode::InventoryMismatch);
        assert!(err.message.contains("b.txt is missing"), "{err}");

        fs::write(paths.staging().join("b.txt"), b"b").unwrap();
        let err = verify_staging(&paths, &inv, &AtomicBool::new(false)).unwrap_err();
        assert!(err.message.contains("required directory empty"), "{err}");
    }

    #[test]
    fn extra_or_differing_files_after_extraction_fail_inventory_verification() {
        let inv = inventory(&[("a.txt", b"a")], &[]);
        let (_dir, paths, result) = run(&zip(&[file("a.txt", b"a")]), Kind::Zip, &inv);
        result.unwrap();
        let ok = || verify_staging(&paths, &inv, &AtomicBool::new(false));
        ok().unwrap();

        fs::write(paths.staging().join("extra.txt"), b"?").unwrap();
        assert!(ok()
            .unwrap_err()
            .message
            .contains("unexpected file extra.txt"));
        fs::remove_file(paths.staging().join("extra.txt")).unwrap();

        fs::create_dir(paths.staging().join("stray")).unwrap();
        assert!(ok()
            .unwrap_err()
            .message
            .contains("unexpected directory stray"));
        fs::remove_dir(paths.staging().join("stray")).unwrap();

        fs::write(paths.staging().join("a.txt"), b"b").unwrap();
        let err = ok().unwrap_err();
        assert_eq!(err.code, ErrorCode::InventoryMismatch);
        assert!(err.message.contains("differs"), "{err}");

        fs::remove_file(paths.staging().join("a.txt")).unwrap();
        fs::write(paths.staging().join("A.TXT"), b"a").unwrap();
        assert!(
            ok().unwrap_err().message.contains("a.txt is missing"),
            "case must match exactly"
        );
    }

    #[test]
    fn release_archive_extraction_cancels_and_leaves_no_staging() {
        let inv = inventory(&[("a.txt", b"a")], &[]);
        let (_dir, _guard, paths) = disposable_root();
        fs::write(paths.archive(), zip(&[file("a.txt", b"a")])).unwrap();
        let err =
            extract_kind(&bundled(), Kind::Zip, &inv, &paths, &AtomicBool::new(true)).unwrap_err();
        assert_eq!(err.code, ErrorCode::Cancelled);
        assert!(!paths.staging().exists());
    }

    #[test]
    fn existing_staging_is_never_reused() {
        let inv = inventory(&[("a.txt", b"a")], &[]);
        let (_dir, _guard, paths) = disposable_root();
        fs::write(paths.archive(), zip(&[file("a.txt", b"a")])).unwrap();
        fs::create_dir(paths.staging()).unwrap();
        fs::write(paths.staging().join("left.txt"), b"old").unwrap();
        let err =
            extract_kind(&bundled(), Kind::Zip, &inv, &paths, &AtomicBool::new(false)).unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(
            paths.staging().join("left.txt").exists(),
            "not removed either"
        );
    }

    #[test]
    fn bundled_7zip_not_matching_the_pin_is_refused() {
        let exe = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/7zip/7za.exe");
        let dir = tempfile::tempdir().unwrap();
        let copy = dir.path().join("7za.exe");
        let mut bytes = fs::read(exe).unwrap();
        bytes[5000] ^= 1;
        fs::write(&copy, bytes).unwrap();
        assert_eq!(
            SevenZip::bundled(&copy).unwrap_err().code,
            ErrorCode::Internal
        );
    }

    #[test]
    fn bundled_7zip_changed_after_locating_is_refused_before_use() {
        let exe = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/7zip/7za.exe");
        let dir = tempfile::tempdir().unwrap();
        let copy = dir.path().join("7za.exe");
        fs::copy(exe, &copy).unwrap();
        let seven_zip = SevenZip::bundled(&copy).unwrap();
        let mut bytes = fs::read(&copy).unwrap();
        bytes[5000] ^= 1;
        fs::write(&copy, bytes).unwrap();

        let inv = inventory(&[("a.txt", b"a")], &[]);
        let (_root, _guard, paths) = disposable_root();
        fs::write(paths.archive(), zip(&[file("a.txt", b"a")])).unwrap();
        let err =
            extract_kind(&seven_zip, Kind::Zip, &inv, &paths, &AtomicBool::new(false)).unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(err.message.contains("does not match the pinned"), "{err}");
        assert!(!paths.staging().exists());
    }

    #[cfg(windows)]
    #[test]
    fn junction_in_staging_fails_inventory_verification() {
        let inv = inventory(&[("a.txt", b"a")], &[]);
        let (dir, paths, result) = run(&zip(&[file("a.txt", b"a")]), Kind::Zip, &inv);
        result.unwrap();
        // A junction needs no privilege, unlike a symlink. Its target is outside staging.
        let outside = dir.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let status = Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(paths.staging().join("j"))
            .arg(&outside)
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");
        let err = verify_staging(&paths, &inv, &AtomicBool::new(false)).unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsafeArchiveEntry, "{err}");
        assert!(
            err.message.contains("j is a link or reparse point"),
            "{err}"
        );
    }
}
