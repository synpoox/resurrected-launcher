//! D2R detection, validation and readiness (#57), on real temp folders with a
//! fake [`GameHost`] for the registry and PE versions; plus one read-only check of
//! a real D2R copy, gated on `RESURRECTED_REAL_D2R`.

mod support;

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use resurrected_launcher_lib::catalog::{GameBuild, Platform, ReleaseManifest};
use resurrected_launcher_lib::contract::{
    ErrorCode, GameLocation, GameStore, Readiness, RelativePath, RunningGame,
};
use resurrected_launcher_lib::game::detect::{candidates, validate};
use resurrected_launcher_lib::game::readiness::{assess, BuildHashCache, Facts};
use resurrected_launcher_lib::game::{GameHost, SystemGameHost};

const GAME_BYTES: &[u8] = b"fixture D2R.exe bytes";
const LOADER: &str = "1.3.1-beta";

#[derive(Default)]
struct FakeHost {
    registry: Vec<PathBuf>,
    /// ProductVersion by file name; files not listed have no version info.
    versions: HashMap<&'static str, String>,
}

impl FakeHost {
    fn with_versions(game: &str, loader: &str, core: &str) -> Self {
        let mut versions = HashMap::new();
        versions.insert("D2R.exe", game.to_string());
        versions.insert("D2RLoader.exe", loader.to_string());
        versions.insert("D2RCore.dll", core.to_string());
        Self {
            registry: Vec::new(),
            versions,
        }
    }
}

impl GameHost for FakeHost {
    fn battle_net_install_locations(&self) -> Vec<PathBuf> {
        self.registry.clone()
    }
    fn processes(&self) -> io::Result<Vec<RunningGame>> {
        unreachable!("detection and readiness never enumerate processes")
    }
    fn product_version(&self, file: &Path) -> io::Result<Option<String>> {
        fs::metadata(file)?;
        let name = file.file_name().unwrap().to_str().unwrap();
        Ok(self.versions.get(name).cloned())
    }
    fn spawn(&self, _: &Path, _: &[String], _: &Path) -> io::Result<u32> {
        unreachable!("detection and readiness never spawn")
    }
}

/// A temp D2R root with `D2R.exe`, `D2RLoader.exe` and `D2RCore.dll`.
fn game_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("D2R.exe"), GAME_BYTES).unwrap();
    fs::write(dir.path().join("D2RLoader.exe"), b"loader").unwrap();
    fs::write(dir.path().join("D2RCore.dll"), b"core").unwrap();
    dir
}

/// The fixture release, verified with exactly `GAME_BYTES` and [`LOADER`].
fn manifest(release_id: &str) -> ReleaseManifest {
    let mut m = support::release(support::PD2R_1).manifest;
    m.release_id = release_id.into();
    m.compatibility.game_builds = vec![GameBuild {
        build: "fixture".into(),
        platform: Platform::WindowsX64,
        executable: RelativePath::new("D2R.exe").unwrap(),
        size: GAME_BYTES.len() as u64,
        sha256: support::sha256(GAME_BYTES),
    }];
    m.compatibility.loader_versions = vec![LOADER.into()];
    m
}

fn location(dir: &Path) -> GameLocation {
    GameLocation {
        path: dir.to_path_buf(),
        build: "3.3.93847".into(),
        store: GameStore::Unknown,
    }
}

fn facts<'a>(
    location: Option<&'a GameLocation>,
    installed: Option<&'a ReleaseManifest>,
    recommended: Option<&'a ReleaseManifest>,
) -> Facts<'a> {
    Facts {
        location,
        installed,
        recommended,
        recovery_pending: false,
        running: &[],
    }
}

fn ok_host() -> FakeHost {
    FakeHost::with_versions("3.3.93847", LOADER, LOADER)
}

fn run(f: &Facts<'_>, host: &FakeHost) -> Readiness {
    assess(f, &BuildHashCache::default(), host).unwrap()
}

// ---- detect ----

#[test]
fn folder_without_d2r_exe_tells_the_player_what_to_choose() {
    let dir = tempfile::tempdir().unwrap();
    let err = validate(dir.path(), &ok_host()).unwrap_err();
    assert_eq!(err.code, ErrorCode::D2rNotFound);
    assert!(err.message.contains("Choose"), "{}", err.message);
    assert!(err.message.contains("D2R.exe"), "{}", err.message);
}

#[test]
fn d2r_exe_must_be_a_regular_file_with_version_info() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("D2R.exe")).unwrap();
    let err = validate(dir.path(), &ok_host()).unwrap_err();
    assert_eq!(err.code, ErrorCode::D2rNotFound);
    assert!(err.message.contains("not a file"), "{}", err.message);

    let game = game_dir();
    let err = validate(game.path(), &FakeHost::default()).unwrap_err();
    assert_eq!(err.code, ErrorCode::D2rNotFound);
    assert!(err.message.contains("no version"), "{}", err.message);
}

#[cfg(windows)]
#[test]
fn a_linked_d2r_exe_is_refused() {
    let real = game_dir();
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("D2R.exe");
    // A file symlink needs Developer Mode; a junction (a reparse point) never does.
    if std::os::windows::fs::symlink_file(real.path().join("D2R.exe"), &link).is_err() {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(real.path())
            .output()
            .unwrap()
            .status;
        assert!(status.success());
    }
    let err = validate(dir.path(), &ok_host()).unwrap_err();
    assert_eq!(err.code, ErrorCode::D2rNotFound);
    assert!(err.message.contains("link"), "{}", err.message);
}

#[test]
fn valid_folder_reports_build_and_store() {
    let game = game_dir();
    let loc = validate(game.path(), &ok_host()).unwrap();
    assert_eq!(loc.path, game.path());
    assert_eq!(loc.build, "3.3.93847");
    assert_eq!(loc.store, GameStore::Unknown);

    // The registry spells it differently: other case, trailing separator.
    let spelled = format!("{}\\", game.path().display()).to_uppercase();
    let host = FakeHost {
        registry: vec![PathBuf::from(spelled)],
        ..ok_host()
    };
    assert_eq!(
        validate(game.path(), &host).unwrap().store,
        GameStore::BattleNet
    );
}

#[test]
fn candidates_put_remembered_first_skip_invalid_and_duplicates() {
    let remembered = game_dir();
    let registry = game_dir();
    let empty = tempfile::tempdir().unwrap();
    let host = FakeHost {
        registry: vec![
            empty.path().into(),
            registry.path().into(),
            PathBuf::from(registry.path().to_string_lossy().to_lowercase()),
            remembered.path().into(),
        ],
        ..ok_host()
    };
    let found = candidates(Some(remembered.path()), &host);
    let paths: Vec<_> = found.iter().map(|l| l.path.as_path()).collect();
    assert_eq!(paths, [remembered.path(), registry.path()]);
    // The remembered folder is also a registry entry, so it is a Battle.net copy.
    assert_eq!(found[0].store, GameStore::BattleNet);
    assert_eq!(found[1].store, GameStore::BattleNet);

    assert!(candidates(None, &FakeHost::default()).is_empty());
}

// ---- readiness ----

#[test]
fn readiness_follows_the_documented_precedence() {
    let game = game_dir();
    let loc = location(game.path());
    let m1 = manifest("r1");
    let host = ok_host();

    assert_eq!(
        run(&facts(None, Some(&m1), None), &host),
        Readiness::D2rMissing
    );

    let running = [
        RunningGame {
            pid: 1,
            image: game.path().join("D2R.exe"),
            since: 200,
        },
        RunningGame {
            pid: 2,
            image: game.path().join("D2RLoader.exe"),
            since: 100,
        },
    ];
    let busy = Facts {
        running: &running,
        recovery_pending: true,
        ..facts(Some(&loc), Some(&m1), None)
    };
    assert_eq!(run(&busy, &host), Readiness::Running { since: 100 });

    let recovering = Facts {
        recovery_pending: true,
        ..facts(Some(&loc), Some(&m1), None)
    };
    assert_eq!(run(&recovering, &host), Readiness::RecoveryRequired);

    assert_eq!(
        run(&facts(Some(&loc), None, None), &host),
        Readiness::NotInstalled
    );
    assert_eq!(
        run(&facts(Some(&loc), None, Some(&m1)), &host),
        Readiness::NotInstalled
    );
    assert_eq!(
        run(&facts(Some(&loc), Some(&m1), None), &host),
        Readiness::Ready
    );
    assert_eq!(
        run(&facts(Some(&loc), Some(&m1), Some(&m1)), &host),
        Readiness::Ready
    );
    let m2 = manifest("r2");
    assert_eq!(
        run(&facts(Some(&loc), Some(&m1), Some(&m2)), &host),
        Readiness::UpdateAvailable
    );

    fs::remove_file(game.path().join("D2R.exe")).unwrap();
    assert_eq!(run(&busy, &host), Readiness::D2rMissing);
}

#[test]
fn unknown_build_is_unsupported_by_size_or_hash() {
    let game = game_dir();
    let loc = location(game.path());
    let host = ok_host();

    let mut other_hash = manifest("r1");
    other_hash.compatibility.game_builds[0].sha256 = support::sha256(b"another build");
    let mut other_size = manifest("r1");
    other_size.compatibility.game_builds[0].size += 1;
    let unsupported = Readiness::BuildUnsupported {
        found: "3.3.93847".into(),
    };
    for m in [&other_hash, &other_size] {
        assert_eq!(run(&facts(Some(&loc), Some(m), None), &host), unsupported);
        // Not installed: the recommended release's compatibility decides.
        assert_eq!(run(&facts(Some(&loc), None, Some(m)), &host), unsupported);
    }
    // Build is checked before the loader.
    let no_loader = FakeHost::with_versions("3.3.93847", "", "");
    fs::remove_file(game.path().join("D2RCore.dll")).unwrap();
    assert_eq!(
        run(&facts(Some(&loc), Some(&other_hash), None), &no_loader),
        unsupported
    );
}

#[test]
fn loader_must_be_present_versioned_and_allowed() {
    let game = game_dir();
    let loc = location(game.path());
    let m = manifest("r1");
    let f = facts(Some(&loc), Some(&m), None);

    // Unsupported loader before unsupported core, both reported against the allowed list.
    let both_old = FakeHost::with_versions("3.3.93847", "1.2.0", "1.1.0");
    assert_eq!(
        run(&f, &both_old),
        Readiness::LoaderUnsupported {
            found: "1.2.0".into(),
            allowed: vec![LOADER.into()],
        }
    );
    let core_old = FakeHost::with_versions("3.3.93847", LOADER, "1.1.0");
    assert_eq!(
        run(&f, &core_old),
        Readiness::LoaderUnsupported {
            found: "1.1.0".into(),
            allowed: vec![LOADER.into()],
        }
    );

    // A file without version info, or absent, is missing (before any unsupported one).
    let mut no_core_version = both_old;
    no_core_version.versions.remove("D2RCore.dll");
    assert_eq!(run(&f, &no_core_version), Readiness::LoaderMissing);
    fs::remove_file(game.path().join("D2RLoader.exe")).unwrap();
    assert_eq!(run(&f, &ok_host()), Readiness::LoaderMissing);
}

#[test]
fn build_hash_is_cached_by_path_size_and_mtime() {
    let game = game_dir();
    let exe = game.path().join("D2R.exe");
    let loc = location(game.path());
    let m = manifest("r1");
    let f = facts(Some(&loc), Some(&m), None);
    let host = ok_host();
    let cache = BuildHashCache::default();
    assert_eq!(assess(&f, &cache, &host).unwrap(), Readiness::Ready);

    // Same size and mtime, different bytes: the cached hash is reused.
    let mtime = fs::metadata(&exe).unwrap().modified().unwrap();
    let mut other = GAME_BYTES.to_vec();
    other[0] ^= 1;
    fs::write(&exe, &other).unwrap();
    set_mtime(&exe, mtime);
    assert_eq!(assess(&f, &cache, &host).unwrap(), Readiness::Ready);

    // A new mtime is a new key, so the changed bytes are hashed.
    set_mtime(&exe, mtime + Duration::from_secs(10));
    assert!(matches!(
        assess(&f, &cache, &host).unwrap(),
        Readiness::BuildUnsupported { .. }
    ));
}

fn set_mtime(file: &Path, at: SystemTime) {
    fs::File::options()
        .write(true)
        .open(file)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

// ---- the real machine ----

#[test]
fn system_host_reads_versions_and_registry_without_failing() {
    if cfg!(windows) {
        let v = SystemGameHost
            .product_version(Path::new(r"C:\Windows\System32\notepad.exe"))
            .unwrap()
            .expect("notepad.exe has version info");
        assert!(v.starts_with("10.0."), "{v}");
    }
    // Whatever the machine has, every registry candidate that validates is listed.
    for loc in candidates(None, &SystemGameHost) {
        assert_eq!(loc.store, GameStore::BattleNet);
    }
}

/// Read-only: validates and assesses `RESURRECTED_REAL_D2R` against the known
/// 3.3.93847 build and loader 1.3.1-beta, and proves no file's mtime changed.
#[test]
fn real_d2r_copy_is_ready_read_only() {
    let Some(root) = std::env::var_os("RESURRECTED_REAL_D2R") else {
        eprintln!("SKIPPED real_d2r_copy_is_ready_read_only: set RESURRECTED_REAL_D2R to a D2R folder to run it");
        return;
    };
    let root = PathBuf::from(root);
    let before = mtimes(&root);

    let host = SystemGameHost;
    let loc = validate(&root, &host).unwrap();
    println!("real D2R: {loc:?}");
    let mut m = manifest("real");
    m.compatibility.game_builds[0].size = 32_109_264;
    m.compatibility.game_builds[0].sha256 =
        "e1f5436e3d9687f644ef16938b1b183d1fdef434f18cf66d852cf68f48cc8936"
            .parse()
            .unwrap();
    for file in ["D2RLoader.exe", "D2RCore.dll"] {
        println!("{file}: {:?}", host.product_version(&root.join(file)));
    }
    let cache = BuildHashCache::default();
    let f = facts(Some(&loc), Some(&m), None);
    assert_eq!(assess(&f, &cache, &host).unwrap(), Readiness::Ready);
    assert_eq!(assess(&f, &cache, &host).unwrap(), Readiness::Ready);

    assert_eq!(mtimes(&root), before, "a file in the real D2R copy changed");
}

/// Modification time of every top-level entry; every file assessment reads is top level.
fn mtimes(root: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut all: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.path(),
                fs::symlink_metadata(e.path()).unwrap().modified().unwrap(),
            )
        })
        .collect();
    all.sort();
    all
}
