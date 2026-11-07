//! Release archive download: HTTPS only (every redirect hop too), into a partial
//! file beside the product root, with size and SHA-256 verification before the
//! archive takes its final name (#6 step 2).
//!
//! An incomplete download restarts from zero; a complete archive found on disk is
//! reused only after it is re-verified. Resume of partial downloads is deferred.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};
use ureq::http::Uri;
use ureq::tls::TlsConfig;

use super::paths::TransactionPaths;
use crate::catalog::Archive;
use crate::contract::{ErrorCode, LauncherError, Sha256Digest};

const MAX_REDIRECTS: usize = 10;
const CHUNK: usize = 64 * 1024;

/// How [`Downloader::fetch`] obtained the verified archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetched {
    Downloaded,
    /// A complete archive was already on disk and re-verified.
    Reused,
}

fn failed(message: impl Into<String>) -> LauncherError {
    LauncherError::new(ErrorCode::DownloadFailed, message)
}

fn mismatch(message: impl Into<String>) -> LauncherError {
    LauncherError::new(ErrorCode::VerificationFailed, message)
}

pub(crate) use super::journal::io_error;

fn cancelled() -> LauncherError {
    LauncherError::new(ErrorCode::Cancelled, "cancelled by the user")
}

/// Copies `src` to `sink` (if any) while hashing, checking `cancel` before every
/// read and failing as soon as more than `limit` bytes arrive.
fn pump(
    src: &mut dyn Read,
    mut sink: Option<&mut File>,
    limit: u64,
    read_failed: &dyn Fn(std::io::Error) -> LauncherError,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(u64, [u8; 32]), LauncherError> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0; CHUNK];
    let mut done = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let n = match src.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(read_failed(e)),
        };
        done += n as u64;
        if done > limit {
            return Err(mismatch(format!("more than the expected {limit} bytes")));
        }
        hasher.update(&buf[..n]);
        if let Some(file) = sink.as_deref_mut() {
            file.write_all(&buf[..n])
                .map_err(|e| failed(format!("writing the download: {e}")))?;
        }
        progress(done, limit);
    }
    Ok((done, hasher.finalize().into()))
}

/// Size and SHA-256 of a file on disk.
pub(crate) fn hash_file(
    path: &Path,
    cancel: &AtomicBool,
) -> Result<(u64, [u8; 32]), LauncherError> {
    let mut file = File::open(path).map_err(|e| io_error("opening", path, e))?;
    let read_failed = |e| io_error("reading", path, e);
    pump(
        &mut file,
        None,
        u64::MAX,
        &read_failed,
        cancel,
        &mut |_, _| {},
    )
}

/// Whether `path` is a regular file with exactly this size and SHA-256.
pub(crate) fn verify_file(
    path: &Path,
    size: u64,
    sha256: &Sha256Digest,
    cancel: &AtomicBool,
) -> Result<bool, LauncherError> {
    let meta = fs::symlink_metadata(path).map_err(|e| io_error("reading", path, e))?;
    if !meta.is_file() || meta.len() != size {
        return Ok(false);
    }
    let (n, digest) = hash_file(path, cancel)?;
    Ok(n == size && &digest == sha256.as_bytes())
}

fn require_https(url: &str) -> Result<(), LauncherError> {
    // shortcut: a relative redirect Location is rejected too; resolve it against the
    // request URL if a real host ever sends one.
    let uri: Uri = url.parse().map_err(|_| failed("malformed download URL"))?;
    if uri.scheme_str() == Some("https") && uri.host().is_some_and(|h| !h.is_empty()) {
        Ok(())
    } else {
        Err(failed("download URL or redirect is not absolute HTTPS"))
    }
}

/// Downloads release archives. The only network client in Install; it takes no
/// URL from the frontend, only the [`Archive`] of a validated release manifest.
pub struct Downloader {
    agent: ureq::Agent,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloader {
    /// A downloader trusting the bundled WebPKI roots.
    pub fn new() -> Self {
        Self::with_tls(TlsConfig::default())
    }

    /// A downloader with its own TLS trust; integration tests pass the fixture
    /// server's certificate. Not for production use.
    #[doc(hidden)]
    pub fn with_tls(tls: TlsConfig) -> Self {
        // shortcut: cancel is checked between reads, so a stalled read waits for the
        // server or the connection to drop; add an idle-read timeout if that bites.
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            // Redirects are followed by hand so every hop is checked for HTTPS.
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .tls_config(tls)
            .build()
            .into();
        Self { agent }
    }

    fn get(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, LauncherError> {
        let mut url = url.to_owned();
        for _ in 0..=MAX_REDIRECTS {
            require_https(&url)?;
            let response = self
                .agent
                .get(&url)
                .call()
                .map_err(|e| failed(format!("request failed: {e}")))?;
            let status = response.status();
            if status.is_redirection() {
                url = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| failed("redirect without a Location"))?
                    .to_owned();
                continue;
            }
            if status != 200 {
                return Err(failed(format!("server answered {status}")));
            }
            return Ok(response);
        }
        Err(failed("too many redirects"))
    }

    /// Makes [`TransactionPaths::archive`] the verified archive for `source`.
    ///
    /// `progress(done, total)` reports bytes as they arrive; setting `cancel` stops
    /// before the next read with [`ErrorCode::Cancelled`]. On any error the partial
    /// file is removed and no archive is left at the final path.
    ///
    /// Errors: [`ErrorCode::DownloadFailed`] (network, HTTP status, non-HTTPS URL or
    /// redirect, truncated body), [`ErrorCode::VerificationFailed`] (size or SHA-256),
    /// [`ErrorCode::Cancelled`], and for local I/O [`ErrorCode::AccessDenied`],
    /// [`ErrorCode::GameRunning`] (a sharing violation), [`ErrorCode::InsufficientSpace`]
    /// or [`ErrorCode::Internal`].
    pub fn fetch(
        &self,
        source: &Archive,
        paths: &TransactionPaths,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Fetched, LauncherError> {
        let dest = paths.archive();
        if fs::symlink_metadata(dest).is_ok() {
            if verify_file(dest, source.size, &source.sha256, cancel)? {
                return Ok(Fetched::Reused);
            }
            fs::remove_file(dest).map_err(|e| io_error("removing unverified", dest, e))?;
        }
        let partial = paths.download_partial();
        let expect = Expect::Exact {
            size: source.size,
            sha256: &source.sha256,
        };
        let result = self.download_to(&source.url, partial, expect, cancel, progress);
        if result.is_err() {
            let _ = fs::remove_file(partial);
        }
        result?;
        fs::rename(partial, dest).map_err(|e| io_error("renaming download to", dest, e))?;
        Ok(Fetched::Downloaded)
    }

    /// Downloads `url` (HTTPS only, every redirect hop too) to `dest`, through
    /// `<dest>.partial`, which replaces any existing `dest` only after `expect` held.
    /// For metadata and the loader installer, which have no [`TransactionPaths`].
    ///
    /// No [`crate::contract::WriteGuard`] covers `dest`: the caller must pass a path
    /// under the launcher's local data dir, never one inside a game folder.
    ///
    /// Returns the size and SHA-256 of the bytes now at `dest`. Progress, cancel and
    /// errors are as for [`Downloader::fetch`]; on any error the partial file is
    /// removed and `dest` is unchanged.
    pub fn fetch_to(
        &self,
        url: &str,
        dest: &Path,
        expect: Expect<'_>,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(u64, Sha256Digest), LauncherError> {
        let mut partial = dest.as_os_str().to_owned();
        partial.push(".partial");
        let partial = Path::new(&partial);
        let result = self
            .download_to(url, partial, expect, cancel, progress)
            .and_then(|(n, digest)| {
                let digest = Sha256Digest::from_bytes(digest)
                    .ok_or_else(|| mismatch("download hashed to the all-zero placeholder"))?;
                fs::rename(partial, dest).map_err(|e| io_error("renaming download to", dest, e))?;
                Ok((n, digest))
            });
        if result.is_err() {
            let _ = fs::remove_file(partial);
        }
        result
    }

    fn download_to(
        &self,
        url: &str,
        partial: &Path,
        expect: Expect<'_>,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(u64, [u8; 32]), LauncherError> {
        let (limit, sha256) = match expect {
            Expect::Exact { size, sha256 } => (size, Some(sha256)),
            Expect::AtMost(max) => (max, None),
        };
        let response = self.get(url)?;
        let announced = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok()?.parse::<u64>().ok());
        if let Some(len) = announced {
            if len > limit || (sha256.is_some() && len != limit) {
                return Err(mismatch(format!(
                    "server announces {len} bytes, expected {}{limit}",
                    if sha256.is_some() { "" } else { "at most " }
                )));
            }
        }
        // create truncates: an incomplete earlier download restarts from zero.
        let mut file = File::create(partial).map_err(|e| io_error("creating", partial, e))?;
        let mut body = response.into_body().into_reader();
        let read_failed = |e| failed(format!("download interrupted: {e}"));
        let (n, digest) = pump(
            &mut body,
            Some(&mut file),
            limit,
            &read_failed,
            cancel,
            progress,
        )?;
        if let Some(sha256) = sha256 {
            if n != limit {
                return Err(mismatch(format!("received {n} bytes, expected {limit}")));
            }
            if &digest != sha256.as_bytes() {
                return Err(mismatch("SHA-256 does not match the release manifest"));
            }
        }
        file.sync_all()
            .map_err(|e| io_error("flushing", partial, e))?;
        Ok((n, digest))
    }
}

/// What [`Downloader::fetch_to`] requires of the downloaded bytes.
#[derive(Debug, Clone, Copy)]
pub enum Expect<'a> {
    /// Exactly `size` bytes with this SHA-256 (a manifest, the loader installer).
    Exact { size: u64, sha256: &'a Sha256Digest },
    /// Size and hash unknown (the feed); fails as soon as more than this many bytes arrive.
    AtMost(u64),
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
    use ureq::tls::{Certificate, RootCerts};

    use super::*;
    use crate::catalog::ArchiveFormat;
    use crate::contract::{WriteGuard, DISPOSABLE_SENTINEL};

    type Handler = dyn Fn(&str, &str, &mut dyn Write) + Send + Sync;

    /// A local HTTPS fixture server with a fresh self-signed certificate for
    /// 127.0.0.1. `handler(path, base_url, out)` writes a raw HTTP response.
    struct Fixture {
        base: String,
        cert: Vec<u8>,
        hits: Arc<Mutex<Vec<String>>>,
    }

    impl Fixture {
        fn serve(handler: impl Fn(&str, &str, &mut dyn Write) + Send + Sync + 'static) -> Self {
            let handler: Arc<Handler> = Arc::new(handler);
            let key = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).unwrap();
            let cert = key.cert.der().to_vec();
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let config = Arc::new(
                rustls::ServerConfig::builder_with_provider(provider)
                    .with_safe_default_protocol_versions()
                    .unwrap()
                    .with_no_client_auth()
                    .with_single_cert(
                        vec![CertificateDer::from(cert.clone())],
                        PrivatePkcs8KeyDer::from(key.signing_key.serialize_der()).into(),
                    )
                    .unwrap(),
            );
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!(
                "https://127.0.0.1:{}",
                listener.local_addr().unwrap().port()
            );
            let hits = Arc::new(Mutex::new(Vec::new()));
            let (base2, hits2) = (base.clone(), hits.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let (config, handler) = (config.clone(), handler.clone());
                    let (base, hits) = (base2.clone(), hits2.clone());
                    std::thread::spawn(move || {
                        let conn = rustls::ServerConnection::new(config).unwrap();
                        let mut tls = rustls::StreamOwned::new(conn, stream);
                        let mut reader = BufReader::new(&mut tls);
                        let mut line = String::new();
                        if reader.read_line(&mut line).is_err() {
                            return;
                        }
                        let path = line.split(' ').nth(1).unwrap_or("").to_owned();
                        loop {
                            let mut header = String::new();
                            match reader.read_line(&mut header) {
                                Ok(0) | Err(_) => return,
                                Ok(_) if header == "\r\n" => break,
                                Ok(_) => {}
                            }
                        }
                        hits.lock().unwrap().push(path.clone());
                        handler(&path, &base, &mut tls);
                        tls.conn.send_close_notify();
                        let _ = tls.flush();
                    });
                }
            });
            Self { base, cert, hits }
        }

        fn url(&self, path: &str) -> String {
            format!("{}{path}", self.base)
        }

        fn downloader(&self) -> Downloader {
            let root = Certificate::from_der(&self.cert).to_owned();
            let tls = TlsConfig::builder()
                .root_certs(RootCerts::new_with_certs(&[root]))
                .build();
            Downloader::with_tls(tls)
        }

        fn hits(&self) -> Vec<String> {
            self.hits.lock().unwrap().clone()
        }
    }

    fn ok_response(out: &mut dyn Write, length: usize, body: &[u8]) {
        let head =
            format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n");
        let _ = out.write_all(head.as_bytes());
        for chunk in body.chunks(16 * 1024) {
            if out.write_all(chunk).is_err() {
                return;
            }
        }
    }

    fn redirect(out: &mut dyn Write, location: &str) {
        let head = format!(
            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        let _ = out.write_all(head.as_bytes());
    }

    pub(crate) fn sha256(bytes: &[u8]) -> Sha256Digest {
        Sha256Digest::from_bytes(Sha256::digest(bytes).into()).unwrap()
    }

    /// A disposable product root (with sentinel) and its transaction paths.
    pub(crate) fn disposable_root() -> (tempfile::TempDir, WriteGuard, TransactionPaths) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Product");
        fs::create_dir(&root).unwrap();
        fs::write(root.join(DISPOSABLE_SENTINEL), b"").unwrap();
        let guard = WriteGuard::check(&root).unwrap();
        let paths = TransactionPaths::of(&guard);
        (dir, guard, paths)
    }

    fn archive_bytes() -> Vec<u8> {
        (0..300_000u32).map(|i| (i % 251) as u8).collect()
    }

    fn source(url: String, bytes: &[u8]) -> Archive {
        Archive {
            url,
            format: ArchiveFormat::SevenZip,
            size: bytes.len() as u64,
            sha256: sha256(bytes),
        }
    }

    fn serve_archive(bytes: Vec<u8>) -> Fixture {
        Fixture::serve(move |path, base, out| match path {
            "/archive" => ok_response(out, bytes.len(), &bytes),
            "/moved" => redirect(out, &format!("{base}/archive")),
            _ => {
                let _ = out.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            }
        })
    }

    fn fetch(
        fixture: &Fixture,
        source: &Archive,
        paths: &TransactionPaths,
    ) -> Result<Fetched, LauncherError> {
        fixture
            .downloader()
            .fetch(source, paths, &AtomicBool::new(false), &mut |_, _| {})
    }

    fn assert_nothing_left(paths: &TransactionPaths) {
        assert!(!paths.archive().exists());
        assert!(!paths.download_partial().exists());
    }

    #[test]
    fn release_archive_downloads_through_https_redirect_with_progress() {
        let bytes = archive_bytes();
        let fixture = serve_archive(bytes.clone());
        let (_dir, _guard, paths) = disposable_root();
        let mut seen = Vec::new();
        let got = fixture
            .downloader()
            .fetch(
                &source(fixture.url("/moved"), &bytes),
                &paths,
                &AtomicBool::new(false),
                &mut |done, total| seen.push((done, total)),
            )
            .unwrap();
        assert_eq!(got, Fetched::Downloaded);
        assert_eq!(fs::read(paths.archive()).unwrap(), bytes);
        assert!(!paths.download_partial().exists());
        assert!(seen.len() > 1, "progress reported per chunk");
        assert_eq!(seen.last(), Some(&(bytes.len() as u64, bytes.len() as u64)));
        assert_eq!(fixture.hits(), ["/moved", "/archive"]);
    }

    #[test]
    fn complete_release_archive_is_reused_only_after_reverification() {
        let bytes = archive_bytes();
        let fixture = serve_archive(bytes.clone());
        let (_dir, _guard, paths) = disposable_root();
        let src = source(fixture.url("/archive"), &bytes);

        fs::write(paths.archive(), &bytes).unwrap();
        assert_eq!(fetch(&fixture, &src, &paths).unwrap(), Fetched::Reused);
        assert!(
            fixture.hits().is_empty(),
            "verified archive needs no request"
        );

        let mut damaged = bytes.clone();
        damaged[1000] ^= 1;
        fs::write(paths.archive(), &damaged).unwrap();
        assert_eq!(fetch(&fixture, &src, &paths).unwrap(), Fetched::Downloaded);
        assert_eq!(fs::read(paths.archive()).unwrap(), bytes);
    }

    #[test]
    fn incomplete_release_archive_download_restarts() {
        let bytes = archive_bytes();
        let fixture = serve_archive(bytes.clone());
        let (_dir, _guard, paths) = disposable_root();
        fs::write(
            paths.download_partial(),
            b"stale half of an earlier attempt",
        )
        .unwrap();
        let src = source(fixture.url("/archive"), &bytes);
        assert_eq!(fetch(&fixture, &src, &paths).unwrap(), Fetched::Downloaded);
        assert_eq!(fs::read(paths.archive()).unwrap(), bytes);
    }

    #[test]
    fn release_archive_with_hash_mismatch_is_rejected() {
        let bytes = archive_bytes();
        let fixture = serve_archive(bytes.clone());
        let (_dir, _guard, paths) = disposable_root();
        let mut src = source(fixture.url("/archive"), &bytes);
        src.sha256 = sha256(b"some other release");
        let err = fetch(&fixture, &src, &paths).unwrap_err();
        assert_eq!(err.code, ErrorCode::VerificationFailed, "{err}");
        assert_nothing_left(&paths);
    }

    #[test]
    fn release_archive_with_size_mismatch_is_rejected() {
        let bytes = archive_bytes();
        let fixture = serve_archive(bytes.clone());
        let (_dir, _guard, paths) = disposable_root();
        for size in [bytes.len() as u64 - 1, bytes.len() as u64 + 1] {
            let mut src = source(fixture.url("/archive"), &bytes);
            src.size = size;
            let err = fetch(&fixture, &src, &paths).unwrap_err();
            assert_eq!(err.code, ErrorCode::VerificationFailed, "{err}");
            assert_nothing_left(&paths);
        }
    }

    #[test]
    fn release_archive_overlong_body_without_length_is_rejected() {
        let bytes = archive_bytes();
        let body = bytes.clone();
        let fixture = Fixture::serve(move |_, _, out| {
            let _ = out.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
            let _ = out.write_all(&body);
        });
        let (_dir, _guard, paths) = disposable_root();
        let mut src = source(fixture.url("/archive"), &bytes);
        src.size -= 10;
        let err = fetch(&fixture, &src, &paths).unwrap_err();
        assert_eq!(err.code, ErrorCode::VerificationFailed, "{err}");
        assert_nothing_left(&paths);
    }

    #[test]
    fn truncated_release_archive_body_is_rejected() {
        let bytes = archive_bytes();
        let body = bytes.clone();
        let fixture = Fixture::serve(move |_, _, out| {
            ok_response(out, body.len(), &body[..body.len() / 2]);
        });
        let (_dir, _guard, paths) = disposable_root();
        let err = fetch(&fixture, &source(fixture.url("/archive"), &bytes), &paths).unwrap_err();
        assert_eq!(err.code, ErrorCode::DownloadFailed, "{err}");
        assert_nothing_left(&paths);
    }

    #[test]
    fn release_archive_redirect_to_http_is_rejected() {
        let bytes = archive_bytes();
        let fixture = Fixture::serve(|path, base, out| match path {
            "/archive" => redirect(out, &format!("{}/plain", base.replace("https:", "http:"))),
            _ => ok_response(out, 0, b""),
        });
        let (_dir, _guard, paths) = disposable_root();
        let err = fetch(&fixture, &source(fixture.url("/archive"), &bytes), &paths).unwrap_err();
        assert_eq!(err.code, ErrorCode::DownloadFailed, "{err}");
        assert!(err.message.contains("HTTPS"), "{err}");
        assert_eq!(
            fixture.hits(),
            ["/archive"],
            "the http hop is never requested"
        );
        assert_nothing_left(&paths);

        let plain = fixture.url("/archive").replace("https:", "http:");
        let err = fetch(&fixture, &source(plain, &bytes), &paths).unwrap_err();
        assert_eq!(err.code, ErrorCode::DownloadFailed, "{err}");
    }

    #[test]
    fn release_archive_download_cancels_mid_transfer() {
        let bytes: Vec<u8> = (0..8_000_000u32).map(|i| i as u8).collect();
        let fixture = serve_archive(bytes.clone());
        let (_dir, _guard, paths) = disposable_root();
        let cancel = AtomicBool::new(false);
        let mut reported = 0;
        let err = fixture
            .downloader()
            .fetch(
                &source(fixture.url("/archive"), &bytes),
                &paths,
                &cancel,
                &mut |done, _| {
                    reported = done;
                    cancel.store(true, Ordering::Relaxed);
                },
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Cancelled, "{err}");
        assert!(reported > 0 && reported < bytes.len() as u64);
        assert_nothing_left(&paths);
    }

    #[test]
    fn fetch_to_downloads_into_a_data_dir_file_without_transaction_paths() {
        let bytes = archive_bytes();
        let fixture = serve_archive(bytes.clone());
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("installer.exe");
        let partial = dir.path().join("installer.exe.partial");
        let fetch_to = |expect| {
            fixture.downloader().fetch_to(
                &fixture.url("/moved"),
                &dest,
                expect,
                &AtomicBool::new(false),
                &mut |_, _| {},
            )
        };
        let digest = sha256(&bytes);
        let exact = Expect::Exact {
            size: bytes.len() as u64,
            sha256: &digest,
        };

        fs::write(&dest, b"older copy").unwrap();
        assert_eq!(fetch_to(exact).unwrap(), (bytes.len() as u64, digest));
        assert_eq!(fs::read(&dest).unwrap(), bytes);
        assert!(!partial.exists());

        // Unknown size (the feed): accepted up to the cap, refused beyond it.
        fs::remove_file(&dest).unwrap();
        let cap = bytes.len() as u64;
        assert_eq!(fetch_to(Expect::AtMost(cap)).unwrap(), (cap, digest));
        fs::write(&dest, b"kept").unwrap();
        let err = fetch_to(Expect::AtMost(cap - 1)).unwrap_err();
        assert_eq!(err.code, ErrorCode::VerificationFailed, "{err}");

        // A hash mismatch leaves no partial file and the old dest untouched.
        let other = sha256(b"some other installer");
        let err = fetch_to(Expect::Exact {
            size: bytes.len() as u64,
            sha256: &other,
        })
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::VerificationFailed, "{err}");
        assert_eq!(fs::read(&dest).unwrap(), b"kept");
        assert!(!partial.exists());

        let plain = fixture.url("/archive").replace("https:", "http:");
        let err = fixture
            .downloader()
            .fetch_to(
                &plain,
                &dest,
                Expect::AtMost(cap),
                &AtomicBool::new(false),
                &mut |_, _| {},
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::DownloadFailed, "{err}");
        assert!(!partial.exists());
    }
}
