//! Fixture products, a local fixture HTTP server and disposable installations for
//! integration tests (#38). Include it with `mod support;` from a file in `tests/`.
//!
//! Nothing here touches a real D2R installation: every directory lives under a
//! fresh [`tempfile::TempDir`], and every product root carries the
//! [`DISPOSABLE_SENTINEL`] so [`WriteGuard::check`] accepts it.
//!
//! # Fixture releases
//!
//! Sources live in `fixtures/products/`: `base.json` holds the manifest fields all
//! releases share, `<release_id>/release.json` overrides top-level keys, and
//! `<release_id>/payload/` is the file tree. `shipped_defaults` and `empty_dirs`
//! in the JSON drive ownership and empty directories; sizes, hashes, `files` and
//! `archive` are computed. Every archive is a real 7z built with the bundled
//! pinned 7-Zip, and every manifest passes [`ReleaseManifest::parse`].
//!
//! | [`release`] name | What it is |
//! | --- | --- |
//! | [`PD2R_1`] | Baseline. Shipped defaults `d2rloader/config.ini` and [`LOOT_FILTER`], empty dir `d2rloader/patches`, an empty file, `retired.txt`. |
//! | [`PD2R_2`] | Update of 1: `armor.txt`, the plugin and the loot filter default change; `weapons.txt` added; `retired.txt` dropped; `config.ini` unchanged. |
//! | [`PD2R_3`] | Save-breaking release (ADR 0001) after 2; `config.ini` default changes. |
//! | [`FOREVER_1`] | A second product (`mods/Forever`), for "other products untouched". |
//!
//! [`malicious`] returns [`PD2R_1`] with a hostile archive (see [`Malice`]) whose
//! manifest `archive.size`/`sha256` match the hostile bytes, so download
//! verification passes and only extraction can refuse it. Its `release_id` is
//! `pd2r-fixture-1-malicious-<kind>`.
//!
//! Manifest URLs are public-looking (`https://fixtures.example.org/...`) because
//! the catalog rejects local hosts and ports. To download, point the archive at
//! the fixture server with [`FixtureServer::serve`], which returns the
//! [`Archive`] to pass to the downloader.
//!
//! # Player files
//!
//! [`edit_loot_filter`] turns the shipped default into an edited player file, and
//! [`add_unknown_plugin`] writes [`UNKNOWN_PLUGIN`], an optional plugin no
//! manifest lists. Both return the bytes written so tests can assert they survive.
//!
//! # Fixture server
//!
//! [`FixtureServer::start`] serves HTTPS on `127.0.0.1` with a fresh self-signed
//! certificate ([`FixtureServer::cert_der`], [`FixtureServer::tls_config`]).
//! [`FixtureServer::route`] sets a path's standing [`Reply`];
//! [`FixtureServer::once`] queues replies used before it (fail first, then
//! succeed). Unrouted paths answer 404. [`FixtureServer::hits`] lists requested
//! paths in order. Injectable failures: [`Reply::Truncated`], [`Reply::Delayed`],
//! [`Reply::WrongHash`], [`Reply::RedirectToHttp`], [`Reply::Status`] (5xx) and
//! [`Reply::Reset`] (TCP RST mid-body).
//!
//! To download from this server, build the downloader with
//! `Downloader::with_tls(server.tls_config())` ([`FixtureServer::tls_config`]).
//!
//! # Example
//!
//! ```ignore
//! let release = support::release(support::PD2R_1);
//! let server = support::FixtureServer::start();
//! let archive = server.serve(&release);          // Archive with a local URL
//! let game = support::DisposableGame::new();
//! let root = game.product_root(&release);        // empty root with sentinel
//! let guard = WriteGuard::check(&root).unwrap();
//! ```
#![allow(dead_code)] // Each test crate uses a different part of the harness.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use resurrected_launcher_lib::catalog::{Archive, ReleaseManifest};
use resurrected_launcher_lib::contract::{Sha256Digest, DISPOSABLE_SENTINEL};
use resurrected_launcher_lib::install::extract::SevenZip;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[allow(unused_imports)] // Not every test crate guards a root itself.
pub use resurrected_launcher_lib::contract::WriteGuard;

pub const PD2R_1: &str = "pd2r-fixture-1";
pub const PD2R_2: &str = "pd2r-fixture-2";
pub const PD2R_3: &str = "pd2r-fixture-3";
pub const FOREVER_1: &str = "forever-fixture-1";
pub const ALL_RELEASES: [&str; 4] = [PD2R_1, PD2R_2, PD2R_3, FOREVER_1];

/// Shipped default loot filter of the PD2R fixture releases.
pub const LOOT_FILTER: &str = "lootfilter/default.filter";
/// An optional plugin the player added; unknown to every manifest.
pub const UNKNOWN_PLUGIN: &str = "d2rloader/plugins/player-optional.dll";

const FIXTURE_HOST: &str = "https://fixtures.example.org";

fn products_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/products")
}

fn seven_zip_exe() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/7zip/7za.exe")
}

/// The bundled, pin-checked 7-Zip.
pub fn seven_zip() -> SevenZip {
    SevenZip::bundled(&seven_zip_exe()).expect("pinned 7-Zip is bundled")
}

pub fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into()).expect("not all zero")
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A built fixture release: validated manifest, its exact bytes, the 7z archive
/// and the payload it unpacks to.
#[derive(Debug, Clone)]
pub struct FixtureRelease {
    pub manifest: ReleaseManifest,
    pub manifest_json: Vec<u8>,
    pub archive: Vec<u8>,
    /// Manifest-relative path to bytes, exactly the manifest `files`.
    pub payload: BTreeMap<String, Vec<u8>>,
}

impl FixtureRelease {
    pub fn release_id(&self) -> &str {
        &self.manifest.release_id
    }

    /// Path of the archive on the fixture server.
    pub fn archive_path(&self) -> String {
        format!("/{}.7z", self.release_id())
    }

    pub fn manifest_path(&self) -> String {
        format!("/{}/release-manifest.json", self.release_id())
    }
}

/// A hostile archive entry added to [`PD2R_1`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Malice {
    /// `..\evil.txt`.
    Traversal,
    /// `C:\evil.txt`.
    Absolute,
    /// `PD2R.MPQ\MODINFO.JSON` beside `PD2R.mpq/modinfo.json`.
    CaseConflict,
    /// `evil.txt`, a file the manifest does not list.
    Unlisted,
    /// `jn`, a directory junction (reparse point) stored with `-snl`.
    Junction,
}

impl Malice {
    pub const ALL: [Malice; 5] = [
        Self::Traversal,
        Self::Absolute,
        Self::CaseConflict,
        Self::Unlisted,
        Self::Junction,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Traversal => "traversal",
            Self::Absolute => "absolute",
            Self::CaseConflict => "case-conflict",
            Self::Unlisted => "unlisted",
            Self::Junction => "junction",
        }
    }
}

/// A fixture release from `fixtures/products/<name>`. Built once per test process.
pub fn release(name: &str) -> FixtureRelease {
    cached(name, None)
}

/// [`PD2R_1`] with a hostile archive; see [`Malice`].
pub fn malicious(malice: Malice) -> FixtureRelease {
    cached(PD2R_1, Some(malice))
}

fn cached(name: &str, malice: Option<Malice>) -> FixtureRelease {
    type Cache = Mutex<HashMap<(String, Option<Malice>), FixtureRelease>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let key = (name.to_owned(), malice);
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap().get(&key) {
        return hit.clone();
    }
    let built = build(name, malice);
    cache.lock().unwrap().insert(key, built.clone());
    built
}

fn read_json(path: &Path) -> Value {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn read_tree(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if entry.file_type().unwrap().is_dir() {
            read_tree(&entry.path(), &rel, out);
        } else {
            out.insert(rel, fs::read(entry.path()).unwrap());
        }
    }
}

fn strings(value: Option<Value>) -> Vec<String> {
    serde_json::from_value(value.unwrap_or(json!([]))).unwrap()
}

fn write_tree(root: &Path, payload: &BTreeMap<String, Vec<u8>>, empty_dirs: &[String]) {
    for (rel, bytes) in payload {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    for dir in empty_dirs {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
}

fn run_7z(cwd: &Path, args: &[&str]) {
    let out = Command::new(seven_zip_exe())
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "7-Zip {args:?}: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

fn junction(link: &Path, target: &Path) {
    // A junction needs no privilege, unlike a symlink.
    let status = Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "mklink /J failed");
}

/// The 7z archive of `payload` plus `empty_dirs`, with `malice` applied.
fn archive(
    payload: &BTreeMap<String, Vec<u8>>,
    empty_dirs: &[String],
    malice: Option<Malice>,
) -> Vec<u8> {
    let work = tempfile::tempdir().unwrap();
    let tree = work.path().join("tree");
    write_tree(&tree, payload, empty_dirs);
    let mut add = vec!["a", "-t7z", "-bso0", "-bsp0"];
    match malice {
        Some(Malice::Junction) => {
            junction(&tree.join("jn"), &tree.join("lootfilter"));
            add.push("-snl");
        }
        Some(_) => fs::write(tree.join("evil.txt"), b"evil").unwrap(),
        None => {}
    }
    add.extend(["--", "../out.7z", "*"]);
    run_7z(&tree, &add);
    let renamed = match malice {
        Some(Malice::Traversal) => Some("..\\evil.txt"),
        Some(Malice::Absolute) => Some("C:\\evil.txt"),
        Some(Malice::CaseConflict) => Some("PD2R.MPQ\\MODINFO.JSON"),
        _ => None,
    };
    if let Some(to) = renamed {
        run_7z(
            work.path(),
            &["rn", "-t7z", "-bso0", "out.7z", "evil.txt", to],
        );
    }
    fs::read(work.path().join("out.7z")).unwrap()
}

fn build(name: &str, malice: Option<Malice>) -> FixtureRelease {
    let dir = products_dir();
    let mut doc = read_json(&dir.join("base.json"));
    let Value::Object(over) = read_json(&dir.join(name).join("release.json")) else {
        panic!("{name}/release.json is not an object");
    };
    // Shallow merge: a release replaces whole top-level keys.
    doc.as_object_mut().unwrap().extend(over);
    let shipped = strings(doc.as_object_mut().unwrap().remove("shipped_defaults"));
    let empty_dirs = strings(doc.get("empty_dirs").cloned());

    let mut payload = BTreeMap::new();
    read_tree(&dir.join(name).join("payload"), "", &mut payload);
    let archive = archive(&payload, &empty_dirs, malice);

    if let Some(m) = malice {
        doc["release_id"] = json!(format!("{name}-malicious-{}", m.name()));
    }
    let release_id = doc["release_id"].as_str().unwrap().to_owned();
    let product_id = doc["product_id"].as_str().unwrap().to_owned();
    doc["files"] = payload
        .iter()
        .map(|(path, bytes)| {
            let ownership = if shipped.contains(path) {
                "shipped_default"
            } else {
                "managed"
            };
            json!({ "path": path, "size": bytes.len(), "sha256": hex(bytes), "ownership": ownership })
        })
        .collect();
    doc["archive"] = json!({
        "url": format!("{FIXTURE_HOST}/{product_id}/{release_id}.7z"),
        "format": "7z",
        "size": archive.len(),
        "sha256": hex(&archive),
        "file_count": payload.len(),
        "total_size": payload.values().map(Vec::len).sum::<usize>(),
    });
    let manifest_json = serde_json::to_vec_pretty(&doc).unwrap();
    let manifest = ReleaseManifest::parse(&manifest_json)
        .unwrap_or_else(|e| panic!("fixture {release_id} fails validation: {e:?}"));
    FixtureRelease {
        manifest,
        manifest_json,
        archive,
        payload,
    }
}

/// A schema-3 release feed recommending each of `releases` (one per product).
/// The URLs are public-looking; the bytes are served at
/// [`FixtureRelease::manifest_path`] by [`FixtureServer::serve`].
pub fn feed(releases: &[&FixtureRelease]) -> Vec<u8> {
    let products: Vec<Value> = releases
        .iter()
        .map(|r| {
            json!({
                "product_id": r.manifest.product_id.0,
                "availability": "available",
                "recommended": {
                    "release_id": r.release_id(),
                    "manifest_url": format!("{FIXTURE_HOST}{}", r.manifest_path()),
                    "manifest_size": r.manifest_json.len(),
                    "manifest_sha256": hex(&r.manifest_json),
                }
            })
        })
        .collect();
    serde_json::to_vec_pretty(&json!({ "schema_version": 3, "products": products })).unwrap()
}

// ---- disposable installations ----

/// A disposable D2R installation directory (no game files) in a fresh temp dir,
/// removed on drop.
pub struct DisposableGame {
    _dir: tempfile::TempDir,
    d2r: PathBuf,
}

impl Default for DisposableGame {
    fn default() -> Self {
        Self::new()
    }
}

impl DisposableGame {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let d2r = dir.path().join("Diablo II Resurrected");
        fs::create_dir_all(d2r.join("mods")).unwrap();
        Self { _dir: dir, d2r }
    }

    /// The fake D2R installation root.
    pub fn d2r(&self) -> &Path {
        &self.d2r
    }

    /// Creates the release's `install_root` holding only the sentinel.
    pub fn product_root(&self, release: &FixtureRelease) -> PathBuf {
        let root = self.d2r.join(release.manifest.install_root.as_str());
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(DISPOSABLE_SENTINEL), b"").unwrap();
        root
    }

    /// [`Self::product_root`] with the release's files and empty dirs already in
    /// place, as after an install (or a manual install to adopt).
    pub fn installed(&self, release: &FixtureRelease) -> PathBuf {
        let root = self.product_root(release);
        let empty: Vec<String> = release
            .manifest
            .inventory
            .empty_dirs()
            .iter()
            .map(|d| d.as_str().to_owned())
            .collect();
        write_tree(&root, &release.payload, &empty);
        root
    }
}

/// Overwrites [`LOOT_FILTER`] with player edits; returns the edited bytes.
pub fn edit_loot_filter(root: &Path) -> Vec<u8> {
    let edited = b"// Edited by the player\nShow item.quality >= magic\n".to_vec();
    fs::write(root.join(LOOT_FILTER), &edited).unwrap();
    edited
}

/// Writes [`UNKNOWN_PLUGIN`]; returns its bytes.
pub fn add_unknown_plugin(root: &Path) -> Vec<u8> {
    let bytes = b"optional plugin the player installed by hand".to_vec();
    let path = root.join(UNKNOWN_PLUGIN);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, &bytes).unwrap();
    bytes
}

// ---- fixture HTTP server ----

/// How the fixture server answers one request.
#[derive(Debug, Clone)]
pub enum Reply {
    /// 200 with this body.
    Body(Vec<u8>),
    /// Announces the full length, sends `after` bytes, then closes cleanly.
    Truncated { body: Vec<u8>, after: usize },
    /// Waits, then answers with the inner reply.
    Delayed(Duration, Box<Reply>),
    /// 200 with the same length but the last byte flipped.
    WrongHash(Vec<u8>),
    /// 302 to this path on the same HTTPS server.
    Redirect(String),
    /// 302 to this path over plain `http://`.
    RedirectToHttp(String),
    /// A bare status, e.g. 500 or 503.
    Status(u16),
    /// Announces the full length, sends `after` bytes, then resets the connection.
    Reset { body: Vec<u8>, after: usize },
}

#[derive(Default)]
struct Route {
    once: VecDeque<Reply>,
    always: Option<Reply>,
}

type Routes = Arc<Mutex<HashMap<String, Route>>>;
type Tls = rustls::StreamOwned<rustls::ServerConnection, TcpStream>;

/// Local HTTPS server with injectable failures. Runs until the process exits.
pub struct FixtureServer {
    base: String,
    cert: Vec<u8>,
    routes: Routes,
    hits: Arc<Mutex<Vec<String>>>,
}

impl Default for FixtureServer {
    fn default() -> Self {
        Self::start()
    }
}

impl FixtureServer {
    pub fn start() -> Self {
        use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
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
        let port = listener.local_addr().unwrap().port();
        let server = Self {
            base: format!("https://127.0.0.1:{port}"),
            cert,
            routes: Routes::default(),
            hits: Arc::default(),
        };
        let (routes, hits) = (server.routes.clone(), server.hits.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (config, routes, hits) = (config.clone(), routes.clone(), hits.clone());
                std::thread::spawn(move || {
                    let conn = rustls::ServerConnection::new(config).unwrap();
                    let mut tls = rustls::StreamOwned::new(conn, stream);
                    let Some(path) = read_request(&mut tls) else {
                        return;
                    };
                    hits.lock().unwrap().push(path.clone());
                    let reply = {
                        let mut routes = routes.lock().unwrap();
                        routes
                            .get_mut(&path)
                            .and_then(|r| r.once.pop_front().or_else(|| r.always.clone()))
                    };
                    respond(&mut tls, reply.unwrap_or(Reply::Status(404)), port);
                });
            }
        });
        server
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// DER of the server's self-signed certificate.
    pub fn cert_der(&self) -> &[u8] {
        &self.cert
    }

    /// A ureq TLS config trusting only this server.
    pub fn tls_config(&self) -> ureq::tls::TlsConfig {
        use ureq::tls::{Certificate, RootCerts, TlsConfig};
        let root = Certificate::from_der(&self.cert).to_owned();
        TlsConfig::builder()
            .root_certs(RootCerts::new_with_certs(&[root]))
            .build()
    }

    /// Sets the standing reply for `path`.
    pub fn route(&self, path: &str, reply: Reply) {
        self.routes
            .lock()
            .unwrap()
            .entry(path.to_owned())
            .or_default()
            .always = Some(reply);
    }

    /// Queues a one-shot reply for `path`, used before the standing one.
    pub fn once(&self, path: &str, reply: Reply) {
        self.routes
            .lock()
            .unwrap()
            .entry(path.to_owned())
            .or_default()
            .once
            .push_back(reply);
    }

    /// Serves the release's archive and manifest; returns its manifest
    /// [`Archive`] pointing at this server.
    pub fn serve(&self, release: &FixtureRelease) -> Archive {
        self.route(
            &release.archive_path(),
            Reply::Body(release.archive.clone()),
        );
        self.route(
            &release.manifest_path(),
            Reply::Body(release.manifest_json.clone()),
        );
        Archive {
            url: self.url(&release.archive_path()),
            ..release.manifest.archive.clone()
        }
    }

    /// Requested paths, in order.
    pub fn hits(&self) -> Vec<String> {
        self.hits.lock().unwrap().clone()
    }
}

/// Reads the request line and headers; returns the path.
fn read_request(tls: &mut Tls) -> Option<String> {
    let mut reader = BufReader::new(tls);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let path = line.split(' ').nth(1)?.to_owned();
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) | Err(_) => return None,
            Ok(_) if header == "\r\n" => return Some(path),
            Ok(_) => {}
        }
    }
}

fn head(tls: &mut Tls, status: &str, extra: &str, length: usize) {
    let head = format!(
        "HTTP/1.1 {status}\r\n{extra}Content-Length: {length}\r\nConnection: close\r\n\r\n"
    );
    let _ = tls.write_all(head.as_bytes());
}

fn respond(tls: &mut Tls, reply: Reply, port: u16) {
    match reply {
        Reply::Body(body) => {
            head(tls, "200 OK", "", body.len());
            let _ = tls.write_all(&body);
        }
        Reply::Truncated { body, after } => {
            head(tls, "200 OK", "", body.len());
            let _ = tls.write_all(&body[..after]);
        }
        Reply::Delayed(wait, inner) => {
            std::thread::sleep(wait);
            return respond(tls, *inner, port);
        }
        Reply::WrongHash(mut body) => {
            if let Some(last) = body.last_mut() {
                *last ^= 0xff;
            }
            return respond(tls, Reply::Body(body), port);
        }
        Reply::Redirect(path) => {
            let to = format!("Location: https://127.0.0.1:{port}{path}\r\n");
            head(tls, "302 Found", &to, 0);
        }
        Reply::RedirectToHttp(path) => {
            let to = format!("Location: http://127.0.0.1:{port}{path}\r\n");
            head(tls, "302 Found", &to, 0);
        }
        Reply::Status(code) => head(tls, &format!("{code} Fixture"), "", 0),
        Reply::Reset { body, after } => {
            head(tls, "200 OK", "", body.len());
            let _ = tls.write_all(&body[..after]);
            let _ = tls.flush();
            // Linger 0 makes close send RST instead of FIN; no close_notify either.
            let _ = socket2::SockRef::from(&tls.sock).set_linger(Some(Duration::ZERO));
            return;
        }
    }
    tls.conn.send_close_notify();
    let _ = tls.flush();
}
