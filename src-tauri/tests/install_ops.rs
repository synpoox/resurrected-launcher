//! Install, adopt, repair, update, uninstall and clean install (#37) end to end
//! through the operation interface, the fixture HTTPS server, the pinned 7-Zip and
//! disposable installations in real temp dirs. Preservation outcomes are checked
//! byte for byte against the whole product root.

mod support;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use resurrected_launcher_lib::catalog::ReleaseManifest;
use resurrected_launcher_lib::contract::{
    Boundary, ErrorCode, FailAt, FaultInjector, FaultPoint, LauncherError, NoFaults, OperationKind,
    RelativePath, DISPOSABLE_SENTINEL,
};
use resurrected_launcher_lib::install::download::Downloader;
use resurrected_launcher_lib::install::extract::SevenZip;
use resurrected_launcher_lib::install::ops::{self, Env, Report};
use resurrected_launcher_lib::install::plan::{Conflict, Request};
use resurrected_launcher_lib::install::preflight::SystemHost;
use resurrected_launcher_lib::install::transaction::Recovery;
use resurrected_launcher_lib::state::StateStore;
use support::*;

use OperationKind::*;

const CONFIG: &str = "d2rloader/config.ini";
const SAVE: &str = "save/Hero.d2s";

/// One disposable D2R installation, the fixture server and launcher state.
struct Rig {
    game: DisposableGame,
    server: FixtureServer,
    downloader: Downloader,
    seven_zip: SevenZip,
    _state_dir: tempfile::TempDir,
    state: Mutex<StateStore>,
    cancel: AtomicBool,
}

impl Rig {
    fn new() -> Self {
        let server = FixtureServer::start();
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

    fn env<'a>(&'a self, injector: &'a dyn FaultInjector) -> Env<'a> {
        Env {
            downloader: &self.downloader,
            seven_zip: &self.seven_zip,
            state: &self.state,
            host: &SystemHost,
            injector,
            cancel: &self.cancel,
        }
    }

    /// The fixture release with its archive served by this rig's server.
    fn served(&self, name: &str) -> (FixtureRelease, ReleaseManifest) {
        let release = release(name);
        let mut manifest = release.manifest.clone();
        manifest.archive = self.server.serve(&release);
        (release, manifest)
    }

    fn request<'a>(
        &'a self,
        kind: OperationKind,
        release: &FixtureRelease,
        manifest: &'a ReleaseManifest,
        installed: Option<&'a ReleaseManifest>,
    ) -> Request<'a> {
        Request {
            kind,
            d2r: self.game.d2r(),
            release: manifest,
            release_sha256: sha256(&release.manifest_json),
            installed,
            acknowledge_save_breaking: false,
            consent_clean_install: false,
        }
    }

    fn run(&self, req: &Request) -> Result<Report, LauncherError> {
        ops::run(&self.env(&NoFaults), req)
    }

    fn installed_release(&self) -> Option<String> {
        let state = self.state.lock().unwrap();
        state
            .installed_releases()
            .iter()
            .find(|r| r.product_id.0 == "pd2r")
            .map(|r| r.release_id.clone())
    }

    fn archive_hits(&self, release: &FixtureRelease) -> usize {
        let path = release.archive_path();
        self.server.hits().iter().filter(|h| **h == path).count()
    }
}

/// Every file (bytes) and dir (`None`) under `dir`, by `/`-separated path.
fn tree(dir: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn go(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.file_type().unwrap().is_dir() {
                go(&entry.path(), &rel, out);
                out.insert(rel, None);
            } else {
                out.insert(rel, Some(fs::read(entry.path()).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    go(dir, "", &mut out);
    out
}

/// The tree an installed `release` has: its payload, its dirs and the sentinel.
fn release_tree(release: &FixtureRelease) -> BTreeMap<String, Option<Vec<u8>>> {
    let mut out = BTreeMap::new();
    for (rel, bytes) in &release.payload {
        out.insert(rel.clone(), Some(bytes.clone()));
        for (i, _) in rel.match_indices('/') {
            out.insert(rel[..i].to_owned(), None);
        }
    }
    for d in release.manifest.inventory.empty_dirs() {
        out.insert(d.to_string(), None);
    }
    out.insert(DISPOSABLE_SENTINEL.to_owned(), Some(Vec::new()));
    out
}

fn with_file(
    mut t: BTreeMap<String, Option<Vec<u8>>>,
    rel: &str,
    bytes: &[u8],
) -> BTreeMap<String, Option<Vec<u8>>> {
    for (i, _) in rel.match_indices('/') {
        t.insert(rel[..i].to_owned(), None);
    }
    t.insert(rel.to_owned(), Some(bytes.to_vec()));
    t
}

fn add_save(root: &Path) -> Vec<u8> {
    let bytes = b"character save, never converted".to_vec();
    fs::create_dir_all(root.join("save")).unwrap();
    fs::write(root.join(SAVE), &bytes).unwrap();
    bytes
}

/// Nothing but the product root and the lock is left beside it.
fn assert_no_scratch(root: &Path) {
    let name = root.file_name().unwrap().to_str().unwrap().to_owned();
    let left: Vec<String> = fs::read_dir(root.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with(&name) && *n != name && *n != format!("{name}.resurrected-lock"))
        .collect();
    assert!(
        left.is_empty(),
        "scratch left beside the product root: {left:?}"
    );
}

fn code(r: Result<Report, LauncherError>) -> ErrorCode {
    r.unwrap_err().code
}

fn rel(p: &str) -> RelativePath {
    RelativePath::new(p).unwrap()
}

/// Installs PD2R fixture 1 through the operation interface.
fn install_1(rig: &Rig) -> (FixtureRelease, ReleaseManifest) {
    let (r1, m1) = rig.served(PD2R_1);
    rig.game.product_root(&r1);
    rig.run(&rig.request(Install, &r1, &m1, None)).unwrap();
    (r1, m1)
}

#[test]
fn install_puts_the_release_in_place_keeps_unknown_plugin_and_records_it_installed() {
    let rig = Rig::new();
    let (r1, m1) = rig.served(PD2R_1);
    let root = rig.game.product_root(&r1);
    let plugin = add_unknown_plugin(&root);

    let report = rig.run(&rig.request(Install, &r1, &m1, None)).unwrap();

    let want = with_file(release_tree(&r1), UNKNOWN_PLUGIN, &plugin);
    assert_eq!(tree(&root), want);
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_1));
    assert_eq!(rig.archive_hits(&r1), 1);
    assert!(report.preserved.is_empty() && report.clean_install_backups.is_empty());
    assert_no_scratch(&root);
}

#[test]
fn update_replaces_untouched_shipped_defaults_using_the_previous_published_hash() {
    let rig = Rig::new();
    let (_, m1) = install_1(&rig);
    let (r2, m2) = rig.served(PD2R_2);
    let root = product_root(&rig, &m2);
    let plugin = add_unknown_plugin(&root);
    let save = add_save(&root);

    let report = rig.run(&rig.request(Update, &r2, &m2, Some(&m1))).unwrap();

    // The loot filter still had fixture 1's published bytes, so it is updated;
    // retired.txt is removed, weapons.txt added; plugin and save untouched.
    let want = with_file(release_tree(&r2), UNKNOWN_PLUGIN, &plugin);
    assert_eq!(tree(&root), with_file(want, SAVE, &save));
    assert!(report.preserved.is_empty());
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_2));
    assert_no_scratch(&root);
}

fn product_root(rig: &Rig, m: &ReleaseManifest) -> std::path::PathBuf {
    resurrected_launcher_lib::install::plan::product_root(rig.game.d2r(), m)
}

#[test]
fn update_keeps_edited_loot_filter_and_compatible_edited_config_byte_for_byte() {
    let rig = Rig::new();
    let (r1, m1) = install_1(&rig);
    let (r2, m2) = rig.served(PD2R_2);
    let root = product_root(&rig, &m2);
    let filter = edit_loot_filter(&root);
    // An edit that still holds every mandatory setting.
    let mut config = r1.payload[CONFIG].clone();
    config.extend_from_slice(b"; my note\nwindowed=1\n");
    fs::write(root.join(CONFIG), &config).unwrap();
    let plugin = add_unknown_plugin(&root);

    let report = rig.run(&rig.request(Update, &r2, &m2, Some(&m1))).unwrap();

    let want = with_file(release_tree(&r2), LOOT_FILTER, &filter);
    let want = with_file(want, CONFIG, &config);
    assert_eq!(tree(&root), with_file(want, UNKNOWN_PLUGIN, &plugin));
    assert_eq!(report.preserved, [rel(CONFIG), rel(LOOT_FILTER)]);
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_2));
}

#[test]
fn update_with_edited_config_conflicting_with_mandatory_setting_pauses_with_conflict() {
    let rig = Rig::new();
    let (r1, m1) = install_1(&rig);
    let (r2, m2) = rig.served(PD2R_2);
    let root = product_root(&rig, &m2);
    fs::write(
        root.join(CONFIG),
        b"[loader]\ndefault_mod=Other\nsave_format=pd2r\n",
    )
    .unwrap();
    let before = tree(&root);

    let insp = ops::inspect(
        &rig.env(&NoFaults),
        &rig.request(Update, &r2, &m2, Some(&m1)),
    )
    .unwrap();
    assert_eq!(
        insp.conflicts,
        [Conflict::Setting {
            file: rel(CONFIG),
            key: "default_mod".into(),
            required: "PD2R".into(),
            found: Some("Other".into()),
        }]
    );
    let err = code(rig.run(&rig.request(Update, &r2, &m2, Some(&m1))));
    assert_eq!(err, ErrorCode::MandatorySettingConflict);

    assert_eq!(tree(&root), before, "nothing changed");
    assert_eq!(rig.archive_hits(&r2), 0, "paused before downloading");
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_1));
    assert_no_scratch(&root);
    let _ = r1;
}

#[test]
fn clean_install_resolves_a_mandatory_setting_conflict_leaving_the_edit_in_the_backup() {
    let rig = Rig::new();
    let (_, m1) = install_1(&rig);
    let (r2, m2) = rig.served(PD2R_2);
    let root = product_root(&rig, &m2);
    let edited = b"[loader]
default_mod=Other
save_format=pd2r
";
    fs::write(root.join(CONFIG), edited).unwrap();
    let before = tree(&root);
    let update = rig.request(Update, &r2, &m2, Some(&m1));
    assert_eq!(code(rig.run(&update)), ErrorCode::MandatorySettingConflict);

    let mut clean = rig.request(CleanInstall, &r2, &m2, Some(&m1));
    clean.consent_clean_install = true;
    let insp = ops::inspect(&rig.env(&NoFaults), &clean).unwrap();
    assert!(insp.conflicts.is_empty() && insp.preserved.is_empty());
    assert_eq!(insp.backup_only, [rel(CONFIG)]);
    let report = rig.run(&clean).unwrap();

    // The new default is live; the player's edit is only in the labelled backup.
    assert_eq!(tree(&root), release_tree(&r2));
    assert!(report.preserved.is_empty());
    let [backup] = &report.clean_install_backups[..] else {
        panic!("one labelled backup: {:?}", report.clean_install_backups);
    };
    assert_eq!(tree(backup), before);
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_2));
}

#[test]
fn update_keeps_and_reports_a_changed_file_the_new_release_drops() {
    let rig = Rig::new();
    let (_, m1) = install_1(&rig);
    let (r2, m2) = rig.served(PD2R_2);
    let root = product_root(&rig, &m2);
    let retired = "PD2R.mpq/data/global/excel/retired.txt";
    fs::write(root.join(retired), b"player-changed retired file").unwrap();

    let report = rig.run(&rig.request(Update, &r2, &m2, Some(&m1))).unwrap();

    let want = with_file(release_tree(&r2), retired, b"player-changed retired file");
    assert_eq!(tree(&root), want);
    assert_eq!(report.preserved, [rel(retired)]);
}

#[test]
fn save_breaking_release_is_refused_without_acknowledgement_and_saves_are_untouched() {
    let rig = Rig::new();
    let (r2, m2) = rig.served(PD2R_2);
    let root = rig.game.installed(&r2);
    rig.run(&rig.request(Adopt, &r2, &m2, None)).unwrap();
    let save = add_save(&root);
    let (r3, m3) = rig.served(PD2R_3);
    let before = tree(&root);

    let insp = ops::inspect(
        &rig.env(&NoFaults),
        &rig.request(Update, &r3, &m3, Some(&m2)),
    )
    .unwrap();
    assert!(insp.save_breaking_notice.is_some());
    let refused = rig.run(&rig.request(Update, &r3, &m3, Some(&m2)));
    assert_eq!(code(refused), ErrorCode::SaveBreakingNotAcknowledged);
    assert_eq!(tree(&root), before);
    assert_eq!(rig.archive_hits(&r3), 0);

    let mut ack = rig.request(Update, &r3, &m3, Some(&m2));
    ack.acknowledge_save_breaking = true;
    rig.run(&ack).unwrap();
    // The untouched config default moves to fixture 3's save format; the save's
    // bytes are exactly what they were (no conversion).
    assert_eq!(tree(&root), with_file(release_tree(&r3), SAVE, &save));
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_3));
}

#[test]
fn adoption_recognises_a_matching_installation_and_changes_no_file() {
    let rig = Rig::new();
    let (r1, m1) = rig.served(PD2R_1);
    let root = rig.game.installed(&r1);
    edit_loot_filter(&root);
    add_unknown_plugin(&root);
    let before = tree(&root);

    let report = rig.run(&rig.request(Adopt, &r1, &m1, None)).unwrap();

    assert_eq!(tree(&root), before);
    assert_eq!(report.preserved, [rel(LOOT_FILTER)]);
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_1));
    assert_eq!(rig.archive_hits(&r1), 0, "adoption downloads nothing");
}

#[test]
fn adoption_of_a_damaged_installation_is_refused_without_a_record() {
    let rig = Rig::new();
    let (r1, m1) = rig.served(PD2R_1);
    let root = rig.game.installed(&r1);
    fs::write(root.join("PD2R.mpq/modinfo.json"), b"damaged").unwrap();
    let before = tree(&root);

    let err = code(rig.run(&rig.request(Adopt, &r1, &m1, None)));

    assert_eq!(err, ErrorCode::InventoryMismatch);
    assert_eq!(tree(&root), before);
    assert_eq!(rig.installed_release(), None);
    assert_no_scratch(&root);
}

#[test]
fn repair_restores_release_owned_files_only_and_keeps_player_files() {
    let rig = Rig::new();
    let (r1, m1) = install_1(&rig);
    let root = product_root(&rig, &m1);
    fs::write(
        root.join("PD2R.mpq/data/global/excel/armor.txt"),
        b"damaged",
    )
    .unwrap();
    fs::remove_file(root.join("PD2R.mpq/modinfo.json")).unwrap();
    fs::remove_file(root.join(CONFIG)).unwrap();
    fs::remove_dir(root.join("d2rloader/patches")).unwrap();
    let filter = edit_loot_filter(&root);
    let plugin = add_unknown_plugin(&root);

    let report = rig.run(&rig.request(Repair, &r1, &m1, Some(&m1))).unwrap();

    let want = with_file(release_tree(&r1), LOOT_FILTER, &filter);
    assert_eq!(tree(&root), with_file(want, UNKNOWN_PLUGIN, &plugin));
    assert_eq!(report.preserved, [rel(LOOT_FILTER)]);
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_1));
    assert_no_scratch(&root);
}

#[test]
fn uninstall_removes_only_positively_identified_managed_files_and_reports_player_files() {
    let rig = Rig::new();
    let (r1, m1) = install_1(&rig);
    let root = product_root(&rig, &m1);
    // Another product, the loader and the game itself must stay untouched.
    let forever = release(FOREVER_1);
    let forever_root = rig.game.installed(&forever);
    let loader = rig.game.d2r().join("D2RLoader.dll");
    fs::write(&loader, b"shared loader").unwrap();
    let outside = tree(rig.game.d2r());
    let filter = edit_loot_filter(&root);
    let plugin = add_unknown_plugin(&root);
    let changed = b"player-changed managed file".to_vec();
    fs::write(root.join("PD2R.mpq/data/global/excel/armor.txt"), &changed).unwrap();

    let report = rig
        .run(&rig.request(Uninstall, &r1, &m1, Some(&m1)))
        .unwrap();

    let mut want = BTreeMap::new();
    want.insert(DISPOSABLE_SENTINEL.to_owned(), Some(Vec::new()));
    let want = with_file(want, LOOT_FILTER, &filter);
    let want = with_file(want, UNKNOWN_PLUGIN, &plugin);
    let want = with_file(want, "PD2R.mpq/data/global/excel/armor.txt", &changed);
    assert_eq!(tree(&root), want);
    assert_eq!(
        report.remaining,
        [
            DISPOSABLE_SENTINEL,
            "PD2R.mpq/data/global/excel/armor.txt",
            UNKNOWN_PLUGIN,
            LOOT_FILTER,
        ]
    );
    assert_eq!(report.product_root.file_name(), root.file_name());
    assert_eq!(rig.installed_release(), None);
    assert_eq!(tree(&forever_root), release_tree(&forever));
    let outside_after: BTreeMap<_, _> = tree(rig.game.d2r())
        .into_iter()
        .filter(|(k, _)| !k.starts_with("mods/PD2R"))
        .collect();
    let outside: BTreeMap<_, _> = outside
        .into_iter()
        .filter(|(k, _)| !k.starts_with("mods/PD2R"))
        .collect();
    assert_eq!(outside_after, outside);
    assert_eq!(fs::read(&loader).unwrap(), b"shared loader");
    assert_no_scratch(&root);
}

#[test]
fn unknown_file_conflict_offers_clean_install_which_keeps_a_labelled_backup() {
    let rig = Rig::new();
    let (r1, m1) = rig.served(PD2R_1);
    let root = rig.game.product_root(&r1);
    let armor = "PD2R.mpq/data/global/excel/armor.txt";
    let old = with_file(BTreeMap::new(), armor, b"someone else's armor");
    for (rel, bytes) in &old {
        if let Some(b) = bytes {
            fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
            fs::write(root.join(rel), b).unwrap();
        }
    }
    let filter = {
        fs::create_dir_all(root.join("lootfilter")).unwrap();
        edit_loot_filter(&root)
    };
    let plugin = add_unknown_plugin(&root);
    let before = tree(&root);

    let insp = ops::inspect(&rig.env(&NoFaults), &rig.request(Install, &r1, &m1, None)).unwrap();
    assert_eq!(insp.conflicts, [Conflict::UnknownFile { path: rel(armor) }]);
    // Consent is informed: what stays only in the backup is listed up front.
    let clean = rig.request(CleanInstall, &r1, &m1, None);
    let insp = ops::inspect(&rig.env(&NoFaults), &clean).unwrap();
    assert!(insp.conflicts.is_empty());
    assert_eq!(insp.backup_only, [rel(armor), rel(UNKNOWN_PLUGIN)]);
    assert_eq!(
        code(rig.run(&rig.request(Install, &r1, &m1, None))),
        ErrorCode::InventoryMismatch
    );
    assert_eq!(
        code(rig.run(&rig.request(CleanInstall, &r1, &m1, None))),
        ErrorCode::CleanInstallConsentRequired
    );
    assert_eq!(tree(&root), before);

    let mut consent = rig.request(CleanInstall, &r1, &m1, None);
    consent.consent_clean_install = true;
    let report = rig.run(&consent).unwrap();

    // The edited loot filter is carried forward; the plugin stays in the backup.
    assert_eq!(
        tree(&root),
        with_file(release_tree(&r1), LOOT_FILTER, &filter)
    );
    assert_eq!(report.preserved, [rel(LOOT_FILTER)]);
    let [backup] = &report.clean_install_backups[..] else {
        panic!("one labelled backup: {:?}", report.clean_install_backups);
    };
    assert_eq!(tree(backup), before, "the old folder, byte for byte");
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_1));
    let _ = plugin;

    // A later operation never deletes the labelled backup.
    let (r2, m2) = rig.served(PD2R_2);
    let report = rig.run(&rig.request(Update, &r2, &m2, Some(&m1))).unwrap();
    assert_eq!(&report.clean_install_backups, std::slice::from_ref(backup));
    assert_eq!(tree(backup), before);
}

#[test]
fn cancel_before_download_leaves_the_installation_unchanged() {
    let rig = Rig::new();
    let (_, m1) = install_1(&rig);
    let (r2, m2) = rig.served(PD2R_2);
    let root = product_root(&rig, &m2);
    let before = tree(&root);
    rig.cancel.store(true, Ordering::Relaxed);

    let err = code(rig.run(&rig.request(Update, &r2, &m2, Some(&m1))));

    assert_eq!(err, ErrorCode::Cancelled);
    assert_eq!(tree(&root), before);
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_1));
    assert_no_scratch(&root);
}

#[test]
fn crash_during_activation_recovers_the_previous_installation_byte_for_byte() {
    let rig = Rig::new();
    let (_, m1) = install_1(&rig);
    let (r2, m2) = rig.served(PD2R_2);
    let root = product_root(&rig, &m2);
    let filter = edit_loot_filter(&root);
    let before = tree(&root);
    let crash = FailAt {
        boundary: Boundary::StagedMovedToLive,
        point: FaultPoint::AfterEntry(0),
    };

    assert!(ops::run(&rig.env(&crash), &rig.request(Update, &r2, &m2, Some(&m1))).is_err());
    assert_ne!(tree(&root), before, "the crash left a partial activation");
    let pending = code(rig.run(&rig.request(Update, &r2, &m2, Some(&m1))));
    assert_eq!(pending, ErrorCode::RecoveryRequired);

    let recovered = ops::recover(&rig.env(&NoFaults), rig.game.d2r(), &m2).unwrap();
    assert_eq!(recovered, Recovery::RolledBack);
    assert_eq!(tree(&root), before);
    assert_eq!(fs::read(root.join(LOOT_FILTER)).unwrap(), filter);
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_1));
    assert_no_scratch(&root);
}

#[test]
fn crash_after_live_verification_rolls_forward_and_commits_the_record() {
    let rig = Rig::new();
    let (_, m1) = install_1(&rig);
    let (r2, m2) = rig.served(PD2R_2);
    let root = product_root(&rig, &m2);
    let filter = edit_loot_filter(&root);
    let crash = FailAt {
        boundary: Boundary::InstalledRecordCommitted,
        point: FaultPoint::BeforeStep,
    };

    assert!(ops::run(&rig.env(&crash), &rig.request(Update, &r2, &m2, Some(&m1))).is_err());
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_1));

    let recovered = ops::recover(&rig.env(&NoFaults), rig.game.d2r(), &m2).unwrap();
    assert_eq!(recovered, Recovery::RolledForward);
    assert_eq!(
        tree(&root),
        with_file(release_tree(&r2), LOOT_FILTER, &filter)
    );
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_2));
    assert_no_scratch(&root);
}

#[test]
fn requests_that_do_not_fit_the_installed_record_are_refused() {
    let rig = Rig::new();
    let (r1, m1) = install_1(&rig);
    let (r2, m2) = rig.served(PD2R_2);
    let root = product_root(&rig, &m1);
    let before = tree(&root);
    for req in [
        rig.request(Install, &r2, &m2, Some(&m1)),
        rig.request(Update, &r2, &m2, None),
        rig.request(Update, &r1, &m1, Some(&m1)),
        rig.request(Repair, &r2, &m2, Some(&m1)),
        rig.request(Uninstall, &r2, &m2, Some(&m2)),
    ] {
        assert_eq!(
            code(rig.run(&req)),
            ErrorCode::InvalidRequest,
            "{:?}",
            req.kind
        );
    }
    assert_eq!(tree(&root), before);
}
