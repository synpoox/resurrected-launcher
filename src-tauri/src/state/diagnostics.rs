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
    /// Whether the Run value exists: Start with Windows stores nothing else.
    pub start_with_windows: bool,
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
    /// 8.3 short-name prefixes (`CHRIST` of `CHRIST~1`), matched with any `~N`.
    short_prefixes: Vec<Vec<char>>,
}

impl Redactor {
    /// Redacts each non-empty name, whole-word and case-insensitive, and the 8.3
    /// short-name form Windows derives from it (`Christopher` → `CHRIST~1`).
    pub fn new(names: impl IntoIterator<Item = String>) -> Self {
        let mut names: Vec<Vec<char>> = names
            .into_iter()
            .map(|n| n.trim().chars().collect())
            .filter(|n: &Vec<char>| !n.is_empty())
            .collect();
        // Longest first, so "micah.PC" is redacted whole before "micah".
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        let mut short_prefixes: Vec<Vec<char>> = names
            .iter()
            .map(|n| {
                n.iter()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .take(6)
                    .map(|c| c.to_ascii_uppercase())
                    .collect()
            })
            .filter(|p: &Vec<char>| !p.is_empty())
            .collect();
        short_prefixes.sort_by_key(|n| std::cmp::Reverse(n.len()));
        short_prefixes.dedup();
        Self {
            names,
            short_prefixes,
        }
    }

    /// The current user's name, profile folder name and that folder's actual 8.3
    /// short name (they can all differ).
    pub fn from_env() -> Self {
        let profile = std::env::var_os("USERPROFILE");
        let leaf = profile.as_ref().and_then(|p| {
            Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        });
        let short = profile.as_deref().and_then(short_leaf_name);
        Self::new(
            std::env::var("USERNAME")
                .ok()
                .into_iter()
                .chain(leaf)
                .chain(short),
        )
    }

    pub fn redact(&self, text: &str) -> String {
        let text: String = text
            .split_inclusive('\n')
            .map(|line| redact_secrets(&redact_url_userinfo(line)))
            .collect();
        let text = self.names.iter().fold(text, |text, name| {
            replace_words(&text, |t, i| {
                let end = i + name.len();
                (end <= t.len() && eq_fold(&t[i..end], name)).then_some(end)
            })
        });
        self.short_prefixes.iter().fold(text, |text, prefix| {
            replace_words(&text, |t, i| {
                let mut end = i + prefix.len();
                if end >= t.len() || !eq_fold(&t[i..end], prefix) || t[end] != '~' {
                    return None;
                }
                end += 1;
                let digits = t[end..].iter().take_while(|c| c.is_ascii_digit()).count();
                (digits > 0).then_some(end + digits)
            })
        })
    }
}

/// The 8.3 short name of `path`'s last component, from Windows itself.
#[cfg(windows)]
fn short_leaf_name(path: &std::ffi::OsStr) -> Option<String> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    #[link(name = "kernel32")]
    extern "system" {
        fn GetShortPathNameW(long: *const u16, short: *mut u16, len: u32) -> u32;
    }
    let wide: Vec<u16> = path.encode_wide().chain([0]).collect();
    let mut buf = vec![0u16; 1024];
    // SAFETY: `wide` is NUL-terminated and `buf` has room for `buf.len()` u16s.
    let n = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    let n = n as usize;
    if n == 0 || n >= buf.len() {
        return None;
    }
    let short = std::ffi::OsString::from_wide(&buf[..n]);
    Path::new(&short)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
}

#[cfg(not(windows))]
fn short_leaf_name(_: &std::ffi::OsStr) -> Option<String> {
    None
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn eq_fold(a: &[char], b: &[char]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.to_lowercase().eq(y.to_lowercase()))
}

/// Replaces with `<you>` every whole-word match; `matches(text, i)` returns the end
/// of a match starting at `i`. Over-redacts a user name that is also a common word,
/// which is the safe direction.
fn replace_words(text: &str, matches: impl Fn(&[char], usize) -> Option<usize>) -> String {
    let t: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < t.len() {
        let hit = (i == 0 || !is_word(t[i - 1]))
            .then(|| matches(&t, i))
            .flatten()
            .filter(|&end| end == t.len() || !is_word(t[end]));
        if let Some(end) = hit {
            out.push_str("<you>");
            i = end;
        } else {
            out.push(t[i]);
            i += 1;
        }
    }
    out
}

/// Longer keys first, so `authorization` wins over `auth`.
const SECRET_KEYS: &[&str] = &[
    "authorization",
    "credential",
    "signature",
    "password",
    "api_key",
    "api-key",
    "session",
    "passwd",
    "secret",
    "apikey",
    "cookie",
    "bearer",
    "token",
    "auth",
    "pwd",
    "sig",
];

fn value_end(b: &[u8], mut j: usize) -> usize {
    while j < b.len() && !b" \t\r\n\"'&,;)}".contains(&b[j]) {
        j += 1;
    }
    j
}

/// `token=abc`, `"password": "a b"`, `Authorization: Bearer abc`, `Bearer abc`,
/// `token%3Dabc` → the value becomes `<redacted>`.
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
        let key = SECRET_KEYS.iter().find(|k| {
            l[i..].starts_with(k.as_bytes())
                // Short keys only at a word start, so `design=` is left alone.
                && (k.len() > 4 || i == 0 || !b[i - 1].is_ascii_alphanumeric())
        });
        let Some(key) = key else {
            i += 1;
            continue;
        };
        let mut j = i + key.len();
        let value = if *key == "bearer" {
            let start = skip(j, b" ");
            (start > j).then_some((start, value_end(b, start)))
        } else {
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b"_-".contains(&b[j])) {
                j += 1;
            }
            j = skip(j, b"\\\"' ");
            let sep = if j < b.len() && b"=:".contains(&b[j]) {
                Some(j + 1)
            } else if l[j..].starts_with(b"%3d") || l[j..].starts_with(b"%3a") {
                Some(j + 3)
            } else {
                None
            };
            sep.map(|s| {
                let s = skip(s, b" \\");
                if s < b.len() && b"\"'".contains(&b[s]) {
                    // Quoted: up to the closing quote, spaces included.
                    let start = s + 1;
                    let mut end = b[start..]
                        .iter()
                        .position(|&c| c == b[s])
                        .map_or(b.len(), |p| start + p);
                    if end > start && b[end - 1] == b'\\' {
                        end -= 1;
                    }
                    (start, end)
                } else {
                    let end = value_end(b, s);
                    if matches!(&l[s..end], b"bearer" | b"basic") {
                        (s, value_end(b, skip(end, b" ")))
                    } else {
                        (s, end)
                    }
                }
            })
        };
        match value {
            Some((start, end)) if end > start => {
                out.push_str(&line[copied..start]);
                out.push_str("<redacted>");
                copied = end;
                i = end;
            }
            _ => i += key.len(),
        }
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
    let start = f.metadata()?.len().saturating_sub(LOG_TAIL_BYTES);
    f.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    f.take(LOG_TAIL_BYTES).read_to_end(&mut bytes)?;
    if start > 0 {
        // Start on a whole line, never mid-line or mid-character.
        let first = bytes.iter().position(|&c| c == b'\n').map_or(0, |p| p + 1);
        bytes.drain(..first);
    }
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
        let _ = writeln!(t, "Start with Windows: {}", on_off(ctx.start_with_windows));
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

impl StateStore {
    /// "Copy log": the end of the launcher log, capped like the report's log
    /// section and redacted the same way. Empty when there is no log yet.
    pub fn launcher_log_excerpt(&self, redactor: &Redactor) -> Result<String, std::io::Error> {
        match read_log_tail(&self.log_path()) {
            Ok(log) => Ok(redactor.redact(&log)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e),
        }
    }
}

impl DiagnosticsReport {
    /// "Save as file…": writes the reviewed text to the path the player chose in
    /// the native save dialog.
    pub fn save_as(&self, path: &Path) -> Result<(), LauncherError> {
        file::write_export(path, self.text.as_bytes())
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
            (
                r#"secret: "two words" next"#,
                r#"secret: "<redacted>" next"#,
            ),
            ("sent Bearer xyz.1 ok", "sent Bearer <redacted> ok"),
            (
                "sig=abc&signature=def",
                "sig=<redacted>&signature=<redacted>",
            ),
            (
                "pwd: p session=s auth=a",
                "pwd: <redacted> session=<redacted> auth=<redacted>",
            ),
            ("design=ok", "design=ok"),
            ("q=1&token%3Dabc", "q=1&token%3D<redacted>"),
            (r#"{\"token\":\"abc\"}"#, r#"{\"token\":\"<redacted>\"}"#),
        ] {
            assert_eq!(r.redact(input), want, "{input}");
        }
    }

    #[test]
    fn short_8_3_form_of_windows_user_name_is_replaced_by_you() {
        let r = Redactor::new(["Christopher Hall".to_owned(), "micah.DESKTOP-7".to_owned()]);
        for (input, want) in [
            (
                r"C:\Users\CHRIST~1\AppData\Local\Temp",
                r"C:\Users\<you>\AppData\Local\Temp",
            ),
            (r"C:\Users\christ~12\x", r"C:\Users\<you>\x"),
            (r"C:\Users\MICAHD~1\x", r"C:\Users\<you>\x"),
            (
                r"C:\PROGRA~1\x CHRIST~ CHRIST~1a",
                r"C:\PROGRA~1\x CHRIST~ CHRIST~1a",
            ),
        ] {
            assert_eq!(r.redact(input), want, "{input}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn actual_short_name_of_profile_folder_is_replaced_by_you() {
        let dir = tempfile::tempdir().unwrap();
        let long = dir.path().join("Long Profile Name.Folder");
        fs::create_dir(&long).unwrap();
        let short = short_leaf_name(long.as_os_str()).unwrap();
        let r = Redactor::new(["Long Profile Name.Folder".to_owned(), short.clone()]);
        assert_eq!(
            r.redact(&format!(r"C:\Users\{short}\x")),
            r"C:\Users\<you>\x"
        );
        // And the real profile: its short form never survives `from_env`.
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            if let Some(short) = short_leaf_name(&profile) {
                let text = format!(r"C:\Users\{short}\AppData");
                assert_eq!(
                    Redactor::from_env().redact(&text),
                    r"C:\Users\<you>\AppData"
                );
            }
        }
    }

    #[test]
    fn launcher_log_over_64_kib_starts_on_a_whole_line() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::open(dir.path());
        let log: String = (0..5000)
            .map(|i| format!("line {i:05} padding\n"))
            .collect();
        assert!(log.len() as u64 > LOG_TAIL_BYTES);
        fs::write(store.log_path(), &log).unwrap();
        let text = store.diagnostics_report(&ctx(), true, &micah()).text;
        let tail = text.split("[Launcher log]\n").nth(1).unwrap();
        assert!(tail.starts_with("line "), "{}", &tail[..30]);
        assert!(tail.ends_with("line 04999 padding\n"));
        assert!(!tail.contains("line 00000"));
        assert!(tail.len() as u64 <= LOG_TAIL_BYTES);
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
            start_with_windows: true,
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
        assert!(
            text.contains("Start with Windows: on"),
            "from the Run value"
        );
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
    fn copied_log_is_capped_and_redacted_like_the_report() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::open(dir.path());
        assert_eq!(store.launcher_log_excerpt(&micah()).unwrap(), "");
        let mut log: String = (0..5000).map(|i| format!("line {i:05} pad\n")).collect();
        log.push_str("C:\\Users\\micah\\x token=abc\n");
        fs::write(store.log_path(), &log).unwrap();
        let copied = store.launcher_log_excerpt(&micah()).unwrap();
        // Capped before redaction, which may lengthen a line slightly.
        assert!((copied.len() as u64) < LOG_TAIL_BYTES + 64);
        assert!(copied.starts_with("line ") && !copied.contains("line 00000"));
        assert!(
            copied.ends_with("C:\\Users\\<you>\\x token=<redacted>\n"),
            "{copied}"
        );
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
        let out = dir.path().join("diagnostics.txt");
        report.save_as(&out).unwrap();
        assert_eq!(fs::read_to_string(out).unwrap(), report.text);
    }
}
