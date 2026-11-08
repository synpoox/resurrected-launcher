//! The tray (variant A, #27): icon, tooltip and a native menu that follow product
//! state. State is read natively through the same commands the frontend calls, so
//! every tray action passes the same checks and locks.
//!
//! The pure part ([`view`]) is unit tested, [`quit`] in `tests/lifecycle.rs`; [`start`] wires it to a
//! real tray icon and runs actions on one worker thread, never the main thread.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use tauri::async_runtime::block_on;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Listener, Manager, Runtime};

use super::icon::{self, Overlay};
use super::{request, show_main};
use crate::app::{
    self, Launcher, ProductState, Refusal, OUTCOME_EVENT, PROGRESS_EVENT, STATE_EVENT,
};
use crate::contract::{
    CancelRequest, LauncherError, OperationKind, OperationOutcome, OperationRequest, Phase,
    ProductId, ProgressEvent, Readiness, TrayAction, Versioned, UMBRELLA_DISPLAY_NAME,
};

/// One menu entry for a product.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// `id` is `<action>:<product id>`.
    Action {
        id: String,
        label: String,
    },
    Disabled(String),
}

/// What the tray shows for one product.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductView {
    pub overlay: Overlay,
    /// The tooltip line, e.g. "PD2R: Ready · 0.4.2 alpha (latest)".
    pub status: String,
    pub items: Vec<Item>,
}

/// What Quit does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quit {
    /// Idle, update available, error or game running: exit at once (the game keeps running).
    Exit,
    /// An operation runs on this product: show the window and ask first.
    Busy(ProductId),
}

/// Tray Quit, through the same gate as `quit_launcher`: [`Launcher::begin_quit`]
/// refuses while an operation or recovery runs, else sets the sticky quitting
/// flag so nothing starts before the exit. On `Busy` it shows the window and emits
/// `quit_while_busy`; on `Exit` the caller exits (the game keeps running).
pub fn quit<R: Runtime>(app: &AppHandle<R>) -> Quit {
    match app.state::<Arc<Launcher>>().begin_quit() {
        Err(Refusal {
            product: Some(product),
            ..
        }) => {
            request(app, TrayAction::QuitWhileBusy, &product, None);
            Quit::Busy(product)
        }
        // Refused with nothing running: already quitting.
        Ok(()) | Err(_) => Quit::Exit,
    }
}

/// Tray Play, through `play_product`. Success emits `product-state-changed`, the
/// only event on which the window reads state again (otherwise emitted when the
/// game exits); the tray refreshes on it too. A refused Play opens the window with
/// `show_what_happened` and the refusal, as the window would show it.
pub fn play<R: Runtime>(app: &AppHandle<R>, product: ProductId) {
    match block_on(app::play_product(app.clone(), app.state(), product.clone())) {
        Ok(_) => {
            let _ = app.emit(STATE_EVENT, Versioned::new(()));
        }
        Err(e) => request(app, TrayAction::ShowWhatHappened, &product, Some(e)),
    }
}

/// shortcut: the upper-cased product id ("pd2r" → "PD2R"); use a catalog display
/// name once a second product gets a release.
fn short_name(product: &ProductId) -> String {
    product.0.to_uppercase()
}

/// Download %; phases after the download count as a full bar.
fn percent(p: &ProgressEvent) -> u8 {
    match (p.bytes_done, p.bytes_total, p.phase) {
        (Some(done), Some(total), _) if total > 0 => (done.min(total) * 100 / total) as u8,
        (_, _, Phase::Planning | Phase::Downloading) => 0,
        _ => 100,
    }
}

/// The tray view of one product. `progress` is its last progress event and
/// `failed` the kind of its last failed operation, both since it last ended well.
pub fn view(
    state: &ProductState,
    progress: Option<&ProgressEvent>,
    failed: Option<OperationKind>,
) -> ProductView {
    let name = short_name(&state.product_id);
    let action = |act: &str, label: String| Item::Action {
        id: format!("{act}:{}", state.product_id.0),
        label,
    };
    let shown = |overlay, status: String, items| ProductView {
        overlay,
        status: format!("{name}: {status}"),
        items,
    };
    if let Some(kind) = state.running {
        let pct = progress.map_or(0, percent);
        if state.cancelling {
            return shown(
                Overlay::Progress(pct),
                "Stopping safely…".into(),
                vec![Item::Disabled(format!("Stopping {name} safely…"))],
            );
        }
        let target = match (kind, &state.recommended) {
            (OperationKind::Install | OperationKind::Update, Some(r)) => {
                format!(" {}", r.display_version)
            }
            _ => String::new(),
        };
        let downloads = matches!(
            kind,
            OperationKind::Install | OperationKind::Update | OperationKind::CleanInstall
        );
        let downloading = match progress.map(|p| p.phase) {
            Some(Phase::Downloading) => true,
            None | Some(Phase::Planning) => downloads,
            Some(_) => false,
        };
        let verb = match kind {
            OperationKind::Repair => "Repairing",
            OperationKind::Uninstall => "Uninstalling",
            OperationKind::Adopt => "Adopting",
            _ => "Applying",
        };
        return if downloading {
            shown(
                Overlay::Progress(pct),
                format!("Downloading{target} · {pct}%"),
                vec![
                    Item::Disabled(format!("Downloading {name} · {pct}%")),
                    action("cancel", "Cancel download".into()),
                ],
            )
        } else {
            shown(
                Overlay::Progress(pct),
                format!("{verb}{target}"),
                vec![
                    Item::Disabled(format!("{verb} {name}…")),
                    action("cancel", "Cancel safely".into()),
                ],
            )
        };
    }
    let error = |status: &str| {
        shown(
            Overlay::Error,
            status.into(),
            vec![
                action("retry", "Try again".into()),
                action("show", "Show what happened".into()),
            ],
        )
    };
    // A running game is fresher than any earlier failure.
    let game_running = matches!(state.readiness, Readiness::Running { .. });
    if failed.is_some() && !game_running {
        return error("Something went wrong");
    }
    if state.startup_failed.is_some() && !game_running {
        return error("The game didn't start");
    }
    let installed = state
        .installed
        .as_ref()
        .map_or("", |r| r.display_version.as_str());
    match &state.readiness {
        Readiness::Running { .. } => shown(
            Overlay::Running,
            "Running".into(),
            vec![Item::Disabled(format!("{name} is running"))],
        ),
        Readiness::Ready => {
            let latest = match (&state.installed, &state.recommended) {
                (Some(i), Some(r)) if i.release_id == r.release_id => " (latest)",
                _ => "",
            };
            shown(
                Overlay::Idle,
                format!("Ready · {installed}{latest}"),
                vec![action("play", format!("Play {name}"))],
            )
        }
        Readiness::UpdateAvailable => {
            let newer = state
                .recommended
                .as_ref()
                .map_or("", |r| r.display_version.as_str());
            shown(
                Overlay::Update,
                format!("Update available · {newer}"),
                vec![
                    action("update", format!("Update {name}")),
                    action("play", format!("Play {installed}")),
                ],
            )
        }
        Readiness::NotInstalled => shown(Overlay::Idle, "Not installed".into(), vec![]),
        // Also every product until startup recovery finished.
        Readiness::RecoveryRequired => shown(Overlay::Idle, "Checking files…".into(), vec![]),
        Readiness::D2rMissing => error("D2R not found"),
        Readiness::BuildUnsupported { .. } => error("D2R version not supported"),
        Readiness::LoaderMissing | Readiness::LoaderUnsupported { .. } => {
            error("D2RLoader not ready")
        }
    }
}

/// What "Try again" does for a product.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Retry {
    Start(OperationKind),
    Play,
    CheckAgain,
}

/// "Try again": restarts a failed Install, Update or Repair (never Uninstall or
/// Clean install without the window), retries Play after the game didn't start,
/// else checks again.
fn retry(failed: Option<OperationKind>, state: Option<&ProductState>) -> Retry {
    match failed {
        Some(kind @ (OperationKind::Install | OperationKind::Update | OperationKind::Repair)) => {
            Retry::Start(kind)
        }
        None if state.is_some_and(|s| s.startup_failed.is_some()) => Retry::Play,
        _ => Retry::CheckAgain,
    }
}

/// Drops failures that a newer operation or a running game has superseded.
fn clear_stale(
    failed: &mut HashMap<ProductId, (OperationKind, Option<LauncherError>)>,
    states: &[ProductState],
) {
    for s in states {
        if s.running.is_some() || matches!(s.readiness, Readiness::Running { .. }) {
            failed.remove(&s.product_id);
        }
    }
}

fn tooltip(views: &[ProductView]) -> String {
    std::iter::once(UMBRELLA_DISPLAY_NAME)
        .chain(views.iter().map(|v| v.status.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Products without a release don't appear in the tray.
fn has_release(state: &ProductState) -> bool {
    state.installed.is_some() || state.recommended.is_some()
}

enum Msg {
    Refresh,
    Progress(ProgressEvent),
    Outcome(OperationOutcome),
    Click(String),
}

/// What the tray knows beyond `get_product_state`. `failed` keeps the kind of the
/// last failed operation and, when known, its error for Show what happened.
#[derive(Default)]
pub struct Model {
    states: Vec<ProductState>,
    progress: HashMap<ProductId, ProgressEvent>,
    failed: HashMap<ProductId, (OperationKind, Option<LauncherError>)>,
}

/// Creates the tray icon and its worker thread.
pub(super) fn start<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let (tx, rx) = channel();
    let click = tx.clone();
    let tray = TrayIconBuilder::with_id("main")
        .icon(icon::draw(Overlay::Idle))
        .tooltip(UMBRELLA_DISPLAY_NAME)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .on_menu_event(move |_, event| {
            let _ = click.send(Msg::Click(event.id.0));
        })
        .build(app)?;
    let forward = |event: &str, wrap: fn(&str) -> Option<Msg>| {
        let tx = tx.clone();
        app.listen_any(event, move |e| {
            if let Some(msg) = wrap(e.payload()) {
                let _ = tx.send(msg);
            }
        });
    };
    forward(STATE_EVENT, |_| Some(Msg::Refresh));
    forward(PROGRESS_EVENT, |p| payload(p).map(Msg::Progress));
    forward(OUTCOME_EVENT, |p| payload(p).map(Msg::Outcome));
    let _ = tx.send(Msg::Refresh);
    let app = app.clone();
    std::thread::spawn(move || run_loop(&app, &tray, rx));
    Ok(())
}

fn payload<T: DeserializeOwned>(json: &str) -> Option<T> {
    serde_json::from_str::<Versioned<T>>(json)
        .ok()?
        .into_current()
        .ok()
}

fn run_loop<R: Runtime>(app: &AppHandle<R>, tray: &TrayIcon<R>, rx: Receiver<Msg>) {
    let mut model = Model::default();
    let mut shown = None;
    for msg in rx {
        match msg {
            Msg::Refresh => model.refresh(app),
            Msg::Progress(p) => {
                model.failed.remove(&p.product_id);
                let stale = model
                    .states
                    .iter()
                    .any(|s| s.product_id == p.product_id && s.running.is_none());
                model.progress.insert(p.product_id.clone(), p);
                if stale {
                    model.refresh(app);
                }
            }
            Msg::Outcome(outcome) => {
                let (product, failed) = match outcome {
                    OperationOutcome::Failed {
                        product_id,
                        kind,
                        error,
                        ..
                    } => (product_id, Some((kind, Some(error)))),
                    OperationOutcome::Succeeded { product_id, .. }
                    | OperationOutcome::Cancelled { product_id, .. } => (product_id, None),
                };
                model.progress.remove(&product);
                match failed {
                    Some(failure) => model.failed.insert(product, failure),
                    None => model.failed.remove(&product),
                };
                model.refresh(app);
            }
            Msg::Click(id) => model.click(app, &id),
        }
        apply(app, tray, model.views(), &mut shown);
    }
}

impl Model {
    fn views(&self) -> Vec<ProductView> {
        self.states
            .iter()
            .map(|s| {
                let id = &s.product_id;
                view(
                    s,
                    self.progress.get(id),
                    self.failed.get(id).map(|(kind, _)| *kind),
                )
            })
            .collect()
    }

    fn refresh<R: Runtime>(&mut self, app: &AppHandle<R>) {
        let ids = app.state::<Arc<Launcher>>().known_products();
        self.states = ids
            .into_iter()
            .filter_map(|id| block_on(app::get_product_state(app.state(), id)).ok())
            .map(|v| v.data)
            .filter(has_release)
            .collect();
        clear_stale(&mut self.failed, &self.states);
    }

    /// Runs the tray menu item `id`, as a click on it would.
    pub fn click<R: Runtime>(&mut self, app: &AppHandle<R>, id: &str) {
        if id == "open" {
            return show_main(app);
        }
        self.refresh(app);
        if id == "quit" {
            if quit(app) == Quit::Exit {
                app.exit(0);
            }
            return;
        }
        let Some((act, product)) = id.split_once(':') else {
            return;
        };
        let product = ProductId(product.to_owned());
        match act {
            "play" => play(app, product),
            "update" => self.start(app, product, OperationKind::Update),
            "cancel" => {
                let cancel = Versioned::new(CancelRequest {
                    product_id: product,
                });
                let _ = block_on(app::cancel_operation(app.state(), cancel));
            }
            "retry" => match retry(
                self.failed.get(&product).map(|(kind, _)| *kind),
                self.states.iter().find(|s| s.product_id == product),
            ) {
                Retry::Start(kind) => self.start(app, product, kind),
                Retry::Play => play(app, product),
                Retry::CheckAgain => {
                    // "Check again": clear the failure and reassess.
                    self.failed.remove(&product);
                    let _ = block_on(app::refresh_catalog(app.clone(), app.state()));
                    self.refresh(app);
                }
            },
            "show" => {
                let error = self.failed.get(&product).and_then(|(_, e)| e.clone());
                request(app, TrayAction::ShowWhatHappened, &product, error)
            }
            _ => {}
        }
    }

    /// Starts `kind` without the window, except on a save-breaking release: that
    /// opens the window for the acknowledgement (ADR 0001); the tray never sends one.
    fn start<R: Runtime>(&mut self, app: &AppHandle<R>, product: ProductId, kind: OperationKind) {
        let save_breaking = self
            .states
            .iter()
            .find(|s| s.product_id == product)
            .and_then(|s| s.recommended.as_ref())
            .is_some_and(|r| r.save_breaking);
        if save_breaking && matches!(kind, OperationKind::Install | OperationKind::Update) {
            return request(app, TrayAction::SaveBreaking, &product, None);
        }
        let req = OperationRequest {
            product_id: product.clone(),
            kind,
            acknowledge_save_breaking: false,
            consent_clean_install: false,
        };
        let started = block_on(async {
            let plan = app::inspect_operation(app.state(), Versioned::new(req.clone())).await?;
            app::start_operation(
                app.clone(),
                app.state(),
                Versioned::new(req),
                plan.data.token,
            )
            .await
        });
        // A refused start shows as an error; Show what happened opens the window.
        match started {
            Ok(()) => self.failed.remove(&product),
            Err(e) => self.failed.insert(product, (kind, Some(e))),
        };
    }
}

/// What the tray last showed, with its product menu items for in-place updates.
struct Shown<R: Runtime> {
    views: Vec<ProductView>,
    items: Vec<MenuItem<R>>,
}

/// The product items' ids (`None` for disabled ones): what a rebuild changes.
fn shape(views: &[ProductView]) -> Vec<Option<&str>> {
    views
        .iter()
        .flat_map(|v| &v.items)
        .map(|i| match i {
            Item::Action { id, .. } => Some(id.as_str()),
            Item::Disabled(_) => None,
        })
        .collect()
}

fn label(item: &Item) -> &str {
    match item {
        Item::Action { label, .. } | Item::Disabled(label) => label,
    }
}

fn apply<R: Runtime>(
    app: &AppHandle<R>,
    tray: &TrayIcon<R>,
    views: Vec<ProductView>,
    shown: &mut Option<Shown<R>>,
) {
    // Progress arrives up to 10 times a second; change only what is visible.
    if shown.as_ref().is_some_and(|s| s.views == views) {
        return;
    }
    // shortcut: the icon follows the first product (only PD2R has a release);
    // pick the most urgent overlay once a second product ships.
    let overlay = views.first().map_or(Overlay::Idle, |v| v.overlay);
    let _ = tray.set_icon(Some(icon::draw(overlay)));
    let _ = tray.set_tooltip(Some(tooltip(&views)));
    match shown {
        // Same items (e.g. only the % changed): update their text in place, so a
        // menu open during a download is never replaced under the player.
        Some(s) if shape(&s.views) == shape(&views) => {
            for (item, new) in s.items.iter().zip(views.iter().flat_map(|v| &v.items)) {
                let _ = item.set_text(label(new));
            }
            s.views = views;
        }
        _ => {
            *shown = menu(app, &views).ok().map(|(menu, items)| {
                bold_first_item(&menu);
                let _ = tray.set_menu(Some(menu));
                Shown { views, items }
            });
        }
    }
}

/// Open, separator, the product items, separator, Quit (spec order). Also returns
/// the product items.
fn menu<R: Runtime>(
    app: &AppHandle<R>,
    views: &[ProductView],
) -> tauri::Result<(Menu<R>, Vec<MenuItem<R>>)> {
    let menu = Menu::new(app)?;
    let open = format!("Open {UMBRELLA_DISPLAY_NAME}");
    menu.append(&MenuItem::with_id(app, "open", open, true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let mut items = Vec::new();
    for item in views.iter().flat_map(|v| &v.items) {
        let built = match item {
            Item::Action { id, label } => MenuItem::with_id(app, id, label, true, None::<&str>)?,
            Item::Disabled(label) => MenuItem::new(app, label, false, None::<&str>)?,
        };
        menu.append(&built)?;
        items.push(built);
    }
    if !items.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok((menu, items))
}

/// Makes Open the bold default item; muda has no API for it.
#[cfg(windows)]
fn bold_first_item<R: Runtime>(menu: &Menu<R>) {
    use tauri::menu::ContextMenu;
    #[link(name = "user32")]
    extern "system" {
        fn SetMenuDefaultItem(menu: isize, item: u32, by_position: u32) -> i32;
    }
    if let Ok(hmenu) = menu.hpopupmenu() {
        // SAFETY: `hmenu` is the live popup menu muda owns; this only sets a flag on it.
        unsafe { SetMenuDefaultItem(hmenu, 0, 1) };
    }
}

#[cfg(not(windows))]
fn bold_first_item<R: Runtime>(_: &Menu<R>) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{CatalogState, ReleaseSummary};
    use crate::contract::Sha256Digest;
    use crate::state::InstalledRelease;

    fn release(id: &str, save_breaking: bool) -> ReleaseSummary {
        ReleaseSummary {
            release_id: id.into(),
            display_version: format!("{id} alpha"),
            save_breaking,
        }
    }

    fn state(readiness: Readiness, installed: Option<&str>, recommended: &str) -> ProductState {
        ProductState {
            product_id: ProductId("pd2r".into()),
            installed: installed.map(|id| InstalledRelease {
                product_id: ProductId("pd2r".into()),
                release_id: id.into(),
                display_version: format!("{id} alpha"),
                manifest_sha256: Sha256Digest::from_bytes([1; 32]).unwrap(),
                installed_at: 0,
                verified: true,
            }),
            recommended: Some(release(recommended, false)),
            running: None,
            location: None,
            readiness,
            running_game: None,
            loader_guidance: None,
            catalog: CatalogState::Refreshed,
            cancelling: false,
            startup_failed: None,
        }
    }

    fn labels(v: &ProductView) -> Vec<String> {
        v.items
            .iter()
            .map(|i| match i {
                Item::Action { id, label } => format!("{label} [{id}]"),
                Item::Disabled(label) => format!("{label} (disabled)"),
            })
            .collect()
    }

    #[test]
    fn ready_offers_play_and_says_latest() {
        let v = view(&state(Readiness::Ready, Some("0.4.2"), "0.4.2"), None, None);
        assert_eq!(v.overlay, Overlay::Idle);
        assert_eq!(v.status, "PD2R: Ready · 0.4.2 alpha (latest)");
        assert_eq!(labels(&v), ["Play PD2R [play:pd2r]"]);
    }

    #[test]
    fn update_available_offers_update_and_play_installed() {
        let v = view(
            &state(Readiness::UpdateAvailable, Some("0.4.1"), "0.4.2"),
            None,
            None,
        );
        assert_eq!(v.overlay, Overlay::Update);
        assert_eq!(
            labels(&v),
            ["Update PD2R [update:pd2r]", "Play 0.4.1 alpha [play:pd2r]"]
        );
    }

    #[test]
    fn running_game_is_a_disabled_item() {
        let v = view(
            &state(Readiness::Running { since: 1 }, Some("0.4.2"), "0.4.2"),
            None,
            None,
        );
        assert_eq!(v.overlay, Overlay::Running);
        assert_eq!(labels(&v), ["PD2R is running (disabled)"]);
    }

    #[test]
    fn downloading_shows_percent_and_cancel() {
        let mut s = state(Readiness::UpdateAvailable, Some("0.4.1"), "0.4.2");
        s.running = Some(OperationKind::Update);
        let p = ProgressEvent {
            product_id: s.product_id.clone(),
            kind: OperationKind::Update,
            phase: Phase::Downloading,
            boundary: None,
            bytes_done: Some(52),
            bytes_total: Some(100),
            files_done: None,
            files_total: None,
        };
        let v = view(&s, Some(&p), None);
        assert_eq!(v.overlay, Overlay::Progress(52));
        assert_eq!(v.status, "PD2R: Downloading 0.4.2 alpha · 52%");
        assert_eq!(
            labels(&v),
            [
                "Downloading PD2R · 52% (disabled)",
                "Cancel download [cancel:pd2r]"
            ]
        );
    }

    #[test]
    fn failure_offers_try_again_and_show_what_happened() {
        let v = view(
            &state(Readiness::UpdateAvailable, Some("0.4.1"), "0.4.2"),
            None,
            Some(OperationKind::Update),
        );
        assert_eq!(v.overlay, Overlay::Error);
        assert_eq!(
            labels(&v),
            ["Try again [retry:pd2r]", "Show what happened [show:pd2r]"]
        );
    }

    #[test]
    fn a_running_game_outranks_a_stale_failure() {
        let s = state(Readiness::Running { since: 1 }, Some("0.4.1"), "0.4.2");
        let v = view(&s, None, Some(OperationKind::Update));
        assert_eq!(v.overlay, Overlay::Running);
        let mut failed = HashMap::from([(s.product_id.clone(), (OperationKind::Update, None))]);
        clear_stale(&mut failed, &[s]);
        assert!(failed.is_empty());
    }

    #[test]
    fn a_newer_operation_clears_a_stale_failure() {
        let mut s = state(Readiness::UpdateAvailable, Some("0.4.1"), "0.4.2");
        s.running = Some(OperationKind::Repair);
        let mut failed = HashMap::from([(s.product_id.clone(), (OperationKind::Update, None))]);
        clear_stale(&mut failed, std::slice::from_ref(&s));
        assert!(failed.is_empty());
        let idle = state(Readiness::UpdateAvailable, Some("0.4.1"), "0.4.2");
        let mut kept = HashMap::from([(idle.product_id.clone(), (OperationKind::Update, None))]);
        clear_stale(&mut kept, &[idle]);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn try_again_after_the_game_did_not_start_plays_again() {
        let mut s = state(Readiness::Ready, Some("0.4.2"), "0.4.2");
        s.startup_failed = Some(crate::app::StartupFailed { exit_code: Some(1) });
        assert_eq!(retry(None, Some(&s)), Retry::Play);
        assert_eq!(
            retry(Some(OperationKind::Update), Some(&s)),
            Retry::Start(OperationKind::Update)
        );
        assert_eq!(
            retry(Some(OperationKind::Uninstall), Some(&s)),
            Retry::CheckAgain
        );
        let ok = state(Readiness::Ready, Some("0.4.2"), "0.4.2");
        assert_eq!(retry(None, Some(&ok)), Retry::CheckAgain);
    }

    #[test]
    fn tooltip_starts_with_the_umbrella_name() {
        let v = view(&state(Readiness::Ready, Some("0.4.2"), "0.4.2"), None, None);
        assert_eq!(
            tooltip(&[v]),
            "Project Resurrected\nPD2R: Ready · 0.4.2 alpha (latest)"
        );
    }

    #[test]
    fn products_without_a_release_are_left_out() {
        let mut s = state(Readiness::NotInstalled, None, "x");
        s.recommended = None;
        assert!(!has_release(&s));
    }

    fn running(kind: OperationKind, phase: Option<Phase>) -> ProductView {
        let mut s = state(Readiness::Ready, Some("0.4.2"), "0.4.2");
        s.running = Some(kind);
        let p = phase.map(|phase| ProgressEvent {
            product_id: s.product_id.clone(),
            kind,
            phase,
            boundary: None,
            bytes_done: None,
            bytes_total: None,
            files_done: None,
            files_total: None,
        });
        view(&s, p.as_ref(), None)
    }

    #[test]
    fn operations_that_do_not_download_never_say_downloading() {
        let v = running(OperationKind::Uninstall, None);
        assert_eq!(
            labels(&v),
            [
                "Uninstalling PD2R… (disabled)",
                "Cancel safely [cancel:pd2r]"
            ]
        );
        let v = running(OperationKind::Repair, Some(Phase::Planning));
        assert_eq!(labels(&v)[0], "Repairing PD2R… (disabled)");
        // An install starts as a download, before its first progress event.
        let v = running(OperationKind::Install, None);
        assert_eq!(labels(&v)[0], "Downloading PD2R · 0% (disabled)");
    }

    #[test]
    fn cancelling_offers_no_second_cancel() {
        let mut s = state(Readiness::Ready, Some("0.4.2"), "0.4.2");
        s.running = Some(OperationKind::Update);
        s.cancelling = true;
        let v = view(&s, None, None);
        assert_eq!(labels(&v), ["Stopping PD2R safely… (disabled)"]);
    }

    #[test]
    fn applying_after_the_download_shows_a_full_bar() {
        let v = running(OperationKind::Update, Some(Phase::Activating));
        assert_eq!(v.overlay, Overlay::Progress(100));
        assert_eq!(labels(&v)[0], "Applying PD2R… (disabled)");
    }

    #[test]
    fn a_percent_change_keeps_the_menu_shape() {
        let a = running(OperationKind::Update, Some(Phase::Downloading));
        let mut b = a.clone();
        b.items[0] = Item::Disabled("Downloading PD2R · 53%".into());
        assert_eq!(shape(std::slice::from_ref(&a)), shape(&[b]));
        let applying = running(OperationKind::Update, Some(Phase::Activating));
        assert_eq!(shape(&[a]), shape(&[applying]));
    }
}
