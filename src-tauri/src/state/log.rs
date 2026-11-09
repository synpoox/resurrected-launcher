//! The launcher log (#125): `launcher.log` in app-local data, next to the state
//! files, the file the diagnostics report's optional launcher-log section reads.
//!
//! A release build has no console (`windows_subsystem = "windows"`), so stderr goes
//! nowhere: anything worth keeping after the fact is appended here. Each line is
//! `[<unix seconds>] <text>`. Writing never fails the caller. Past
//! [`MAX_LOG_BYTES`] the file is renamed to `launcher.log.1` (replacing an older
//! one) and a new one starts, so it never grows without bound.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// The log's file name in app-local data.
pub const LOG_FILE: &str = "launcher.log";
/// The previous log, kept after rotation.
pub const OLD_LOG_FILE: &str = "launcher.log.1";
/// The size at which the log is rotated before the next line.
pub const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// Appends one line to `dir/launcher.log` (and stderr, for a debug console),
/// creating `dir` when missing. Errors are dropped: logging is best effort.
pub fn append(dir: &Path, line: &str) {
    eprintln!("{line}");
    let _ = try_append(dir, line);
}

fn try_append(dir: &Path, line: &str) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join(LOG_FILE);
    if fs::metadata(&path).is_ok_and(|m| m.len() >= MAX_LOG_BYTES) {
        let old = dir.join(OLD_LOG_FILE);
        let _ = fs::remove_file(&old);
        fs::rename(&path, &old)?;
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // One write per line, so lines from two threads never interleave mid-line.
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(format!("[{secs}] {line}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_appended_with_a_timestamp_and_the_dir_is_created() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("not-yet-created");
        append(&dir, "one");
        append(&dir, "two");
        let text = fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(
            lines[0].starts_with('[') && lines[0].ends_with("] one"),
            "{text}"
        );
        assert!(lines[1].ends_with("] two"), "{text}");
    }

    #[test]
    fn a_full_log_is_rotated_to_launcher_log_1() {
        let tmp = tempfile::tempdir().unwrap();
        let full = "x".repeat(MAX_LOG_BYTES as usize);
        fs::write(tmp.path().join(LOG_FILE), &full).unwrap();
        fs::write(tmp.path().join(OLD_LOG_FILE), "older").unwrap();
        append(tmp.path(), "fresh");
        assert_eq!(
            fs::read_to_string(tmp.path().join(OLD_LOG_FILE)).unwrap(),
            full
        );
        let text = fs::read_to_string(tmp.path().join(LOG_FILE)).unwrap();
        assert!(text.ends_with("] fresh\n") && text.len() < 64, "{text}");
    }

    #[test]
    fn an_unwritable_log_does_not_panic() {
        let tmp = tempfile::tempdir().unwrap();
        // A directory where the file should be: the open fails, quietly.
        fs::create_dir(tmp.path().join(LOG_FILE)).unwrap();
        append(tmp.path(), "dropped");
    }
}
