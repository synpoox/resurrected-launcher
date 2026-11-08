//! Crash matrix on disposable installations in real temp dirs: a fault at every
//! boundary and entry point of install, update, repair, uninstall and clean
//! install, then restart recovery (itself crashed and rerun) must leave either the
//! complete previous installation or the complete new one, byte for byte.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::json;

use super::*;
use crate::contract::{FailAt, NoFaults};
use crate::install::journal::journal_path;

const SENTINEL: (&str, &str) = (DISPOSABLE_SENTINEL, "");
const PLAYER_SAVE: (&str, &str) = ("save/hero.d2s", "player save");
const UNKNOWN_PLUGIN: (&str, &str) = ("plugins/mine.dll", "player plugin");

/// One operation on one disposable product root.
#[derive(Clone)]
struct Case {
    kind: OperationKind,
    /// Files in `L` before the operation.
    before: Vec<(&'static str, &'static str)>,
    /// The release files extraction puts in `S`.
    release: Vec<(&'static str, &'static str)>,
    /// What `LiveVerified` requires: `Some(bytes)` present, `None` absent.
    live: Vec<(&'static str, Option<&'static str>)>,
    to_backup: Vec<&'static str>,
    to_live: Vec<&'static str>,
    carry_forward: Vec<&'static str>,
    prune_if_empty: Vec<&'static str>,
    record: RecordChange,
}

fn rels(v: &[&str]) -> Vec<RelativePath> {
    v.iter().map(|s| RelativePath::new(*s).unwrap()).collect()
}

fn install_case() -> Case {
    Case {
        kind: OperationKind::Install,
        before: vec![SENTINEL, PLAYER_SAVE, UNKNOWN_PLUGIN],
        release: vec![("data/a.txt", "new a"), ("data/new/c.txt", "new c")],
        live: vec![
            ("data/a.txt", Some("new a")),
            ("data/new/c.txt", Some("new c")),
        ],
        to_backup: vec![],
        to_live: vec!["data/a.txt", "data/new/c.txt"],
        carry_forward: vec![],
        prune_if_empty: vec![],
        record: RecordChange::Set(json!("new release")),
    }
}

const OLD: [(&str, &str); 3] = [
    ("data/a.txt", "old a"),
    ("data/b.txt", "same b"),
    ("removed.txt", "old only"),
];

fn with_old(extra: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut v = vec![SENTINEL, PLAYER_SAVE, UNKNOWN_PLUGIN];
    v.extend(extra);
    v
}

fn update_case() -> Case {
    Case {
        kind: OperationKind::Update,
        before: with_old(&OLD),
        release: vec![
            ("data/a.txt", "new a"),
            ("data/b.txt", "same b"),
            ("data/new/c.txt", "new c"),
        ],
        live: vec![
            ("data/a.txt", Some("new a")),
            ("data/b.txt", Some("same b")),
            ("data/new/c.txt", Some("new c")),
            ("removed.txt", None),
        ],
        to_backup: vec!["data/a.txt", "removed.txt"],
        to_live: vec!["data/a.txt", "data/new/c.txt"],
        carry_forward: vec![],
        prune_if_empty: vec![],
        record: RecordChange::Set(json!("new release")),
    }
}

fn repair_case() -> Case {
    Case {
        kind: OperationKind::Repair,
        before: with_old(&[("data/a.txt", "damaged"), ("removed.txt", "old only")]),
        release: vec![("data/a.txt", "old a"), ("data/b.txt", "same b")],
        live: vec![
            ("data/a.txt", Some("old a")),
            ("data/b.txt", Some("same b")),
        ],
        to_backup: vec!["data/a.txt"],
        to_live: vec!["data/a.txt", "data/b.txt"],
        carry_forward: vec![],
        prune_if_empty: vec![],
        record: RecordChange::Set(json!("old release")),
    }
}

fn uninstall_case() -> Case {
    Case {
        kind: OperationKind::Uninstall,
        before: with_old(&OLD),
        release: vec![],
        live: vec![
            ("data/a.txt", None),
            ("data/b.txt", None),
            ("removed.txt", None),
            PLAYER_SAVE_PRESENT,
        ],
        to_backup: vec!["data/a.txt", "data/b.txt", "removed.txt"],
        to_live: vec![],
        carry_forward: vec![],
        prune_if_empty: vec!["data"],
        record: RecordChange::Remove,
    }
}

const PLAYER_SAVE_PRESENT: (&str, Option<&str>) = (PLAYER_SAVE.0, Some(PLAYER_SAVE.1));

fn clean_install_case() -> Case {
    Case {
        kind: OperationKind::CleanInstall,
        before: with_old(&[("data/a.txt", "conflicting unknown")]),
        release: vec![("data/a.txt", "new a"), ("data/new/c.txt", "new c")],
        live: vec![
            ("data/a.txt", Some("new a")),
            ("data/new/c.txt", Some("new c")),
            PLAYER_SAVE_PRESENT,
            (DISPOSABLE_SENTINEL, Some("")),
        ],
        to_backup: vec![],
        to_live: vec![],
        carry_forward: vec![PLAYER_SAVE.0],
        prune_if_empty: vec![],
        record: RecordChange::Set(json!("new release")),
    }
}

fn all_cases() -> Vec<Case> {
    vec![
        install_case(),
        update_case(),
        repair_case(),
        uninstall_case(),
        clean_install_case(),
    ]
}

fn write_files(base: &Path, files: &[(&str, &str)]) {
    for (rel, bytes) in files {
        let p = under(base, &RelativePath::new(*rel).unwrap());
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }
}

/// The fixture's caller-owned steps: "download", "extract" and verify.
struct FixtureSteps<'a> {
    case: &'a Case,
    fail_at: Option<(Boundary, ErrorCode)>,
}

impl Steps for FixtureSteps<'_> {
    fn perform(&mut self, b: Boundary, paths: &Paths) -> Result<(), LauncherError> {
        if let Some((at, code)) = self.fail_at {
            if at == b {
                return Err(LauncherError::new(code, format!("fixture fails {b:?}")));
            }
        }
        let mismatch = |what: &str| Err(LauncherError::new(ErrorCode::InventoryMismatch, what));
        match b {
            Boundary::DownloadComplete => fs::write(&paths.archive, b"archive").unwrap(),
            Boundary::Staged => {
                fs::create_dir(&paths.staging).unwrap();
                write_files(&paths.staging, &self.case.release);
            }
            Boundary::StagedInventoryVerified => {
                for (rel, bytes) in &self.case.release {
                    let p = under(&paths.staging, &RelativePath::new(*rel).unwrap());
                    if fs::read(p).ok().as_deref() != Some(bytes.as_bytes()) {
                        return mismatch(rel);
                    }
                }
            }
            Boundary::LiveVerified => {
                for (rel, want) in &self.case.live {
                    let p = under(&paths.root, &RelativePath::new(*rel).unwrap());
                    if fs::read(p).ok().as_deref() != want.map(str::as_bytes) {
                        return mismatch(rel);
                    }
                }
            }
            other => panic!("engine asked the caller for {other:?}"),
        }
        Ok(())
    }
}

/// The installed-release record, as State would keep it.
struct Record(Mutex<Option<serde_json::Value>>);

impl InstalledRecordSink for Record {
    fn commit(&self, _: &Path, change: &RecordChange) -> Result<(), LauncherError> {
        *self.0.lock().unwrap() = match change {
            RecordChange::Set(v) => Some(v.clone()),
            RecordChange::Remove => None,
        };
        Ok(())
    }
}

impl Record {
    fn get(&self) -> Option<serde_json::Value> {
        self.0.lock().unwrap().clone()
    }
}

struct Plenty;

impl Host for Plenty {
    fn free_space(&self, _: &Path) -> io::Result<u64> {
        Ok(u64::MAX)
    }
    fn processes_using(&self, _: &[PathBuf]) -> Vec<String> {
        Vec::new()
    }
}

/// Records every point the transaction consults, failing none.
#[derive(Default)]
struct Recorder(Mutex<Vec<(Boundary, FaultPoint)>>);

impl FaultInjector for Recorder {
    fn check(&self, b: Boundary, p: FaultPoint) -> Result<(), InjectedFault> {
        self.0.lock().unwrap().push((b, p));
        Ok(())
    }
}

impl Recorder {
    fn points(&self) -> Vec<(Boundary, FaultPoint)> {
        self.0.lock().unwrap().clone()
    }
}

/// One disposable product root `<tmp>/PD2R` holding `case.before`.
struct Fixture {
    dir: tempfile::TempDir,
    root: PathBuf,
    record: Record,
}

impl Fixture {
    fn new(case: &Case) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("PD2R");
        fs::create_dir(&root).unwrap();
        write_files(&root, &case.before);
        Self {
            dir,
            root,
            record: Record(Mutex::new(Some(json!("old release")))),
        }
    }

    fn ctx<'a>(&'a self, injector: &'a dyn FaultInjector, host: &'a dyn Host) -> Context<'a> {
        Context {
            injector,
            host,
            sink: &self.record,
        }
    }

    fn plan(case: &Case) -> Plan {
        Plan {
            kind: case.kind,
            to_backup: rels(&case.to_backup),
            to_live: rels(&case.to_live),
            carry_forward: rels(&case.carry_forward),
            prune_if_empty: rels(&case.prune_if_empty),
            record: case.record.clone(),
            space: SpaceNeeds {
                download: 7,
                staging: 10,
                recovery_copy: 10,
            },
        }
    }

    fn run_with(
        &self,
        case: &Case,
        injector: &dyn FaultInjector,
        host: &dyn Host,
        fail_at: Option<(Boundary, ErrorCode)>,
    ) -> Result<(), LauncherError> {
        let guard = WriteGuard::check(&self.root).unwrap();
        let lock = OperationLock::acquire(&guard).unwrap();
        let mut steps = FixtureSteps { case, fail_at };
        run(
            &guard,
            &lock,
            Self::plan(case),
            &mut steps,
            &self.ctx(injector, host),
        )
    }

    fn run(&self, case: &Case, injector: &dyn FaultInjector) -> Result<(), LauncherError> {
        self.run_with(case, injector, &Plenty, None)
    }

    fn recover(&self, injector: &dyn FaultInjector) -> Result<Recovery, LauncherError> {
        recover(&self.root, &self.ctx(injector, &Plenty))
    }

    /// Every file and dir under the temp dir (journal, scratch and backups
    /// included), with the clean-install backup's timestamp normalized and the
    /// lock file left out.
    fn snapshot(&self) -> BTreeMap<String, Option<Vec<u8>>> {
        fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Option<Vec<u8>>>) {
            for e in fs::read_dir(dir).unwrap() {
                let e = e.unwrap();
                let mut name = e.file_name().into_string().unwrap();
                if name.ends_with(".resurrected-lock") {
                    continue;
                }
                if let Some(i) = name.find(".clean-install-backup-") {
                    name.replace_range(i.., ".clean-install-backup-N");
                }
                let rel = format!("{prefix}{name}");
                if e.file_type().unwrap().is_dir() {
                    out.insert(format!("{rel}/"), None);
                    walk(&e.path(), &format!("{rel}/"), out);
                } else {
                    out.insert(rel, Some(fs::read(e.path()).unwrap()));
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(self.dir.path(), "", &mut out);
        out
    }
}

type Snapshot = BTreeMap<String, Option<Vec<u8>>>;

/// The two acceptable end states of `case`: untouched, and completed without
/// faults, each with its installed-release record.
struct Ends {
    before: (Snapshot, Option<serde_json::Value>),
    after: (Snapshot, Option<serde_json::Value>),
    /// Every point the fault-free run consulted, in order.
    points: Vec<(Boundary, FaultPoint)>,
}

fn ends(case: &Case) -> Ends {
    let fx = Fixture::new(case);
    let before = (fx.snapshot(), fx.record.get());
    let recorder = Recorder::default();
    fx.run(case, &recorder).unwrap();
    let after = (fx.snapshot(), fx.record.get());
    assert_ne!(before.0, after.0, "{:?} changed nothing", case.kind);
    Ends {
        before,
        after,
        points: recorder.points(),
    }
}

fn state(fx: &Fixture) -> (Snapshot, Option<serde_json::Value>) {
    (fx.snapshot(), fx.record.get())
}

/// Forward crash points from which recovery must roll forward.
fn rolls_forward_from(kind: OperationKind, b: Boundary) -> bool {
    let seq = Boundary::sequence(kind);
    let pos = |x| seq.iter().position(|y| *y == x).unwrap();
    pos(b) > pos(Boundary::LiveVerified)
}

fn assert_whole(fx: &Fixture, ends: &Ends, want_new: bool, ctx: &str) {
    let got = state(fx);
    let want = if want_new { &ends.after } else { &ends.before };
    assert!(
        got == *want,
        "{ctx}: expected the complete {} installation\n got: {:#?}\nwant: {:#?}",
        if want_new { "new" } else { "previous" },
        got,
        want
    );
}

fn crash_at_every_point_then_recover(case: Case) {
    let ends = ends(&case);
    let seq = Boundary::sequence(case.kind);
    let steps: Vec<_> = ends
        .points
        .iter()
        .filter(|(_, p)| *p == FaultPoint::BeforeStep)
        .map(|(b, _)| *b)
        .collect();
    assert_eq!(steps, seq, "every boundary is reported in order");

    for &(b, p) in &ends.points {
        let ctx = format!("{:?} crash at {b:?} {p:?}", case.kind);
        let fx = Fixture::new(&case);
        let err = fx
            .run(
                &case,
                &FailAt {
                    boundary: b,
                    point: p,
                },
            )
            .unwrap_err();
        assert!(err.message.contains("injected fault"), "{ctx}: {err}");
        let want_new = rolls_forward_from(case.kind, b);
        let expected = if want_new {
            Recovery::RolledForward
        } else {
            Recovery::RolledBack
        };
        assert_eq!(fx.recover(&NoFaults), Ok(expected), "{ctx}");
        assert_whole(&fx, &ends, want_new, &ctx);
        assert_eq!(fx.recover(&NoFaults), Ok(Recovery::Nothing), "{ctx}");
    }
}

/// For each forward crash, crash recovery at every point it consults, then
/// recover again.
fn crash_recovery_at_every_point_then_rerun(case: Case) {
    let ends = ends(&case);
    for &(b, p) in &ends.points {
        let crash = FailAt {
            boundary: b,
            point: p,
        };
        let fx = Fixture::new(&case);
        fx.run(&case, &crash).unwrap_err();
        let recorder = Recorder::default();
        fx.recover(&recorder).unwrap();
        let want_new = rolls_forward_from(case.kind, b);
        for (rb, rp) in recorder.points() {
            let ctx = format!(
                "{:?} crash at {b:?} {p:?}, recovery crash at {rb:?} {rp:?}",
                case.kind
            );
            let fx = Fixture::new(&case);
            fx.run(&case, &crash).unwrap_err();
            let err = fx
                .recover(&FailAt {
                    boundary: rb,
                    point: rp,
                })
                .unwrap_err();
            assert!(err.message.contains("injected fault"), "{ctx}: {err}");
            assert!(fx.recover(&NoFaults).is_ok(), "{ctx}");
            assert_whole(&fx, &ends, want_new, &ctx);
        }
    }
}

#[test]
fn install_crash_at_every_boundary_recovers_whole() {
    crash_at_every_point_then_recover(install_case());
}

#[test]
fn update_crash_at_every_boundary_recovers_whole() {
    crash_at_every_point_then_recover(update_case());
}

#[test]
fn repair_crash_at_every_boundary_recovers_whole() {
    crash_at_every_point_then_recover(repair_case());
}

#[test]
fn uninstall_crash_at_every_boundary_recovers_whole() {
    crash_at_every_point_then_recover(uninstall_case());
}

#[test]
fn clean_install_crash_at_every_boundary_recovers_whole() {
    crash_at_every_point_then_recover(clean_install_case());
}

#[test]
fn install_recovery_crashed_and_rerun_recovers_whole() {
    crash_recovery_at_every_point_then_rerun(install_case());
}

#[test]
fn update_recovery_crashed_and_rerun_recovers_whole() {
    crash_recovery_at_every_point_then_rerun(update_case());
}

#[test]
fn repair_recovery_crashed_and_rerun_recovers_whole() {
    crash_recovery_at_every_point_then_rerun(repair_case());
}

#[test]
fn uninstall_recovery_crashed_and_rerun_recovers_whole() {
    crash_recovery_at_every_point_then_rerun(uninstall_case());
}

#[test]
fn clean_install_recovery_crashed_and_rerun_recovers_whole() {
    crash_recovery_at_every_point_then_rerun(clean_install_case());
}

#[test]
fn completed_operations_keep_player_files_and_the_clean_install_backup() {
    for case in all_cases() {
        let fx = Fixture::new(&case);
        fx.run(&case, &NoFaults).unwrap();
        let snap = fx.snapshot();
        assert_eq!(
            snap.get(&format!("PD2R/{}", PLAYER_SAVE.0)),
            Some(&Some(PLAYER_SAVE.1.as_bytes().to_vec())),
            "{:?}",
            case.kind
        );
        let kept = snap
            .keys()
            .any(|k| k.starts_with("PD2R.clean-install-backup-N/"));
        assert_eq!(
            kept,
            case.kind == OperationKind::CleanInstall,
            "{:?}",
            case.kind
        );
        let plugin = format!("PD2R/{}", UNKNOWN_PLUGIN.0);
        assert_eq!(
            snap.contains_key(&plugin),
            case.kind != OperationKind::CleanInstall,
            "unknown plugin stays in place, or in the clean-install backup"
        );
        assert!(
            !snap
                .keys()
                .any(|k| k.starts_with("PD2R.resurrected-") || k.contains(".resurrected-tmp")),
            "{:?} left scratch: {snap:#?}",
            case.kind
        );
    }
    let fx = Fixture::new(&uninstall_case());
    fx.run(&uninstall_case(), &NoFaults).unwrap();
    assert!(!fx.root.join("data").exists(), "empty managed dir pruned");
    assert_eq!(fx.record.get(), None);
}

#[test]
fn failure_before_live_verified_restores_the_previous_installation() {
    for case in all_cases() {
        let ends = ends(&case);
        for (b, code) in [
            (Boundary::DownloadComplete, ErrorCode::Cancelled),
            (
                Boundary::StagedInventoryVerified,
                ErrorCode::InventoryMismatch,
            ),
            (Boundary::LiveVerified, ErrorCode::InventoryMismatch),
        ] {
            if !Boundary::sequence(case.kind).contains(&b) {
                continue;
            }
            let fx = Fixture::new(&case);
            let err = fx
                .run_with(&case, &NoFaults, &Plenty, Some((b, code)))
                .unwrap_err();
            assert_eq!(err.code, code);
            assert_whole(&fx, &ends, false, &format!("{:?} fails {b:?}", case.kind));
        }
    }
}

#[test]
fn crash_during_in_process_rollback_is_resolved_by_recovery() {
    let case = update_case();
    let ends = ends(&case);
    let fx = Fixture::new(&case);
    let crash = FailAt {
        boundary: Boundary::BackupRestoredToLive,
        point: FaultPoint::AfterEntry(0),
    };
    let fail = Some((Boundary::LiveVerified, ErrorCode::InventoryMismatch));
    fx.run_with(&case, &crash, &Plenty, fail).unwrap_err();
    assert_eq!(fx.recover(&NoFaults), Ok(Recovery::RolledBack));
    assert_whole(&fx, &ends, false, "rollback crash");
}

/// Free space that runs out after the first preflight.
struct Shrinking(Mutex<u64>);

impl Host for Shrinking {
    fn free_space(&self, _: &Path) -> io::Result<u64> {
        let mut calls = self.0.lock().unwrap();
        *calls += 1;
        Ok(if *calls == 1 { u64::MAX } else { 0 })
    }
    fn processes_using(&self, _: &[PathBuf]) -> Vec<String> {
        Vec::new()
    }
}

struct Tight;

impl Host for Tight {
    fn free_space(&self, _: &Path) -> io::Result<u64> {
        Ok(26) // download + staging + recovery copy is 27
    }
    fn processes_using(&self, _: &[PathBuf]) -> Vec<String> {
        Vec::new()
    }
}

struct GameRunning;

impl Host for GameRunning {
    fn free_space(&self, _: &Path) -> io::Result<u64> {
        Ok(u64::MAX)
    }
    fn processes_using(&self, files: &[PathBuf]) -> Vec<String> {
        files
            .iter()
            .map(|f| format!("D2R.exe has {}", f.display()))
            .collect()
    }
}

#[test]
fn preflight_refusals_leave_the_installation_unchanged() {
    let case = update_case();
    let ends = ends(&case);
    for (host, code) in [
        (&Tight as &dyn Host, ErrorCode::InsufficientSpace),
        (&GameRunning, ErrorCode::GameRunning),
        (&Shrinking(Mutex::new(0)), ErrorCode::InsufficientSpace),
    ] {
        let fx = Fixture::new(&case);
        let err = fx.run_with(&case, &NoFaults, host, None).unwrap_err();
        assert_eq!(err.code, code, "{err}");
        assert_whole(&fx, &ends, false, &format!("{code:?}"));
    }
}

#[cfg(windows)]
#[test]
fn access_denied_leaves_the_installation_unchanged() {
    let case = update_case();
    let ends = ends(&case);
    let fx = Fixture::new(&case);
    let icacls = |args: &[&str]| {
        std::process::Command::new("icacls")
            .arg(&fx.root)
            .args(args)
            .output()
            .is_ok_and(|o| o.status.success())
    };
    // Deny Everyone (S-1-1-0) adding files to the product root itself.
    if !icacls(&["/deny", "*S-1-1-0:(WD)"]) {
        eprintln!("SKIPPED access_denied_leaves_the_installation_unchanged: icacls failed");
        return;
    }
    let result = fx.run(&case, &NoFaults);
    assert!(
        icacls(&["/remove:d", "*S-1-1-0"]),
        "could not restore the ACL"
    );
    assert_eq!(result.unwrap_err().code, ErrorCode::AccessDenied);
    assert_whole(&fx, &ends, false, "access denied");
}

#[test]
fn second_writer_is_refused_while_an_operation_holds_the_lock() {
    let case = update_case();
    let fx = Fixture::new(&case);
    let guard = WriteGuard::check(&fx.root).unwrap();
    let _held = OperationLock::acquire(&guard).unwrap();
    assert_eq!(
        OperationLock::acquire(&guard).unwrap_err().code,
        ErrorCode::OperationInProgress
    );
    // Recovery is a writer too.
    let crash = FailAt {
        boundary: Boundary::Staged,
        point: FaultPoint::AfterStep,
    };
    drop(_held);
    fx.run(&case, &crash).unwrap_err();
    let _held = OperationLock::acquire(&guard).unwrap();
    assert_eq!(
        fx.recover(&NoFaults).unwrap_err().code,
        ErrorCode::OperationInProgress
    );
}

#[test]
fn interrupted_transaction_blocks_new_operations_until_recovered() {
    let case = update_case();
    let fx = Fixture::new(&case);
    let crash = FailAt {
        boundary: Boundary::StagedMovedToLive,
        point: FaultPoint::AfterEntry(1),
    };
    fx.run(&case, &crash).unwrap_err();
    assert_eq!(
        fx.run(&case, &NoFaults).unwrap_err().code,
        ErrorCode::RecoveryRequired
    );
    fx.recover(&NoFaults).unwrap();
    fx.run(&case, &NoFaults).unwrap();
}

#[test]
fn unreadable_journal_requires_recovery_and_changes_nothing() {
    let case = update_case();
    let fx = Fixture::new(&case);
    fx.run(
        &case,
        &FailAt {
            boundary: Boundary::LiveMovedToBackup,
            point: FaultPoint::AfterEntry(0),
        },
    )
    .unwrap_err();
    let journal = journal_path(WriteGuard::check(&fx.root).unwrap().root());
    fs::write(&journal, b"{ torn").unwrap();
    let before = fx.snapshot();
    assert_eq!(
        fx.recover(&NoFaults).unwrap_err().code,
        ErrorCode::RecoveryRequired
    );
    assert_eq!(fx.snapshot(), before);
}

/// A crash leaves `data/a.txt` in `B`; then something appears at its live path.
/// Restoring would clobber it, so recovery stops without further changes.
#[test]
fn conflict_stops_recovery_with_recovery_required() {
    let case = update_case();
    let fx = Fixture::new(&case);
    fx.run(
        &case,
        &FailAt {
            boundary: Boundary::LiveMovedToBackup,
            point: FaultPoint::AfterEntry(0),
        },
    )
    .unwrap_err();
    fs::write(fx.root.join("data/a.txt"), b"appeared").unwrap();
    let before = fx.snapshot();
    for _ in 0..2 {
        assert_eq!(
            fx.recover(&NoFaults).unwrap_err().code,
            ErrorCode::RecoveryRequired
        );
    }
    let after = fx.snapshot();
    let changed: Vec<_> = after
        .iter()
        .filter(|(k, v)| before.get(*k) != Some(*v) && !k.contains(".resurrected-journal"))
        .collect();
    assert!(changed.is_empty(), "{changed:#?}");
    assert_eq!(fs::read(fx.root.join("data/a.txt")).unwrap(), b"appeared");
}

#[test]
fn installed_record_is_untouched_until_live_verified() {
    let case = update_case();
    let fx = Fixture::new(&case);
    fx.run(
        &case,
        &FailAt {
            boundary: Boundary::LiveVerified,
            point: FaultPoint::AfterStep,
        },
    )
    .unwrap_err();
    assert_eq!(fx.record.get(), Some(json!("old release")));
    assert_eq!(fx.recover(&NoFaults), Ok(Recovery::RolledBack));
    assert_eq!(fx.record.get(), Some(json!("old release")));
}

#[test]
fn product_root_without_sentinel_is_never_recovered() {
    let case = update_case();
    let fx = Fixture::new(&case);
    fx.run(
        &case,
        &FailAt {
            boundary: Boundary::StagedMovedToLive,
            point: FaultPoint::BeforeStep,
        },
    )
    .unwrap_err();
    fs::remove_file(fx.root.join(DISPOSABLE_SENTINEL)).unwrap();
    assert_eq!(
        fx.recover(&NoFaults).unwrap_err().code,
        ErrorCode::ProductRootNotDisposable
    );
}

/// Release builds flush every write; run a clean install (copies included) and a
/// crashed-then-recovered update on that path.
#[test]
fn clean_install_and_recovery_succeed_with_real_flushes() {
    crate::install::journal::flush_on_this_thread();
    let case = clean_install_case();
    let fx = Fixture::new(&case);
    fx.run(&case, &NoFaults).unwrap();
    assert_eq!(fx.record.get(), Some(json!("new release")));

    let case = update_case();
    let ends = ends(&case);
    let fx = Fixture::new(&case);
    let crash = FailAt {
        boundary: Boundary::StagedMovedToLive,
        point: FaultPoint::AfterEntry(1),
    };
    fx.run(&case, &crash).unwrap_err();
    assert_eq!(fx.recover(&NoFaults), Ok(Recovery::RolledBack));
    assert_whole(&fx, &ends, false, "flushed recovery");
}

/// A player file in the old root named like a carry-forward temp must survive the
/// rollback that puts the old root back at `L`.
#[test]
fn clean_install_rollback_keeps_player_file_named_like_a_copy_temp() {
    let mut case = clean_install_case();
    let lookalike = ("save/hero.d2s.resurrected-tmp", "player's own file");
    case.before.push(lookalike);
    let fx = Fixture::new(&case);
    let crash = FailAt {
        boundary: Boundary::PlayerFilesCarriedForward,
        point: FaultPoint::AfterEntry(0),
    };
    fx.run(&case, &crash).unwrap_err();
    assert_eq!(fx.recover(&NoFaults), Ok(Recovery::RolledBack));
    assert_eq!(
        fs::read(fx.root.join(lookalike.0)).unwrap(),
        lookalike.1.as_bytes()
    );
}

/// `L/data` is a junction to a folder outside the root: no planned move may go
/// through it, so that folder and the installation stay untouched.
#[cfg(windows)]
#[test]
fn junction_inside_the_product_root_is_never_written_through() {
    for case in [update_case(), install_case()] {
        let fx = Fixture::new(&case);
        let outside = fx.dir.path().join("elsewhere");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("a.txt"), b"not ours").unwrap();
        if fx.root.join("data").exists() {
            fs::remove_dir_all(fx.root.join("data")).unwrap();
        }
        let made = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(fx.root.join("data"))
            .arg(&outside)
            .output()
            .is_ok_and(|o| o.status.success());
        if !made {
            eprintln!("SKIPPED junction_inside_the_product_root_is_never_written_through: mklink /J failed");
            return;
        }
        let err = fx.run(&case, &NoFaults).unwrap_err();
        assert_eq!(
            err.code,
            ErrorCode::ProductRootNotDisposable,
            "{:?}: {err}",
            case.kind
        );
        assert_eq!(fs::read(outside.join("a.txt")).unwrap(), b"not ours");
        assert_eq!(
            fs::read_dir(&outside).unwrap().count(),
            1,
            "{:?}",
            case.kind
        );
        let paths = Paths::for_root(WriteGuard::check(&fx.root).unwrap().root());
        assert!(!paths.journal().exists(), "rolled back and closed");
        assert!(!paths.backup.exists() && !paths.staging.exists());
        fs::remove_dir(fx.root.join("data")).unwrap(); // the junction, not its target
    }
}

#[test]
fn recovery_refuses_while_the_game_runs_and_keeps_the_journal() {
    let case = update_case();
    let ends = ends(&case);
    let fx = Fixture::new(&case);
    let crash = FailAt {
        boundary: Boundary::BackupRestoredToLive,
        point: FaultPoint::AfterEntry(0),
    };
    let fail = Some((Boundary::LiveVerified, ErrorCode::InventoryMismatch));
    fx.run_with(&case, &crash, &Plenty, fail).unwrap_err();
    let before = fx.snapshot();
    let err = recover(&fx.root, &fx.ctx(&NoFaults, &GameRunning)).unwrap_err();
    assert_eq!(err.code, ErrorCode::GameRunning, "{err}");
    assert_eq!(fx.snapshot(), before, "recovery wrote while the game ran");
    assert_eq!(fx.recover(&NoFaults), Ok(Recovery::RolledBack));
    assert_whole(&fx, &ends, false, "recovery after the game exits");
}
