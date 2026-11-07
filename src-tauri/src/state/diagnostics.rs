//! Local diagnostics report for the Troubleshooting review dialog. The report is
//! built from typed state only, redacted, shown to the player and optionally saved
//! to a file they choose. Nothing here sends anything anywhere.

use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

use super::{file, StateStore};
use crate::contract::{LauncherError, CONTRACT_VERSION, UMBRELLA_DISPLAY_NAME};

/// Shown beside the report in the review dialog.
pub const NEVER_INCLUDED: &[&str] = &[
    "Saves, characters and stash files",
    "The contents of loot filters and game settings",
    "Passwords, tokens and other credentials",
    "Your Windows user name (shown as <you>)",
    "Files from your D2R or product folders",
    "Other programs, environment variables and browsing data",
];

/// Facts the app supplies that State does not own.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticsContext {
    pub app_version: String,
    /// For example "Windows 11 Pro 10.0.26200 x64".
    pub os: String,
    /// Recent operation errors, oldest first.
    pub recent_errors: Vec<LauncherError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticsReport {
    /// The full redacted report the player reviews before sharing.
    pub text: String,
    pub never_included: &'static [&'static str],
}

/// Only the end of the launcher log is included.
const LOG_TAIL_BYTES: u64 = 64 * 1024;

/// Replaces the Windows user name with `<you>` and strips credentials.
#[derive(Debug, Clone)]
pub struct Redactor {
    names: Vec<Vec<char>>,
}

impl Redactor {
    /// Redacts each non-empty name, whole-word and case-insensitive.
    pub fn new(names: impl IntoIterator<Item = String>) -> Self {
        let mut names: Vec<Vec<char>> = names
            .into_iter()
            .map(|n| n.trim().chars().collect())
            .filter(|n: &Vec<char>| !n.is_empty())
            .collect();
        // Longest first, so "micah.PC" is redacted whole before "micah".
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        Self { names }
    }

    /// The current user's name and profile folder name (they can differ).
    pub fn from_env() -> Self {
        let profile = std::env::var_os("USERPROFILE").and_then(|p| {
            Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        });
        Self::new(std::env::var("USERNAME").ok().into_iter().chain(profile))
    }

    pub fn redact(&self, text: &str) -> String {
        let text: String = text
            .split_inclusive('\n')
            .map(|line| redact_secrets(&redact_url_userinfo(line)))
            .collect();
        self.names
            .iter()
            .fold(text, |text, name| replace_word(&text, name, "<you>"))
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whole-word, case-insensitive replacement. Over-redacts a user name that is also a
/// common word, which is the safe direction.
fn replace_word(text: &str, word: &[char], with: &str) -> String {
    let t: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < t.len() {
        let end = i + word.len();
        let hit = end <= t.len()
            && (i == 0 || !is_word(t[i - 1]))
            && (end == t.len() || !is_word(t[end]))
            && t[i..end]
                .iter()
                .zip(word)
                .all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()));
        if hit {
            out.push_str(with);
            i = end;
        } else {
            out.push(t[i]);
            i += 1;
        }
    }
    out
}

const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "api-key",
    "apikey",
    "authorization",
    "cookie",
    "credential",
];

fn value_end(b: &[u8], mut j: usize) -> usize {
    while j < b.len() && !b" \t\r\n\"'&,;)}".contains(&b[j]) {
        j += 1;
    }
    j
}

/// `token=abc`, `"password": "abc"`, `Authorization: Bearer abc` → `<redacted>`.
fn redact_secrets(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let (b, l) = (line.as_bytes(), lower.as_bytes());
    let skip = |mut j: usize, set: &[u8]| {
        while j < b.len() && set.contains(&b[j]) {
            j += 1;
        }
        j
    };
    let mut out = String::with_capacity(line.len());
    let (mut copied, mut i) = (0, 0);
    while i < b.len() {
        let Some(key) = SECRET_KEYS
            .iter()
            .find(|k| l[i..].starts_with(k.as_bytes()))
        else {
            i += 1;
            continue;
        };
        let mut j = i + key.len();
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b"_-".contains(&b[j])) {
            j += 1;
        }
        j = skip(j, b"\"' ");
        if j < b.len() && b"=:".contains(&b[j]) {
            let start = skip(j + 1, b"\"' ");
            let mut end = value_end(b, start);
            if matches!(&l[start..end], b"bearer" | b"basic") {
                end = value_end(b, skip(end, b" "));
            }
            if end > start {
                out.push_str(&line[copied..start]);
                out.push_str("<redacted>");
                copied = end;
                i = end;
                continue;
            }
        }
        i += key.len();
    }
    out.push_str(&line[copied..]);
    out
}

/// `https://user:pass@host` → `https://<redacted>@host`.
fn redact_url_userinfo(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let auth_end = tail
            .find(|c: char| c == '/' || c.is_whitespace())
            .unwrap_or(tail.len());
        match tail[..auth_end].rfind('@') {
            Some(userinfo_end) => {
                out.push_str("<redacted>");
                rest = &tail[userinfo_end..];
            }
            None => rest = tail,
        }
    }
    out.push_str(rest);
    out
}

fn on_off(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

fn read_log_tail(path: &Path) -> Result<String, std::io::Error> {
    let mut f = fs::File::open(path)?;
    let len = f.metadata()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(LOG_TAIL_BYTES)))?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

impl StateStore {
    /// Builds the redacted report for review. The launcher log is read only when
    /// the player ticks `include_log`.
    pub fn diagnostics_report(
        &self,
        ctx: &DiagnosticsContext,
        include_log: bool,
        redactor: &Redactor,
    ) -> DiagnosticsReport {
        // Writing to a String cannot fail.
        let mut t = String::new();
        let _ = writeln!(t, "{UMBRELLA_DISPLAY_NAME} launcher diagnostics");
        let _ = writeln!(t, "Made on this PC. Nothing has been sent.\n");
        let _ = writeln!(t, "Launcher version: {}", ctx.app_version);
        let _ = writeln!(t, "Contract version: {CONTRACT_VERSION}");
        let _ = writeln!(t, "Windows: {}\n", ctx.os);

        let p = self.preferences();
        let _ = writeln!(t, "[Preferences]");
        let location = p.d2r_location.as_ref();
        let _ = writeln!(
            t,
            "D2R location: {}",
            location.map_or("not set".into(), |l| l.display().to_string())
        );
        let _ = writeln!(t, "Start with Windows: {}", on_off(p.start_with_windows));
        let _ = writeln!(
            t,
            "Download launcher updates automatically: {}",
            on_off(p.download_launcher_updates_automatically)
        );
        let last = p
            .last_product
            .as_ref()
            .map_or("not set", |id| id.0.as_str());
        let _ = writeln!(t, "Last product: {last}\n");

        let _ = writeln!(t, "[Installed releases]");
        if self.installed_releases().is_empty() {
            let _ = writeln!(t, "none");
        }
        for r in self.installed_releases() {
            let _ = writeln!(
                t,
                "{}: release {} ({}), manifest SHA-256 {}, installed at Unix time {}, {}",
                r.product_id.0,
                r.release_id,
                r.display_version,
                r.manifest_sha256,
                r.installed_at,
                if r.verified {
                    "verified"
                } else {
                    "not verified"
                }
            );
        }

        let _ = writeln!(t, "\n[Cached metadata]");
        if self.cache.is_empty() {
            let _ = writeln!(t, "none");
        }
        for e in &self.cache {
            let _ = writeln!(t, "{}: {} bytes, SHA-256 {}", e.identity, e.size, e.sha256);
        }

        let _ = writeln!(t, "\n[State file problems]");
        if self.issues().is_empty() {
            let _ = writeln!(t, "none");
        }
        for i in self.issues() {
            let aside = i.quarantined_as.as_deref().unwrap_or("left in place");
            let _ = writeln!(t, "{}: {:?}, {} ({aside})", i.file, i.problem, i.detail);
        }

        let _ = writeln!(t, "\n[Recent errors]");
        if ctx.recent_errors.is_empty() {
            let _ = writeln!(t, "none");
        }
        for e in &ctx.recent_errors {
            let _ = writeln!(t, "{e}");
        }

        let _ = writeln!(t, "\n[Launcher log]");
        if !include_log {
            let _ = writeln!(t, "not included");
        } else {
            match read_log_tail(&self.log_path()) {
                Ok(log) => t.push_str(&log),
                Err(e) => {
                    let _ = writeln!(t, "unavailable: {e}");
                }
            }
        }

        DiagnosticsReport {
            text: redactor.redact(&t),
            never_included: NEVER_INCLUDED,
        }
    }
}

impl DiagnosticsReport {
    /// "Save as file…": writes the reviewed text to the path the player chose in
    /// the native save dialog.
    pub fn save_as(&self, path: &Path) -> Result<(), LauncherError> {
        file::write_atomic(path, self.text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ErrorCode, ProductId};
    use crate::state::{InstalledRelease, Preferences};

    fn micah() -> Redactor {
        Redactor::new([
            "Micah".to_owned(),
            "micah.DESKTOP-7".to_owned(),
            " ".to_owned(),
        ])
    }

    #[test]
    fn windows_user_name_is_replaced_by_you() {
        let r = micah();
        for (input, want) in [
            (r"C:\Users\micah\Saved Games", r"C:\Users\<you>\Saved Games"),
            ("C:/Users/MICAH/AppData", "C:/Users/<you>/AppData"),
            (r"C:\Users\micah.DESKTOP-7\x", r"C:\Users\<you>\x"),
            ("user micah: ok", "user <you>: ok"),
            ("micah", "<you>"),
            (
                "micahel and xmicah and micah_2",
                "micahel and xmicah and micah_2",
            ),
        ] {
            assert_eq!(r.redact(input), want, "{input}");
        }
    }

    #[test]
    fn non_ascii_windows_user_name_is_replaced_by_you() {
        let r = Redactor::new(["Žofia Ä".to_owned()]);
        assert_eq!(r.redact(r"C:\Users\žofia ä\Saved"), r"C:\Users\<you>\Saved");
    }

    #[test]
    fn credentials_and_tokens_are_never_included() {
        let r = micah();
        for (input, want) in [
            (
                "GET /feed?token=abc123&x=1",
                "GET /feed?token=<redacted>&x=1",
            ),
            (
                r#"{"password": "hunter2", "a": 1}"#,
                r#"{"password": "<redacted>", "a": 1}"#,
            ),
            ("Authorization: Bearer eyJ.abc", "Authorization: <redacted>"),
            (
                "access_token:xyz refresh_token = q",
                "access_token:<redacted> refresh_token = <redacted>",
            ),
            ("API-KEY=k", "API-KEY=<redacted>"),
            (
                "https://u:p@example.com/x",
                "https://<redacted>@example.com/x",
            ),
            (
                "tokens are fine without a value",
                "tokens are fine without a value",
            ),
            ("ünïcödé token=ä", "ünïcödé token=<redacted>"),
        ] {
            assert_eq!(r.redact(input), want, "{input}");
        }
    }

    fn store_with_state(dir: &Path) -> StateStore {
        let mut store = StateStore::open(dir);
        store
            .set_preferences(Preferences {
                d2r_location: Some(r"C:\Users\micah\Games\Diablo II Resurrected".into()),
                last_product: Some(ProductId("pd2r".into())),
                ..Preferences::default()
            })
            .unwrap();
        store
            .mark_installed(InstalledRelease {
                product_id: ProductId("pd2r".into()),
                release_id: "pd2r-0.4.2".into(),
                display_version: "alpha 0.4.2".into(),
                manifest_sha256: "ab".repeat(32).parse().unwrap(),
                installed_at: 1_760_000_000,
                verified: true,
            })
            .unwrap();
        store
    }

    fn ctx() -> DiagnosticsContext {
        DiagnosticsContext {
            app_version: "0.1.0".into(),
            os: "Windows 11 x64".into(),
            recent_errors: vec![LauncherError::new(
                ErrorCode::AccessDenied,
                r"cannot write C:\Users\Micah\x?token=s3cret",
            )],
        }
    }

    #[test]
    fn diagnostics_report_is_redacted_and_lists_never_included() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_state(dir.path());
        let report = store.diagnostics_report(&ctx(), false, &micah());
        let text = &report.text;
        assert!(text.contains(r"D2R location: C:\Users\<you>\Games\Diablo II Resurrected"));
        assert!(text.contains("pd2r: release pd2r-0.4.2 (alpha 0.4.2)"));
        assert!(text.contains(r"AccessDenied: cannot write C:\Users\<you>\x?token=<redacted>"));
        assert!(text.contains("[Launcher log]\nnot included"));
        assert!(text.starts_with("Project Resurrected launcher diagnostics"));
        assert!(!text.to_lowercase().contains("micah"));
        assert!(!text.contains("s3cret"));
        assert!(!text.contains("PD2Resurrect"));
        assert!(report
            .never_included
            .contains(&"Passwords, tokens and other credentials"));
        assert!(report
            .never_included
            .iter()
            .any(|i| i.contains("Windows user name") && i.contains("<you>")));
    }

    #[test]
    fn launcher_log_is_included_only_when_chosen_and_redacted() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_state(dir.path());
        fs::write(store.log_path(), "started by micah\npassword=pw\n").unwrap();
        let without = store.diagnostics_report(&ctx(), false, &micah());
        assert!(!without.text.contains("started by"));
        let with = store.diagnostics_report(&ctx(), true, &micah());
        assert!(with
            .text
            .contains("started by <you>\npassword=<redacted>\n"));
    }

    #[test]
    fn missing_launcher_log_is_reported_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let report = StateStore::open(dir.path()).diagnostics_report(&ctx(), true, &micah());
        assert!(report.text.contains("[Launcher log]\nunavailable:"));
        assert!(report.text.contains("[Installed releases]\nnone"));
    }

    #[test]
    fn diagnostics_report_lists_quarantined_state_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(crate::state::PREFERENCES_FILE), "{").unwrap();
        let store = StateStore::open(dir.path());
        let text = store.diagnostics_report(&ctx(), false, &micah()).text;
        assert!(text.contains("preferences.json: Corrupt"), "{text}");
        assert!(text.contains("preferences.json.quarantined-"));
    }

    #[test]
    fn diagnostics_report_saved_as_file_is_the_reviewed_text() {
        let dir = tempfile::tempdir().unwrap();
        let report = store_with_state(dir.path()).diagnostics_report(&ctx(), false, &micah());
        let out = dir.path().join("exports").join("diagnostics.txt");
        report.save_as(&out).unwrap();
        assert_eq!(fs::read_to_string(out).unwrap(), report.text);
    }
}
