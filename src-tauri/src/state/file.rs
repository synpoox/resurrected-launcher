//! Versioned state files with atomic replacement, and quarantine of files that
//! cannot be trusted.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::contract::{ErrorCode, LauncherError};

/// Why a state file was not used. Reported to the frontend and in diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateIssue {
    /// File name inside the state directory (never a full path).
    pub file: String,
    pub problem: StateProblem,
    pub detail: String,
    /// Name the file was renamed to, so nothing is lost. `None` if it was not moved.
    pub quarantined_as: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateProblem {
    /// Not valid JSON, wrong shape, or failed validation (including a cached blob
    /// whose bytes no longer match its SHA-256).
    Corrupt,
    /// Written by another launcher version with a schema this one does not know.
    UnknownVersion,
    /// Exists but could not be read (for example access denied). Left in place.
    Unreadable,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u32,
    data: T,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn temp_path(path: &Path) -> PathBuf {
    // Fixed name: the launcher is single-instance, and the next write truncates any
    // temp file a crash left behind.
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".tmp");
    path.with_file_name(name)
}

pub(super) fn io_error(path: &Path, err: io::Error) -> LauncherError {
    let code = if err.kind() == io::ErrorKind::PermissionDenied {
        ErrorCode::AccessDenied
    } else {
        ErrorCode::Internal
    };
    LauncherError::new(code, format!("state file {}: {err}", file_name(path)))
}

/// Writes `bytes` to a temp file beside `path`, flushes it to disk, then renames it
/// over `path`, so a crash at any point leaves either the old or the new file.
pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), LauncherError> {
    write_atomic_inner(path, bytes, None).map_err(|e| io_error(path, e))
}

fn write_atomic_inner(path: &Path, bytes: &[u8], crash_after: Option<usize>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = temp_path(path);
    let mut file = File::create(&tmp)?;
    if let Some(n) = crash_after {
        // Simulated crash: part of the new content is on disk, nothing renamed.
        file.write_all(&bytes[..n.min(bytes.len())])?;
        return Err(io::Error::other("simulated crash mid-write"));
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    // std uses MoveFileExW(MOVEFILE_REPLACE_EXISTING), which replaces atomically on NTFS.
    // shortcut: the directory entry is not flushed (no MOVEFILE_WRITE_THROUGH); a power
    // loss right after rename may roll back to the old file, never to a torn one.
    fs::rename(&tmp, path)
}

/// Test hook: dies after writing `n` bytes of the temp file, as a killed process would.
#[cfg(test)]
pub(super) fn write_crashing(path: &Path, bytes: &[u8], n: usize) {
    assert!(write_atomic_inner(path, bytes, Some(n)).is_err());
}

pub(super) fn save<T: Serialize>(path: &Path, version: u32, data: &T) -> Result<(), LauncherError> {
    let json = serde_json::to_vec_pretty(&Envelope {
        schema_version: version,
        data,
    })
    .map_err(|e| LauncherError::new(ErrorCode::Internal, e.to_string()))?;
    write_atomic(path, &json)
}

/// Renames an untrusted file aside (never deletes it) and describes the problem.
pub(super) fn quarantine(path: &Path, problem: StateProblem, detail: String) -> StateIssue {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    // Never rename over an earlier quarantined file (rename replaces on Windows).
    let aside = (0..)
        .map(|n| {
            let suffix = if n == 0 {
                String::new()
            } else {
                format!("-{n}")
            };
            path.with_file_name(format!("{}.quarantined-{millis}{suffix}", file_name(path)))
        })
        .find(|p| !p.exists())
        .expect("unbounded range");
    let (quarantined_as, detail) = match fs::rename(path, &aside) {
        Ok(()) => (Some(file_name(&aside)), detail),
        Err(e) => (None, format!("{detail}; could not quarantine: {e}")),
    };
    StateIssue {
        file: file_name(path),
        problem,
        detail,
        quarantined_as,
    }
}

/// Loads a versioned state file. `Ok(None)` when it does not exist. Anything else
/// that is not a valid file of exactly `version` is quarantined (or, if unreadable,
/// left in place) and returned as an issue; it never panics.
pub(super) fn load<T: DeserializeOwned>(
    path: &Path,
    version: u32,
    validate: impl FnOnce(&T) -> Result<(), String>,
) -> Result<Option<T>, StateIssue> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(StateIssue {
                file: file_name(path),
                problem: StateProblem::Unreadable,
                detail: e.to_string(),
                quarantined_as: None,
            })
        }
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| quarantine(path, StateProblem::Corrupt, e.to_string()))?;
    match value.get("schema_version").and_then(|v| v.as_u64()) {
        Some(v) if v == u64::from(version) => {}
        Some(v) => {
            let detail = format!("schema version {v}, this launcher reads {version}");
            return Err(quarantine(path, StateProblem::UnknownVersion, detail));
        }
        None => {
            let detail = "missing schema_version".to_owned();
            return Err(quarantine(path, StateProblem::Corrupt, detail));
        }
    }
    let envelope: Envelope<T> = serde_json::from_value(value)
        .map_err(|e| quarantine(path, StateProblem::Corrupt, e.to_string()))?;
    validate(&envelope.data).map_err(|e| quarantine(path, StateProblem::Corrupt, e))?;
    Ok(Some(envelope.data))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_check(_: &Vec<u32>) -> Result<(), String> {
        Ok(())
    }

    #[test]
    fn crash_mid_write_leaves_previous_state_file_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        save(&path, 1, &vec![1u32, 2]).unwrap();
        let new = serde_json::to_vec(&Envelope {
            schema_version: 1,
            data: vec![3u32],
        })
        .unwrap();
        for n in [0, 5, new.len()] {
            write_crashing(&path, &new, n);
            assert_eq!(load(&path, 1, no_check).unwrap(), Some(vec![1, 2]));
        }
        assert!(temp_path(&path).exists(), "crash leaves its temp file");
        save(&path, 1, &vec![3u32]).unwrap();
        assert_eq!(load(&path, 1, no_check).unwrap(), Some(vec![3]));
        assert!(!temp_path(&path).exists());
    }

    #[test]
    fn missing_state_file_is_none_not_an_issue() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("x.json"), 1, no_check), Ok(None));
    }

    #[test]
    fn corrupt_and_unknown_version_state_files_are_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        let cases: [(&[u8], StateProblem); 5] = [
            (b"{\"schema_version\":1,\"data\":[1,", StateProblem::Corrupt),
            (b"{\"data\":[1]}", StateProblem::Corrupt),
            (
                b"{\"schema_version\":1,\"data\":\"x\"}",
                StateProblem::Corrupt,
            ),
            (
                b"{\"schema_version\":1,\"data\":[1],\"x\":0}",
                StateProblem::Corrupt,
            ),
            (
                b"{\"schema_version\":2,\"data\":[1]}",
                StateProblem::UnknownVersion,
            ),
        ];
        for (bytes, problem) in cases {
            fs::write(&path, bytes).unwrap();
            let issue = load::<Vec<u32>>(&path, 1, no_check).unwrap_err();
            assert_eq!(issue.problem, problem);
            assert!(!path.exists());
            let aside = dir.path().join(issue.quarantined_as.unwrap());
            assert_eq!(
                fs::read(aside).unwrap(),
                bytes,
                "quarantine keeps the bytes"
            );
        }
    }

    #[test]
    fn failed_validation_quarantines_the_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        save(&path, 1, &vec![7u32]).unwrap();
        let issue = load::<Vec<u32>>(&path, 1, |_| Err("bad".into())).unwrap_err();
        assert_eq!(
            (issue.problem, issue.detail.as_str()),
            (StateProblem::Corrupt, "bad")
        );
    }
}
