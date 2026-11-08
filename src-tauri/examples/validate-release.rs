//! Checks a release before it is published (#56), with the launcher's own rules:
//!
//! ```text
//! cargo run --example validate-release -- <release-manifest.json> <feed.json> [<archive.7z>]
//! ```
//!
//! Parses both documents, checks the feed recommends this manifest with its exact
//! size and SHA-256, and, given the archive, checks its size and SHA-256 and that
//! the bundled 7-Zip listing is exactly `files` + `empty_dirs` with matching sizes.
//! Exit status: 0 valid, 1 invalid (each problem on stderr), 2 usage.

use std::path::Path;
use std::process::{Command, ExitCode};

use resurrected_launcher_lib::catalog::{Availability, Feed, ReleaseManifest};
use sha2::{Digest, Sha256};

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{path}: {e}"))
}

fn check(args: &[String]) -> Result<Vec<String>, String> {
    let manifest_bytes = read(&args[0])?;
    let manifest =
        ReleaseManifest::parse(&manifest_bytes).map_err(|e| format!("{}: {e}", args[0]))?;
    let feed = Feed::parse(&read(&args[1])?).map_err(|e| format!("{}: {e}", args[1]))?;
    let mut problems = Vec::new();

    match feed.product(&manifest.product_id) {
        None => problems.push(format!("feed does not list {}", manifest.product_id.0)),
        Some(entry) => {
            if let Err(e) = entry.check_manifest(&manifest) {
                problems.push(e.to_string());
            }
            if let Availability::Available(p) = &entry.availability {
                if p.manifest_size != manifest_bytes.len() as u64 {
                    problems.push(format!(
                        "feed manifest_size is {}, the manifest is {} bytes",
                        p.manifest_size,
                        manifest_bytes.len()
                    ));
                }
                if p.manifest_sha256.to_string() != hex(&manifest_bytes) {
                    problems.push("feed manifest_sha256 does not match the manifest".into());
                }
            }
        }
    }

    if let Some(archive) = args.get(2) {
        let bytes = read(archive)?;
        if bytes.len() as u64 != manifest.archive.size {
            problems.push(format!(
                "archive is {} bytes, manifest says {}",
                bytes.len(),
                manifest.archive.size
            ));
        }
        if hex(&bytes) != manifest.archive.sha256.to_string() {
            problems.push("archive SHA-256 does not match archive.sha256".into());
        }
        let listing = list(archive)?;
        let entries: Vec<(&str, Option<u64>)> =
            listing.iter().map(|(p, s)| (p.as_str(), *s)).collect();
        problems.extend(manifest.archive_mismatches(&entries));
    }
    Ok(problems)
}

/// `7za l -slt` entries as (path, size), size `None` for a directory.
fn list(archive: &str) -> Result<Vec<(String, Option<u64>)>, String> {
    let seven_zip = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/7zip/7za.exe");
    let out = Command::new(&seven_zip)
        .args(["l", "-slt", "-ba", "-t7z", "--", archive])
        .output()
        .map_err(|e| format!("running {}: {e}", seven_zip.display()))?;
    if !out.status.success() {
        return Err(format!(
            "7-Zip could not list {archive}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text =
        String::from_utf8(out.stdout).map_err(|_| "7-Zip listing is not UTF-8".to_owned())?;
    let mut entries = Vec::new();
    for block in text.replace("\r\n", "\n").split("\n\n") {
        let prop = |key: &str| {
            block
                .lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix(" = "))
        };
        let Some(path) = prop("Path") else { continue };
        let dir =
            prop("Folder") == Some("+") || prop("Attributes").is_some_and(|a| a.contains('D'));
        let size = match dir {
            true => None,
            false => Some(
                prop("Size")
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| format!("{path}: no size in the listing"))?,
            ),
        };
        entries.push((path.to_owned(), size));
    }
    Ok(entries)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !(2..=3).contains(&args.len()) {
        eprintln!("usage: validate-release <release-manifest.json> <feed.json> [<archive.7z>]");
        return ExitCode::from(2);
    }
    match check(&args) {
        Ok(problems) if problems.is_empty() => {
            println!("valid: {}", args.join(" "));
            ExitCode::SUCCESS
        }
        Ok(problems) => {
            for p in &problems {
                eprintln!("invalid: {p}");
            }
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("invalid: {e}");
            ExitCode::from(1)
        }
    }
}
