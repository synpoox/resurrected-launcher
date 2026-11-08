//! Slice 4 Settings and troubleshooting commands (#74): Patch Notes, folder
//! shortcuts (#16, ADR 0004), diagnostics export and Start with Windows. Every
//! path is built natively; the frontend sends product IDs and fixed choices only.
//! Nothing here sends anything over the network or asks for elevation.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use super::operations::{invalid, locked, Launcher, Release};
use super::winapi;
use crate::catalog::ReleaseStatus;
use crate::contract::{ErrorCode, LauncherError, ProductId, Versioned, WriteGuard};
use crate::game::detect;
use crate::install::plan::product_root;
use crate::state::{DiagnosticsContext, DiagnosticsReport, Redactor, NEVER_INCLUDED};

/// The default file name the save dialog offers.
const DIAGNOSTICS_FILE: &str = "resurrected-launcher-diagnostics.txt";

/// The last previewed and the last saved diagnostics report.
#[derive(Debug, Default)]
pub(crate) struct Exported {
    /// `(include_log, text)`: saving with the same choice writes exactly this.
    preview: Option<(bool, String)>,
    saved: Option<PathBuf>,
}

fn access_denied(what: &str, e: impl std::fmt::Display) -> LauncherError {
    LauncherError::new(ErrorCode::AccessDenied, format!("{what}: {e}"))
}

/// One release in Patch Notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseNotes {
    pub release_id: String,
    pub display_version: String,
    /// `YYYY-MM-DD`.
    pub release_date: String,
    /// `alpha`, `beta` or `release`.
    pub status: String,
    /// shortcut: always set today, because [`super::Releases`] only yields cached
    /// manifests and a feed pointer has no version or date to list a newer release
    /// without one; `None` is reserved for listing such a release once it does.
    pub notes: Option<String>,
}

impl ReleaseNotes {
    fn of(r: &Release) -> Self {
        let m = &r.manifest;
        Self {
            release_id: m.release_id.clone(),
            display_version: m.display_version.clone(),
            release_date: m.release_date.clone(),
            status: match m.status {
                ReleaseStatus::Alpha => "alpha",
                ReleaseStatus::Beta => "beta",
                ReleaseStatus::Release => "release",
            }
            .to_owned(),
            notes: Some(m.release_notes.clone()),
        }
    }
}

/// A folder the Folders and plugins tab (and the Manage menu) opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Folder {
    /// `<Saved Games>\Diablo II Resurrected\mods\<install folder>`.
    Saves,
    /// The product root, `<D2R>\mods\<install folder>`.
    Settings,
    /// `<product root>\d2rloader\plugins`: loads only with that product.
    ProductPlugins,
    /// `<D2R>\d2rloader\plugins`: loads with every product.
    GlobalPlugins,
}

fn not_installed(why: String) -> LauncherError {
    LauncherError::new(ErrorCode::ProductNotInstalled, why)
}

impl Launcher {
    /// The D2R root and the folder `folder` names, built from native state only.
    fn folder(
        &self,
        product: Option<&ProductId>,
        folder: Folder,
    ) -> Result<(PathBuf, PathBuf), LauncherError> {
        let mut state = locked(&self.state)?;
        let d2r = self.d2r_root(&state)?;
        let release = match (folder, product) {
            (Folder::GlobalPlugins, None) => None,
            (Folder::GlobalPlugins, Some(_)) => {
                return Err(invalid("global plugins belong to no product"))
            }
            (_, None) => return Err(invalid("this folder needs a product")),
            (_, Some(p)) => {
                let record = state
                    .installed_release(p)
                    .cloned()
                    .ok_or_else(|| not_installed(format!("{} is not installed", p.0)))?;
                Some(self.releases.installed(&mut state, &record)?)
            }
        };
        drop(state);
        let root = release.as_ref().map(|r| product_root(&d2r, &r.manifest));
        let dir = match (folder, release, root) {
            (Folder::Saves, Some(r), _) => self
                .desktop
                .saved_games()
                .map_err(|e| access_denied("finding Saved Games", e))?
                .join("Diablo II Resurrected")
                .join("mods")
                .join(r.manifest.mod_folder()),
            (Folder::Settings, _, Some(root)) => root,
            (Folder::ProductPlugins, _, Some(root)) => root.join("d2rloader").join("plugins"),
            _ => d2r.join("d2rloader").join("plugins"),
        };
        Ok((d2r, dir))
    }

    /// Creates a missing plugins folder under a validated D2R root, through the
    /// write guard of the folder it lives in (the product root or the D2R root).
    fn create_plugins(&self, d2r: &Path, dir: &Path) -> Result<(), LauncherError> {
        detect::validate(d2r, &*self.game)?;
        // `dir` is `<owner>\d2rloader\plugins`, built by `folder` under `d2r`.
        let owner = dir
            .ancestors()
            .nth(2)
            .ok_or_else(|| invalid("no plugins owner"))?;
        if !owner.is_dir() {
            return Err(not_installed(format!("{} does not exist", owner.display())));
        }
        // No write lands through a link or junction anywhere above it.
        super::game::refuse_linked_root(dir)
            .map_err(|e| access_denied("creating the plugins folder", e.message))?;
        let _guard = WriteGuard::check(owner)
            .map_err(|e| access_denied("creating the plugins folder", e.message))?;
        std::fs::create_dir_all(dir)
            .map_err(|e| access_denied(&format!("creating {}", dir.display()), e))
    }

    fn diagnostics(&self, include_log: bool) -> Result<DiagnosticsReport, LauncherError> {
        let ctx = DiagnosticsContext {
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            os: winapi::os_description(),
            recent_errors: locked(&self.recent_errors)?.clone(),
            start_with_windows: self.start_with_windows().unwrap_or(false),
        };
        let report =
            locked(&self.state)?.diagnostics_report(&ctx, include_log, &Redactor::from_env());
        Ok(report)
    }

    /// The Run value Start with Windows writes: `"<current exe>" --autostart`.
    fn run_command() -> Result<String, LauncherError> {
        let exe = std::env::current_exe().map_err(|e| access_denied("finding the launcher", e))?;
        Ok(format!("\"{}\" --autostart", exe.display()))
    }

    /// On only when the Run value starts this launcher; a stale value counts as off.
    fn start_with_windows(&self) -> Result<bool, LauncherError> {
        let value = self
            .desktop
            .run_value()
            .map_err(|e| access_denied("reading Start with Windows", e))?;
        Ok(value == Some(Self::run_command()?))
    }
}

/// `get_release_notes`: newest first, the recommended release when it is not the
/// installed one, then the installed release (from its cached manifest, so it
/// works offline). Empty when neither is known.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_release_notes(
    launcher: State<'_, Arc<Launcher>>,
    product_id: ProductId,
) -> Result<Versioned<Vec<ReleaseNotes>>, LauncherError> {
    let mut state = locked(&launcher.state)?;
    let installed = match state.installed_release(&product_id).cloned() {
        Some(record) => Some(launcher.releases.installed(&mut state, &record)?),
        None => None,
    };
    let recommended = launcher.releases.recommended(&mut state, &product_id).ok();
    let newer = recommended.filter(|r| {
        installed
            .as_ref()
            .is_none_or(|i| i.manifest.release_id != r.manifest.release_id)
    });
    Ok(Versioned::new(
        newer
            .iter()
            .chain(&installed)
            .map(ReleaseNotes::of)
            .collect(),
    ))
}

/// `open_folder`: opens one of the [`Folder`]s in Explorer. A missing plugins
/// folder is created first; a missing saves or settings folder is
/// `ProductNotInstalled`. `product_id` is `None` for global plugins only.
#[tauri::command(rename_all = "snake_case")]
pub async fn open_folder(
    launcher: State<'_, Arc<Launcher>>,
    product_id: Option<ProductId>,
    folder: Folder,
) -> Result<(), LauncherError> {
    let (d2r, dir) = launcher.folder(product_id.as_ref(), folder)?;
    if !dir.is_dir() {
        match folder {
            Folder::ProductPlugins | Folder::GlobalPlugins => {
                launcher.create_plugins(&d2r, &dir)?
            }
            Folder::Saves | Folder::Settings => {
                return Err(not_installed(format!(
                    "{} does not exist yet",
                    dir.display()
                )))
            }
        }
    }
    launcher
        .desktop
        .open_folder(&dir)
        .map_err(|e| access_denied(&format!("opening {}", dir.display()), e))
}

/// `preview_diagnostics`: the redacted report for the review dialog.
#[tauri::command(rename_all = "snake_case")]
pub async fn preview_diagnostics(
    launcher: State<'_, Arc<Launcher>>,
    include_log: bool,
) -> Result<Versioned<DiagnosticsReport>, LauncherError> {
    let report = launcher.diagnostics(include_log)?;
    locked(&launcher.exported)?.preview = Some((include_log, report.text.clone()));
    Ok(Versioned::new(report))
}

/// `save_diagnostics`: the native save dialog, then the report. With the same
/// `include_log` as the last preview, the file is exactly the previewed text.
/// `false` when the player cancels.
#[tauri::command(rename_all = "snake_case")]
pub async fn save_diagnostics(
    launcher: State<'_, Arc<Launcher>>,
    include_log: bool,
) -> Result<Versioned<bool>, LauncherError> {
    let previewed = locked(&launcher.exported)?
        .preview
        .clone()
        .filter(|(log, _)| *log == include_log);
    let text = match previewed {
        Some((_, text)) => text,
        None => launcher.diagnostics(include_log)?.text,
    };
    let picker = Arc::clone(&launcher);
    let picked = tauri::async_runtime::spawn_blocking(move || {
        picker.desktop.pick_save_file(DIAGNOSTICS_FILE)
    })
    .await
    .map_err(|e| LauncherError::new(ErrorCode::Internal, e.to_string()))?;
    let Some(path) = picked else {
        return Ok(Versioned::new(false));
    };
    DiagnosticsReport {
        text,
        never_included: NEVER_INCLUDED,
    }
    .save_as(&path)?;
    locked(&launcher.exported)?.saved = Some(path);
    Ok(Versioned::new(true))
}

/// `show_diagnostics_in_folder`: Explorer with the last saved report selected.
#[tauri::command(rename_all = "snake_case")]
pub async fn show_diagnostics_in_folder(
    launcher: State<'_, Arc<Launcher>>,
) -> Result<(), LauncherError> {
    let saved = locked(&launcher.exported)?
        .saved
        .clone()
        .ok_or_else(|| invalid("no diagnostics report was saved"))?;
    launcher
        .desktop
        .show_in_folder(&saved)
        .map_err(|e| access_denied(&format!("showing {}", saved.display()), e))
}

/// `get_start_with_windows`: whether the per-user Run value starts this launcher.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_start_with_windows(
    launcher: State<'_, Arc<Launcher>>,
) -> Result<Versioned<bool>, LauncherError> {
    launcher.start_with_windows().map(Versioned::new)
}

/// `set_start_with_windows`: writes or deletes
/// `HKCU\...\Run\<identifier>` = `"<current exe>" --autostart`, the only stored state.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_start_with_windows(
    launcher: State<'_, Arc<Launcher>>,
    enabled: bool,
) -> Result<(), LauncherError> {
    let command = Launcher::run_command()?;
    launcher
        .desktop
        .set_run_value(enabled.then_some(command.as_str()))
        .map_err(|e| access_denied("changing Start with Windows", e))
}
