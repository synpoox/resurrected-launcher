//! The slice 2 failure matrix (#39): the #8 gate, one named test per clause, run
//! through the operation interface ([`ops::run`], [`ops::recover`]) on disposable
//! installations in real temp dirs, with the local fixture HTTPS server, the
//! pinned 7-Zip and injected faults. Nothing here mocks a file swap.
//!
//! | Module | Gate clause |
//! | --- | --- |
//! | [`crash_at`] | crash at every mutation [`Boundary`] (every [`FaultPoint`]) of install, update, repair, uninstall and clean install, then restart recovery |
//! | [`cancel_at`] | cancellation at every mutation boundary, then restart recovery |
//! | [`crash_during_recovery`] | a crash inside restart recovery itself, then recovery again |
//! | [`refusals`] | hash, size, path and link failures; insufficient space; access denial; second writer; running process; `WriteGuard` |
//! | [`preservation`] | shipped defaults, edited loot filters, unknown plugins, clean-install backup retention |
//!
//! After every failure the product root must hold either the complete previous
//! installation or the complete new one, byte for byte, with the installed-release
//! record agreeing, no journal and no transaction scratch left beside it.

#[path = "../support/mod.rs"]
mod support;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};

use resurrected_launcher_lib::catalog::ReleaseManifest;
use resurrected_launcher_lib::contract::{
    Boundary, FaultInjector, FaultPoint, InjectedFault, LauncherError, NoFaults, OperationKind,
    DISPOSABLE_SENTINEL,
};
use resurrected_launcher_lib::install::download::Downloader;
use resurrected_launcher_lib::install::extract::SevenZip;
use resurrected_launcher_lib::install::ops::{self, Env, Report};
use resurrected_launcher_lib::install::plan::{self, Request};
use resurrected_launcher_lib::install::preflight::{Host, SystemHost};
use resurrected_launcher_lib::install::transaction::Recovery;
use resurrected_launcher_lib::state::StateStore;
use support::*;

use OperationKind::*;

/// One `#[test]` per boundary, calling `$check(kind, boundary)`, plus a test that
/// the named boundaries are exactly `$seq` in order, so no boundary goes untested.
macro_rules! boundary_tests {
    ($check:ident, $kind:ident, $seq:expr; $($(#[$m:meta])* $name:ident => $b:ident),+ $(,)?) => {
        $(
            $(#[$m])*
            #[test]
            fn $name() {
                $check(OperationKind::$kind, Boundary::$b);
            }
        )+

        #[test]
        fn every_boundary_has_a_named_test() {
            assert_eq!(&[$(Boundary::$b),+][..], $seq);
        }
    };
}

mod cancel_at;
mod crash_at;
mod crash_during_recovery;
mod preservation;
mod refusals;

pub const CONFIG: &str = "d2rloader/config.ini";
pub const SAVE: &str = "save/Hero.d2s";
pub const ARMOR: &str = "PD2R.mpq/data/global/excel/armor.txt";

// ---- trees on disk ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    File(Vec<u8>),
    Dir,
    /// A symlink, junction or other reparse point; never followed.
    Link,
}

pub type Tree = BTreeMap<String, Node>;

fn is_link(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    m.file_type().is_symlink()
}

/// Every file, dir and link under `dir`, by `/`-separated path.
pub fn tree(dir: &Path) -> Tree {
    fn go(dir: &Path, prefix: &str, out: &mut Tree) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let meta = fs::symlink_metadata(entry.path()).unwrap();
            if is_link(&meta) {
                out.insert(rel, Node::Link);
            } else if meta.is_dir() {
                go(&entry.path(), &rel, out);
                out.insert(rel, Node::Dir);
            } else {
                out.insert(rel, Node::File(fs::read(entry.path()).unwrap()));
            }
        }
    }
    let mut out = Tree::new();
    go(dir, "", &mut out);
    out
}

pub fn with_file(mut t: Tree, rel: &str, bytes: &[u8]) -> Tree {
    for (i, _) in rel.match_indices('/') {
        t.insert(rel[..i].to_owned(), Node::Dir);
    }
    t.insert(rel.to_owned(), Node::File(bytes.to_vec()));
    t
}

/// Only the sentinel.
pub fn empty_root() -> Tree {
    with_file(Tree::new(), DISPOSABLE_SENTINEL, b"")
}

/// The tree an installed `release` has: its payload, its dirs and the sentinel.
pub fn release_tree(release: &FixtureRelease) -> Tree {
    let mut out = empty_root();
    for (rel, bytes) in &release.payload {
        out = with_file(out, rel, bytes);
    }
    for d in release.manifest.inventory.empty_dirs() {
        out.insert(d.to_string(), Node::Dir);
    }
    out
}

pub fn add_save(root: &Path) -> Vec<u8> {
    let bytes = b"character save, never converted".to_vec();
    fs::create_dir_all(root.join("save")).unwrap();
    fs::write(root.join(SAVE), &bytes).unwrap();
    bytes
}

/// Siblings of the product root other than its lock: journal, progress, staging,
/// backup, archive, partial download and clean-install backups.
pub fn siblings(root: &Path) -> Vec<String> {
    let name = root.file_name().unwrap().to_str().unwrap().to_owned();
    let mut left: Vec<String> = fs::read_dir(root.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with(&name) && *n != name && *n != format!("{name}.resurrected-lock"))
        .collect();
    left.sort();
    left
}

pub fn clean_install_backups(root: &Path) -> Vec<PathBuf> {
    let name = root.file_name().unwrap().to_str().unwrap().to_owned();
    siblings(root)
        .into_iter()
        .filter(|n| n.starts_with(&format!("{name}.clean-install-backup-")))
        .map(|n| root.with_file_name(n))
        .collect()
}

// ---- injectors and hosts ----

/// Runs `F` at every fault point and never fails: records, cancels or changes
/// the disk mid-transaction (a player edit, a game starting, a lost permission).
pub struct Hook<F>(pub F);

impl<F: Fn(Boundary, FaultPoint) + Send + Sync> FaultInjector for Hook<F> {
    fn check(&self, b: Boundary, p: FaultPoint) -> Result<(), InjectedFault> {
        (self.0)(b, p);
        Ok(())
    }
}

/// Every fault point a run consulted, in order.
#[derive(Default)]
pub struct Recorder(Mutex<Vec<(Boundary, FaultPoint)>>);

impl FaultInjector for Recorder {
    fn check(&self, b: Boundary, p: FaultPoint) -> Result<(), InjectedFault> {
        self.0.lock().unwrap().push((b, p));
        Ok(())
    }
}

impl Recorder {
    pub fn points_of(&self, b: Boundary) -> Vec<FaultPoint> {
        let seen = self.0.lock().unwrap();
        seen.iter()
            .filter(|(x, _)| *x == b)
            .map(|(_, p)| *p)
            .collect()
    }
}

// ---- rig ----

fn shared_server() -> &'static FixtureServer {
    static SERVER: OnceLock<FixtureServer> = OnceLock::new();
    SERVER.get_or_init(FixtureServer::start)
}

/// One disposable D2R installation, a fixture server and launcher state.
pub struct Rig {
    pub game: DisposableGame,
    pub server: &'static FixtureServer,
    downloader: Downloader,
    seven_zip: SevenZip,
    _state_dir: tempfile::TempDir,
    pub state: Mutex<StateStore>,
    pub cancel: AtomicBool,
}

impl Rig {
    fn on(server: &'static FixtureServer) -> Self {
        let state_dir = tempfile::tempdir().unwrap();
        Self {
            game: DisposableGame::new(),
            downloader: Downloader::with_tls(server.tls_config()),
            server,
            seven_zip: seven_zip(),
            state: Mutex::new(StateStore::open(state_dir.path())),
            _state_dir: state_dir,
            cancel: AtomicBool::new(false),
        }
    }

    /// Shares one fixture server; for tests that never change its replies.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self::on(shared_server())
    }

    /// Its own fixture server, so the test may inject failing replies.
    pub fn with_own_server() -> Self {
        Self::on(Box::leak(Box::new(FixtureServer::start())))
    }

    pub fn env<'a>(&'a self, injector: &'a dyn FaultInjector, host: &'a dyn Host) -> Env<'a> {
        Env {
            downloader: &self.downloader,
            seven_zip: &self.seven_zip,
            state: &self.state,
            host,
            injector,
            cancel: &self.cancel,
        }
    }

    /// The fixture release with its archive served by this rig's server.
    pub fn served(&self, name: &str) -> (FixtureRelease, ReleaseManifest) {
        self.served_release(release(name))
    }

    pub fn served_release(&self, release: FixtureRelease) -> (FixtureRelease, ReleaseManifest) {
        let mut manifest = release.manifest.clone();
        manifest.archive = self.server.serve(&release);
        (release, manifest)
    }

    /// The PD2R product root.
    pub fn root(&self) -> PathBuf {
        plan::product_root(self.game.d2r(), &release(PD2R_1).manifest)
    }

    /// The installed PD2R release ID, after checking the record also names that
    /// fixture's exact manifest and is marked verified.
    pub fn installed_release(&self) -> Option<String> {
        let state = self.state.lock().unwrap();
        let r = state
            .installed_releases()
            .iter()
            .find(|r| r.product_id.0 == "pd2r")?;
        let manifest = &release(&r.release_id).manifest_json;
        assert_eq!(
            r.manifest_sha256,
            sha256(manifest),
            "{}: manifest sha",
            r.release_id
        );
        assert!(r.verified, "{}: record not verified", r.release_id);
        Some(r.release_id.clone())
    }

    pub fn recover(&self, injector: &dyn FaultInjector) -> Result<Recovery, LauncherError> {
        let env = self.env(injector, &SystemHost);
        ops::recover(&env, self.game.d2r(), &release(PD2R_1).manifest)
    }

    /// Installs PD2R fixture 1 without faults.
    pub fn install_1(&self) -> ReleaseManifest {
        let op = Op::new(self, Install, PD2R_1, None);
        self.game.product_root(&op.release);
        op.run(self, &NoFaults).unwrap();
        op.manifest
    }
}

/// An operation request with owned parts.
pub struct Op {
    pub kind: OperationKind,
    pub release: FixtureRelease,
    pub manifest: ReleaseManifest,
    pub installed: Option<ReleaseManifest>,
    pub consent_clean_install: bool,
    pub acknowledge_save_breaking: bool,
}

impl Op {
    pub fn new(
        rig: &Rig,
        kind: OperationKind,
        name: &str,
        installed: Option<ReleaseManifest>,
    ) -> Self {
        let (release, manifest) = rig.served(name);
        Self {
            kind,
            release,
            manifest,
            installed,
            consent_clean_install: kind == CleanInstall,
            acknowledge_save_breaking: false,
        }
    }

    pub fn request<'a>(&'a self, rig: &'a Rig) -> Request<'a> {
        Request {
            kind: self.kind,
            d2r: rig.game.d2r(),
            release: &self.manifest,
            release_sha256: sha256(&self.release.manifest_json),
            installed: self.installed.as_ref(),
            acknowledge_save_breaking: self.acknowledge_save_breaking,
            consent_clean_install: self.consent_clean_install,
        }
    }

    pub fn run(&self, rig: &Rig, injector: &dyn FaultInjector) -> Result<Report, LauncherError> {
        self.run_on(rig, injector, &SystemHost)
    }

    pub fn run_on(
        &self,
        rig: &Rig,
        injector: &dyn FaultInjector,
        host: &dyn Host,
    ) -> Result<Report, LauncherError> {
        ops::run(&rig.env(injector, host), &self.request(rig))
    }
}

// ---- scenarios ----

/// One operation on a disposable installation that holds player files: the tree
/// and installed-release record before it (`old`) and after it commits (`new`).
pub struct Scenario {
    pub rig: Rig,
    pub op: Op,
    pub root: PathBuf,
    pub old: Tree,
    pub old_record: Option<&'static str>,
    pub new: Tree,
    pub new_record: Option<&'static str>,
}

impl Scenario {
    /// Every scenario has an unknown plugin; all but install also an edited loot
    /// filter (install has no filter yet; clean install carries it forward, so
    /// `PlayerFilesCarriedForward` has a real entry); all but uninstall a save.
    pub fn new(kind: OperationKind) -> Self {
        Self::on(Rig::new(), kind)
    }

    pub fn with_own_server(kind: OperationKind) -> Self {
        Self::on(Rig::with_own_server(), kind)
    }

    fn on(rig: Rig, kind: OperationKind) -> Self {
        let r1 = release(PD2R_1);
        let r2 = release(PD2R_2);
        let root = rig.root();
        let (op, new, old_record, new_record) = match kind {
            Install => {
                rig.game.product_root(&r1);
                let plugin = add_unknown_plugin(&root);
                let save = add_save(&root);
                let new = with_file(release_tree(&r1), UNKNOWN_PLUGIN, &plugin);
                let op = Op::new(&rig, Install, PD2R_1, None);
                (op, with_file(new, SAVE, &save), None, Some(PD2R_1))
            }
            Update => {
                let m1 = rig.install_1();
                let filter = edit_loot_filter(&root);
                let plugin = add_unknown_plugin(&root);
                let save = add_save(&root);
                let new = with_file(release_tree(&r2), LOOT_FILTER, &filter);
                let new = with_file(new, UNKNOWN_PLUGIN, &plugin);
                let op = Op::new(&rig, Update, PD2R_2, Some(m1));
                (op, with_file(new, SAVE, &save), Some(PD2R_1), Some(PD2R_2))
            }
            Repair => {
                let m1 = rig.install_1();
                fs::write(root.join(ARMOR), b"damaged").unwrap();
                fs::remove_file(root.join("PD2R.mpq/modinfo.json")).unwrap();
                fs::remove_file(root.join(CONFIG)).unwrap();
                let filter = edit_loot_filter(&root);
                let plugin = add_unknown_plugin(&root);
                let save = add_save(&root);
                let new = with_file(release_tree(&r1), LOOT_FILTER, &filter);
                let new = with_file(new, UNKNOWN_PLUGIN, &plugin);
                let op = Op::new(&rig, Repair, PD2R_1, Some(m1));
                (op, with_file(new, SAVE, &save), Some(PD2R_1), Some(PD2R_1))
            }
            Uninstall => {
                let m1 = rig.install_1();
                let filter = edit_loot_filter(&root);
                let plugin = add_unknown_plugin(&root);
                let new = with_file(empty_root(), LOOT_FILTER, &filter);
                let op = Op::new(&rig, Uninstall, PD2R_1, Some(m1));
                (
                    op,
                    with_file(new, UNKNOWN_PLUGIN, &plugin),
                    Some(PD2R_1),
                    None,
                )
            }
            CleanInstall => {
                let m1 = rig.install_1();
                let filter = edit_loot_filter(&root);
                add_unknown_plugin(&root);
                add_save(&root);
                // The edited filter is carried forward; the plugin and save are
                // not release files, so they stay in the labelled backup only.
                let new = with_file(release_tree(&r2), LOOT_FILTER, &filter);
                let op = Op::new(&rig, CleanInstall, PD2R_2, Some(m1));
                (op, new, Some(PD2R_1), Some(PD2R_2))
            }
            Adopt => unreachable!("adoption writes only the record"),
        };
        Self {
            old: tree(&root),
            root,
            rig,
            op,
            old_record,
            new,
            new_record,
        }
    }

    pub fn run(&self, injector: &dyn FaultInjector) -> Result<Report, LauncherError> {
        self.op.run(&self.rig, injector)
    }

    /// The root holds the complete previous (`new == false`) or new installation,
    /// the record agrees, and no journal or scratch is left. A committed clean
    /// install leaves exactly one labelled backup holding the old folder.
    pub fn assert_settled(&self, new: bool, case: &str) {
        let (want, record) = if new {
            (&self.new, self.new_record)
        } else {
            (&self.old, self.old_record)
        };
        assert!(
            tree(&self.root) == *want,
            "{case}: the product root is not the {} installation\n got: {:#?}\nwant: {:#?}",
            if new { "new" } else { "previous" },
            tree(&self.root).keys().collect::<Vec<_>>(),
            want.keys().collect::<Vec<_>>(),
        );
        assert_eq!(
            self.rig.installed_release().as_deref(),
            record,
            "{case}: installed-release record"
        );
        let backups = clean_install_backups(&self.root);
        if new && self.op.kind == CleanInstall {
            assert_eq!(backups.len(), 1, "{case}: one labelled backup");
            assert!(
                tree(&backups[0]) == self.old,
                "{case}: backup is the old folder"
            );
        }
        let scratch: Vec<_> = siblings(&self.root)
            .into_iter()
            .filter(|n| !backups.iter().any(|b| b.file_name().unwrap() == n.as_str()))
            .collect();
        assert!(
            scratch.is_empty(),
            "{case}: left beside the root: {scratch:?}"
        );
        if !(new && self.op.kind == CleanInstall) {
            assert!(backups.is_empty(), "{case}: unexpected backup {backups:?}");
        }
    }

    /// Nothing changed, and there is nothing to recover.
    pub fn assert_unchanged(&self, case: &str) {
        assert_eq!(
            self.rig.recover(&NoFaults).unwrap(),
            Recovery::Nothing,
            "{case}"
        );
        self.assert_settled(false, case);
    }
}

/// Whether a stop at `p` of forward boundary `b` must end in the new installation:
/// only once `LiveVerified` is journaled, i.e. after its `AfterStep`.
pub fn past_live_verified(kind: OperationKind, b: Boundary) -> bool {
    let seq = Boundary::sequence(kind);
    let at = |x| seq.iter().position(|y| *y == x).unwrap();
    at(b) > at(Boundary::LiveVerified)
}

/// Every forward fault point of `b` in a fault-free run of `kind`, checked to
/// commit the expected new installation.
pub fn forward_points(kind: OperationKind, b: Boundary) -> Vec<FaultPoint> {
    let reference = Scenario::new(kind);
    let seen = Recorder::default();
    reference.run(&seen).unwrap();
    reference.assert_settled(true, &format!("{kind:?} without faults"));
    let points = seen.points_of(b);
    assert_eq!(
        points.first(),
        Some(&FaultPoint::BeforeStep),
        "{kind:?} {b:?}"
    );
    assert_eq!(
        points.last(),
        Some(&FaultPoint::AfterStep),
        "{kind:?} {b:?}"
    );
    points
}
