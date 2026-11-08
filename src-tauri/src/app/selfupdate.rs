//! Launcher self-update (#88, slice 5): the signed Tauri updater behind
//! `SelfUpdateApi` (`docs/specs/launcher-ui.md` → Launcher self-update, ADR 0003).
//!
//! Trust comes from `tauri.conf.json` only: `plugins.updater.pubkey` is the key
//! posted on #86 and `plugins.updater.endpoints` its `latest.json`. Nothing here
//! takes a key, endpoint, URL or path from the frontend, the product feed or a
//! state file; [`SelfUpdater::check`] uses the plugin's configured updater as is.
//!
//! Every downloaded installer is verified against that key before it is kept
//! ([`tauri_plugin_updater::Update::download`]), and again right before it runs
//! ([`tauri_plugin_updater::Update::verify`]). A failure deletes it and ends in
//! [`SelfUpdateStatus::SignatureRejected`]; nothing is installed.
//!
//! Downloads live in `<app local data>\launcher-updates`, never under D2R.
//! Restart to update is refused while a product operation or recovery runs, or a
//! game runs under the selected D2R folder (the held phases), and once it starts it
//! sets Quit's sticky flag so no operation can begin while the installer takes over.
//!
//! Quit (window and tray, [`begin_quit`]) also installs a ready update, without
//! relaunching. Quit is itself refused while an operation or recovery runs, so it
//! never interrupts an install transaction; a running game holds the install (Quit
//! still exits, and the update installs on a later Quit or Restart to update).
//!
//! The webview drives checks: with `VITE_SELF_UPDATE` off (release builds until
//! the #89 gate), nothing calls these commands and the launcher never checks.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use super::operations::{locked, Launcher, Refusal};
use crate::contract::{ErrorCode, LauncherError, Versioned};

/// Event carrying `Versioned<SelfUpdateStatus>` on every phase change.
pub const SELF_UPDATE_EVENT: &str = "self-update-phase";

/// The folder under the launcher's app local data that holds downloaded updates.
pub const UPDATE_DIR: &str = "launcher-updates";

/// Written right before the installer runs: `{ "from": ..., "to": ... }`.
const INSTALLING_MARKER: &str = "installing.json";

/// What's new opens this page plus a version. Fixed here; the frontend sends nothing.
pub const RELEASE_PAGE: &str =
    "https://github.com/synpoox/resurrected-launcher/releases/tag/launcher-v";

/// Where the launcher's own update is. Mirrors `SelfUpdateStatus` in `src/selfupdate/api.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum SelfUpdateStatus {
    UpToDate {
        current: String,
    },
    Available {
        current: String,
        next: String,
    },
    Downloading {
        current: String,
        next: String,
        percent: u8,
    },
    /// Verified and kept in app data; Restart to update may still be held.
    Ready {
        current: String,
        next: String,
    },
    Restarting {
        current: String,
        next: String,
    },
    /// After the restart that installed it: `current` is the new version.
    Updated {
        current: String,
    },
    DownloadFailed {
        current: String,
        next: String,
    },
    /// The download failed its signature check; it was deleted and nothing installed.
    SignatureRejected {
        current: String,
        next: String,
    },
}

impl SelfUpdateStatus {
    /// The phases from which the update the last check found may be downloaded.
    /// `UpToDate`/`Updated` with a found update is the moment between an automatic
    /// check and the download it starts.
    fn downloadable(&self) -> bool {
        use SelfUpdateStatus::*;
        matches!(
            self,
            UpToDate { .. }
                | Updated { .. }
                | Available { .. }
                | DownloadFailed { .. }
                | SignatureRejected { .. }
        )
    }

    fn next(&self) -> Option<&str> {
        use SelfUpdateStatus::*;
        match self {
            UpToDate { .. } | Updated { .. } => None,
            Available { next, .. }
            | Downloading { next, .. }
            | Ready { next, .. }
            | Restarting { next, .. }
            | DownloadFailed { next, .. }
            | SignatureRejected { next, .. } => Some(next),
        }
    }
}

/// The two things the updater does outside the process. Tests record them.
pub trait UpdateHost: Send + Sync {
    /// Runs the verified installer; `restart` relaunches the launcher afterwards
    /// (Restart to update), else not (Quit). The real one exits on success.
    fn install(
        &self,
        update: &Update,
        bytes: &[u8],
        restart: bool,
    ) -> Result<(), tauri_plugin_updater::Error>;
    /// Opens a [`RELEASE_PAGE`] address in the default browser.
    fn open_page(&self, url: &str) -> io::Result<()>;
}

/// The real host: the plugin's NSIS install (per user, no elevation; it re-verifies
/// the bytes, starts the installer and exits) and Explorer for the release page.
pub struct SystemUpdateHost;

impl UpdateHost for SystemUpdateHost {
    fn install(
        &self,
        update: &Update,
        bytes: &[u8],
        restart: bool,
    ) -> Result<(), tauri_plugin_updater::Error> {
        // Without `/R`, the NSIS installer does not start the launcher afterwards.
        update.clone().restart_after_install(restart).install(bytes)
    }

    fn open_page(&self, url: &str) -> io::Result<()> {
        let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        // explorer.exe hands an https URL to the default browser; no shell parses it.
        std::process::Command::new(Path::new(&windows).join("explorer.exe"))
            .arg(url)
            .spawn()
            .map(drop)
    }
}

/// True for the plugin errors that mean the bytes, the signature or the key do not
/// match: a bad or missing signature, a tampered installer or a wrong key.
fn is_signature_error(e: &tauri_plugin_updater::Error) -> bool {
    use tauri_plugin_updater::Error::*;
    matches!(
        e,
        Minisign(_)
            | Base64(_)
            | SignatureUtf8(_)
            | SignedVersionMismatch { .. }
            | MissingSignedVersion
    )
}

fn failed(code: ErrorCode, what: &str, e: impl std::fmt::Display) -> LauncherError {
    LauncherError::new(code, format!("{what}: {e}"))
}

#[derive(Serialize, Deserialize)]
struct Installing {
    from: String,
    to: String,
}

struct Inner {
    status: SelfUpdateStatus,
    /// The update the last check found.
    found: Option<Update>,
}

/// Native self-update state, managed by Tauri as `Arc<SelfUpdater>`.
pub struct SelfUpdater {
    dir: PathBuf,
    current: String,
    host: Box<dyn UpdateHost>,
    inner: Mutex<Inner>,
}

impl SelfUpdater {
    /// `dir` is `<app local data>\launcher-updates`; `current` the running version.
    /// A marker left by the last Restart to update turns into
    /// [`SelfUpdateStatus::Updated`] when this launcher is the version it installed.
    pub fn new(dir: PathBuf, current: String, host: Box<dyn UpdateHost>) -> Self {
        let marker = dir.join(INSTALLING_MARKER);
        let installed = std::fs::read(&marker)
            .ok()
            .and_then(|b| serde_json::from_slice::<Installing>(&b).ok())
            .is_some_and(|m| m.to == current && m.from != current);
        let status = if installed {
            // The installer it ran is spent.
            let _ = std::fs::remove_dir_all(&dir);
            SelfUpdateStatus::Updated {
                current: current.clone(),
            }
        } else {
            let _ = std::fs::remove_file(&marker);
            SelfUpdateStatus::UpToDate {
                current: current.clone(),
            }
        };
        Self {
            dir,
            current,
            host,
            inner: Mutex::new(Inner {
                status,
                found: None,
            }),
        }
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        // Every write replaces whole fields, so a poisoned lock is still consistent.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> SelfUpdateStatus {
        self.inner().status.clone()
    }

    fn set<R: Runtime>(&self, app: &AppHandle<R>, status: SelfUpdateStatus) {
        self.inner().status = status.clone();
        let _ = app.emit(SELF_UPDATE_EVENT, Versioned::new(status));
    }

    /// Where the verified installer for `version` is kept.
    pub fn download_path(&self, version: &str) -> PathBuf {
        // A semver string from the plugin; anything else never names a file.
        let safe = version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
        let name = if safe { version } else { "unknown" };
        self.dir.join(format!("launcher-{name}-setup.exe"))
    }

    /// Deletes every kept installer except `keep`'s.
    fn prune(&self, keep: Option<&Path>) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for path in entries.flatten().map(|e| e.path()) {
            if Some(path.as_path()) != keep && path.file_name() != Some(INSTALLING_MARKER.as_ref())
            {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    /// Checks the configured endpoint. Returns true when an update was found and
    /// `auto` asks to download it now (the caller starts [`Self::download`]).
    /// Does nothing while a download, a ready update or a restart is in hand.
    pub async fn check<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        auto: bool,
    ) -> Result<bool, LauncherError> {
        if matches!(
            self.status(),
            SelfUpdateStatus::Downloading { .. }
                | SelfUpdateStatus::Ready { .. }
                | SelfUpdateStatus::Restarting { .. }
        ) {
            return Ok(false);
        }
        let updater = app
            .updater()
            .map_err(|e| failed(ErrorCode::Internal, "starting the updater", e))?;
        let found = updater
            .check()
            .await
            .map_err(|e| failed(ErrorCode::DownloadFailed, "checking for updates", e))?;
        let Some(update) = found else {
            self.prune(None);
            let mut inner = self.inner();
            inner.found = None;
            // Updated stays until the next update appears.
            if !matches!(inner.status, SelfUpdateStatus::Updated { .. }) {
                drop(inner);
                self.set(
                    app,
                    SelfUpdateStatus::UpToDate {
                        current: self.current.clone(),
                    },
                );
            }
            return Ok(false);
        };
        let next = update.version.clone();
        self.prune(Some(&self.download_path(&next)));
        let previous = {
            let mut inner = self.inner();
            inner.found = Some(update);
            inner.status.clone()
        };
        // A failed or rejected attempt at this same release keeps its notice.
        let same_failure = matches!(
            previous,
            SelfUpdateStatus::DownloadFailed { .. } | SelfUpdateStatus::SignatureRejected { .. }
        ) && previous.next() == Some(next.as_str());
        if auto && !same_failure {
            return Ok(true);
        }
        if !same_failure {
            self.set(
                app,
                SelfUpdateStatus::Available {
                    current: self.current.clone(),
                    next,
                },
            );
        }
        Ok(false)
    }

    /// Downloads the update the last check found and verifies its signature. A
    /// verified installer already kept for that version is checked again and reused.
    pub async fn download<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), LauncherError> {
        let (current, update) = (self.current.clone(), {
            // Checked and claimed under one lock, so two calls never download at once.
            let mut inner = self.inner();
            let update = match (&inner.status, &inner.found) {
                (status, Some(update)) if status.downloadable() => update.clone(),
                (status, _) => {
                    return Err(LauncherError::new(
                        ErrorCode::InvalidRequest,
                        format!("nothing to download ({status:?})"),
                    ))
                }
            };
            inner.status = SelfUpdateStatus::Downloading {
                current: self.current.clone(),
                next: update.version.clone(),
                percent: 0,
            };
            update
        });
        let next = update.version.clone();
        let path = self.download_path(&next);
        self.set(
            app,
            SelfUpdateStatus::Downloading {
                current: current.clone(),
                next: next.clone(),
                percent: 0,
            },
        );
        if std::fs::read(&path).is_ok_and(|kept| update.verify(&kept).is_ok()) {
            self.set(app, SelfUpdateStatus::Ready { current, next });
            return Ok(());
        }
        let _ = std::fs::remove_file(&path);
        let mut done = 0u64;
        let mut shown = 0u8;
        let result = update
            .download(
                |chunk, total| {
                    done += chunk as u64;
                    let Some(total) = total.filter(|t| *t > 0) else {
                        return;
                    };
                    // Capped below 100 until the signature is checked.
                    let percent = (done.saturating_mul(100) / total).min(99) as u8;
                    if percent > shown {
                        shown = percent;
                        self.set(
                            app,
                            SelfUpdateStatus::Downloading {
                                current: current.clone(),
                                next: next.clone(),
                                percent,
                            },
                        );
                    }
                },
                || {},
            )
            .await;
        match result {
            Ok(bytes) => match keep(&path, &bytes) {
                Ok(()) => {
                    self.set(app, SelfUpdateStatus::Ready { current, next });
                    Ok(())
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&path);
                    self.set(app, SelfUpdateStatus::DownloadFailed { current, next });
                    Err(failed(
                        ErrorCode::AccessDenied,
                        "keeping the launcher update",
                        e,
                    ))
                }
            },
            Err(e) if is_signature_error(&e) => {
                self.set(app, SelfUpdateStatus::SignatureRejected { current, next });
                Err(failed(
                    ErrorCode::VerificationFailed,
                    "launcher update signature",
                    e,
                ))
            }
            Err(e) => {
                self.set(app, SelfUpdateStatus::DownloadFailed { current, next });
                Err(failed(
                    ErrorCode::DownloadFailed,
                    "downloading the launcher update",
                    e,
                ))
            }
        }
    }

    /// Whether [`Self::download`] would start now.
    fn can_download(&self) -> bool {
        let inner = self.inner();
        inner.found.is_some() && inner.status.downloadable()
    }

    /// The update in hand when it is ready: `(update, current, next)`.
    fn ready(&self) -> Result<(Update, String, String), LauncherError> {
        let inner = self.inner();
        match (&inner.status, &inner.found) {
            (SelfUpdateStatus::Ready { current, next }, Some(update)) => {
                Ok((update.clone(), current.clone(), next.clone()))
            }
            (status, _) => Err(LauncherError::new(
                ErrorCode::InvalidRequest,
                format!("no launcher update is ready ({status:?})"),
            )),
        }
    }

    /// Installs the ready update and restarts. Refused in the held phases. The kept
    /// installer is verified again first; a mismatch deletes it and installs nothing.
    pub fn restart_to_update<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        launcher: &Launcher,
    ) -> Result<(), LauncherError> {
        let ready = self.ready()?;
        launcher.begin_self_update_install()?;
        self.install(app, ready, true)
            .inspect_err(|_| launcher.end_self_update_install())
    }

    /// Quit's install: once [`Launcher::begin_quit`] has passed (no operation or
    /// recovery runs, and none can start), installs a ready update without
    /// relaunching. Held while a game runs under the selected D2R folder, or when
    /// that can't be told. Returns true when the installer was started (the real
    /// host does not return then); false when nothing is ready or the install is held.
    pub fn install_on_quit<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        launcher: &Launcher,
    ) -> Result<bool, LauncherError> {
        let Ok(ready) = self.ready() else {
            return Ok(false);
        };
        if launcher.game_holds_self_update() {
            return Ok(false);
        }
        self.install(app, ready, false).map(|()| true)
    }

    /// Verifies the kept installer again, writes the marker and runs it. A
    /// mismatch deletes it and installs nothing.
    fn install<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        (update, current, next): (Update, String, String),
        restart: bool,
    ) -> Result<(), LauncherError> {
        let path = self.download_path(&next);
        let verified = std::fs::read(&path)
            .map_err(|e| failed(ErrorCode::VerificationFailed, "reading the update", e))
            .and_then(|bytes| match update.verify(&bytes) {
                Ok(()) => Ok(bytes),
                Err(e) => Err(failed(
                    ErrorCode::VerificationFailed,
                    "launcher update signature",
                    e,
                )),
            });
        let bytes = match verified {
            Ok(bytes) => bytes,
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                self.set(app, SelfUpdateStatus::SignatureRejected { current, next });
                return Err(e);
            }
        };
        let marker = Installing {
            from: current.clone(),
            to: next.clone(),
        };
        let written = serde_json::to_vec(&marker)
            .map_err(io::Error::other)
            .and_then(|b| keep(&self.dir.join(INSTALLING_MARKER), &b));
        if let Err(e) = written {
            return Err(failed(
                ErrorCode::AccessDenied,
                "preparing the launcher update",
                e,
            ));
        }
        self.set(
            app,
            SelfUpdateStatus::Restarting {
                current: current.clone(),
                next: next.clone(),
            },
        );
        // The real host does not return on success: the installer takes over.
        self.host.install(&update, &bytes, restart).map_err(|e| {
            let _ = std::fs::remove_file(self.dir.join(INSTALLING_MARKER));
            let signature = is_signature_error(&e);
            if signature {
                let _ = std::fs::remove_file(&path);
                self.set(
                    app,
                    SelfUpdateStatus::SignatureRejected {
                        current: current.clone(),
                        next: next.clone(),
                    },
                );
            } else {
                self.set(app, SelfUpdateStatus::Ready { current, next });
            }
            let code = if signature {
                ErrorCode::VerificationFailed
            } else {
                ErrorCode::Internal
            };
            failed(code, "installing the launcher update", e)
        })
    }

    /// Opens the release page of the update in hand, or of this launcher.
    pub fn open_whats_new(&self) -> Result<(), LauncherError> {
        let status = self.status();
        let version = status.next().unwrap_or(&self.current);
        self.host
            .open_page(&format!("{RELEASE_PAGE}{version}"))
            .map_err(|e| failed(ErrorCode::AccessDenied, "opening What's new", e))
    }
}

/// Writes `bytes` to `path` through a temp file, so a crash never leaves a partial
/// installer under the final name.
fn keep(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

impl Launcher {
    /// Restart to update's gate. Refused while an operation or recovery runs
    /// (`OperationInProgress`) or a game runs under the selected D2R folder
    /// (`GameRunning`). Otherwise sets Quit's sticky flag under the operation lock,
    /// so no operation, recovery or Play starts while the installer takes over.
    pub fn begin_self_update_install(&self) -> Result<(), LauncherError> {
        let d2r = locked(&self.state)?.preferences().d2r_location.clone();
        let mut running = self.running();
        running.refuse_any()?;
        if let Some(d2r) = d2r {
            self.refuse_if_game_running(&d2r)?;
        }
        running.quitting = true;
        Ok(())
    }

    /// Undoes [`Self::begin_self_update_install`] when the install did not start.
    pub fn end_self_update_install(&self) {
        self.running().quitting = false;
    }

    /// Quit's hold on a ready update: a game runs under the selected D2R folder,
    /// or the process list or state can't be read (fail closed).
    fn game_holds_self_update(&self) -> bool {
        let Ok(state) = locked(&self.state) else {
            return true;
        };
        let d2r = state.preferences().d2r_location.clone();
        drop(state);
        d2r.is_some_and(|d2r| self.refuse_if_game_running(&d2r).is_err())
    }

    fn auto_download(&self) -> Result<bool, LauncherError> {
        Ok(locked(&self.state)?
            .preferences()
            .download_launcher_updates_automatically)
    }
}

/// Quit's gate for the window and the tray: [`Launcher::begin_quit`] (refused
/// while an operation or recovery runs), then [`SelfUpdater::install_on_quit`].
/// The caller exits on `Ok`. A failed or held install never stops the Quit: the
/// update stays kept, and the current launcher keeps working next time.
pub fn begin_quit<R: Runtime>(app: &AppHandle<R>) -> Result<(), Refusal> {
    let launcher = app.state::<Arc<Launcher>>();
    launcher.begin_quit()?;
    if let Some(updater) = app.try_state::<Arc<SelfUpdater>>() {
        let _ = updater.install_on_quit(app, &launcher);
    }
    Ok(())
}

fn spawn_download<R: Runtime>(app: AppHandle<R>, updater: Arc<SelfUpdater>) {
    tauri::async_runtime::spawn(async move {
        // The outcome arrives as a phase event.
        let _ = updater.download(&app).await;
    });
}

/// `get_launcher_update_status`: the current phase.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_launcher_update_status(
    updater: State<'_, Arc<SelfUpdater>>,
) -> Result<Versioned<SelfUpdateStatus>, LauncherError> {
    Ok(Versioned::new(updater.status()))
}

/// `check_for_launcher_update`: checks now. With automatic downloads on, a found
/// update starts downloading in the background; the result arrives as phase events.
#[tauri::command(rename_all = "snake_case")]
pub async fn check_for_launcher_update<R: Runtime>(
    app: AppHandle<R>,
    updater: State<'_, Arc<SelfUpdater>>,
    launcher: State<'_, Arc<Launcher>>,
) -> Result<(), LauncherError> {
    let auto = launcher.auto_download()?;
    if updater.check(&app, auto).await? {
        spawn_download(app, Arc::clone(&updater));
    }
    Ok(())
}

/// `download_launcher_update`: starts (or retries) downloading the found update.
#[tauri::command(rename_all = "snake_case")]
pub async fn download_launcher_update<R: Runtime>(
    app: AppHandle<R>,
    updater: State<'_, Arc<SelfUpdater>>,
) -> Result<(), LauncherError> {
    // The same guard `download` claims under its lock; this one answers the caller.
    if !updater.can_download() {
        return Err(LauncherError::new(
            ErrorCode::InvalidRequest,
            "nothing to download",
        ));
    }
    spawn_download(app, Arc::clone(&updater));
    Ok(())
}

/// `restart_to_update_launcher`: installs the ready update and restarts, unless held.
#[tauri::command(rename_all = "snake_case")]
pub async fn restart_to_update_launcher<R: Runtime>(
    app: AppHandle<R>,
    updater: State<'_, Arc<SelfUpdater>>,
    launcher: State<'_, Arc<Launcher>>,
) -> Result<(), LauncherError> {
    updater.restart_to_update(&app, &launcher)
}

/// `open_launcher_whats_new`: the launcher release page, built natively.
#[tauri::command(rename_all = "snake_case")]
pub async fn open_launcher_whats_new(
    updater: State<'_, Arc<SelfUpdater>>,
) -> Result<(), LauncherError> {
    updater.open_whats_new()
}

/// `get_launcher_auto_download`: Download launcher updates automatically.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_launcher_auto_download(
    launcher: State<'_, Arc<Launcher>>,
) -> Result<Versioned<bool>, LauncherError> {
    launcher.auto_download().map(Versioned::new)
}

/// `set_launcher_auto_download`: stored in Preferences.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_launcher_auto_download(
    launcher: State<'_, Arc<Launcher>>,
    enabled: bool,
) -> Result<(), LauncherError> {
    let mut state = locked(&launcher.state)?;
    let mut preferences = state.preferences().clone();
    preferences.download_launcher_updates_automatically = enabled;
    state.set_preferences(preferences)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes_like_the_frontend_union() {
        let s = SelfUpdateStatus::Downloading {
            current: "1.0.0".into(),
            next: "1.1.0".into(),
            percent: 38,
        };
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::json!({ "phase": "downloading", "current": "1.0.0", "next": "1.1.0", "percent": 38 })
        );
        let r = SelfUpdateStatus::SignatureRejected {
            current: "1.0.0".into(),
            next: "1.1.0".into(),
        };
        assert_eq!(
            serde_json::to_value(&r).unwrap()["phase"],
            "signature_rejected"
        );
    }

    #[test]
    fn download_path_stays_in_the_update_dir() {
        struct NoHost;
        impl UpdateHost for NoHost {
            fn install(
                &self,
                _: &Update,
                _: &[u8],
                _: bool,
            ) -> Result<(), tauri_plugin_updater::Error> {
                unreachable!()
            }
            fn open_page(&self, _: &str) -> io::Result<()> {
                unreachable!()
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let updater = SelfUpdater::new(dir.path().into(), "1.0.0".into(), Box::new(NoHost));
        for version in ["1.1.0", "..\\..\\evil", "1.1.0/../../x", "C:\\x"] {
            let path = updater.download_path(version);
            assert_eq!(path.parent(), Some(dir.path()), "{version}");
        }
        assert!(updater
            .download_path("1.1.0-beta.1")
            .ends_with("launcher-1.1.0-beta.1-setup.exe"));
    }
}
