//! The tray (variant A, #27): icon, tooltip and a native menu that follow product
//! state. State is read natively through the same commands the frontend calls, so
//! every tray action passes the same checks and locks.
//!
//! The pure part ([`view`], [`quit_plan`]) is unit tested; [`start`] wires it to a
//! real tray icon and runs actions on one worker thread, never the main thread.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use tauri::async_runtime::block_on;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Listener, Manager, Runtime};

use super::icon::{self, Overlay};
use super::{request, show_main, TrayAction};
use crate::app::{self, Launcher, ProductState, OUTCOME_EVENT, PROGRESS_EVENT, STATE_EVENT};
use crate::catalog::{Feed, FEED_IDENTITY};
use crate::contract::{
    CancelRequest, OperationKind, OperationOutcome, OperationRequest, Phase, ProductId,
    ProgressEvent, Readiness, Versioned, UMBRELLA_DISPLAY_NAME,
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

pub fn quit_plan(states: &[ProductState]) -> Quit {
    states
        .iter()
        .find(|s| s.running.is_some())
        .map_or(Quit::Exit, |s| Quit::Busy(s.product_id.clone()))
}

/// shortcut: the upper-cased product id ("pd2r" → "PD2R"); use a catalog display
/// name once a second product gets a release.
fn short_name(product: &ProductId) -> String {
    product.0.to_uppercase()
}

fn percent(p: &ProgressEvent) -> u8 {
    match (p.bytes_done, p.bytes_total) {
        (Some(done), Some(total)) if total > 0 => (done.min(total) * 100 / total) as u8,
        _ => 0,
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
        let target = match (kind, &state.recommended) {
            (OperationKind::Install | OperationKind::Update, Some(r)) => {
                format!(" {}", r.display_version)
            }
            _ => String::new(),
        };
        let downloading =
            progress.is_none_or(|p| matches!(p.phase, Phase::Planning | Phase::Downloading));
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
                format!("Applying{target}"),
                vec![
                    Item::Disabled(format!("Applying {name}…")),
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
    if failed.is_some() {
        return error("Something went wrong");
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

/// What the tray knows beyond `get_product_state`.
#[derive(Default)]
struct Model {
    states: Vec<ProductState>,
    progress: HashMap<ProductId, ProgressEvent>,
    failed: HashMap<ProductId, OperationKind>,
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
                        product_id, kind, ..
                    } => (product_id, Some(kind)),
                    OperationOutcome::Succeeded { product_id, .. }
                    | OperationOutcome::Cancelled { product_id, .. } => (product_id, None),
                };
                model.progress.remove(&product);
                match failed {
                    Some(kind) => model.failed.insert(product, kind),
                    None => model.failed.remove(&product),
                };
                model.refresh(app);
            }
            Msg::Click(id) => model.click(app, &id),
        }
        let views = model.views();
        // Progress arrives up to 10 times a second; rebuild only on a visible change.
        if shown.as_ref() != Some(&views) {
            apply(app, tray, &views);
            shown = Some(views);
        }
    }
}

impl Model {
    fn views(&self) -> Vec<ProductView> {
        self.states
            .iter()
            .map(|s| {
                let id = &s.product_id;
                view(s, self.progress.get(id), self.failed.get(id).copied())
            })
            .collect()
    }

    fn refresh<R: Runtime>(&mut self, app: &AppHandle<R>) {
        let ids = product_ids(&app.state::<Arc<Launcher>>());
        self.states = ids
            .into_iter()
            .filter_map(|id| block_on(app::get_product_state(app.state(), id)).ok())
            .map(|v| v.data)
            .filter(has_release)
            .collect();
    }

    fn click<R: Runtime>(&mut self, app: &AppHandle<R>, id: &str) {
        if id == "open" {
            return show_main(app);
        }
        self.refresh(app);
        if id == "quit" {
            return match quit_plan(&self.states) {
                Quit::Exit => app.exit(0),
                Quit::Busy(product) => request(app, TrayAction::QuitWhileBusy, &product),
            };
        }
        let Some((act, product)) = id.split_once(':') else {
            return;
        };
        let product = ProductId(product.to_owned());
        match act {
            "play" => {
                let _ = block_on(app::play_product(app.clone(), app.state(), product));
                self.refresh(app);
            }
            "update" => self.start(app, product, OperationKind::Update),
            "cancel" => {
                let cancel = Versioned::new(CancelRequest {
                    product_id: product,
                });
                let _ = block_on(app::cancel_operation(app.state(), cancel));
            }
            "retry" => match self.failed.get(&product).copied() {
                // Never Uninstall or Clean install again without the window.
                Some(
                    kind @ (OperationKind::Install | OperationKind::Update | OperationKind::Repair),
                ) => self.start(app, product, kind),
                _ => {
                    // "Check again": clear the failure and reassess.
                    self.failed.remove(&product);
                    let _ = block_on(app::refresh_catalog(app.clone(), app.state()));
                    self.refresh(app);
                }
            },
            "show" => request(app, TrayAction::ShowWhatHappened, &product),
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
            return request(app, TrayAction::SaveBreaking, &product);
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
            Err(_) => self.failed.insert(product, kind),
        };
    }
}

/// Feed products in feed order, then installed products the feed no longer lists.
fn product_ids(launcher: &Launcher) -> Vec<ProductId> {
    let mut state = launcher.state.lock().unwrap_or_else(|e| e.into_inner());
    let feed = state
        .cached(FEED_IDENTITY)
        .and_then(|c| Feed::parse(&c.bytes).ok());
    let mut ids: Vec<ProductId> = feed
        .into_iter()
        .flat_map(|f| f.products)
        .map(|p| p.product_id)
        .collect();
    for r in state.installed_releases() {
        if !ids.contains(&r.product_id) {
            ids.push(r.product_id.clone());
        }
    }
    ids
}

fn apply<R: Runtime>(app: &AppHandle<R>, tray: &TrayIcon<R>, views: &[ProductView]) {
    // shortcut: the icon follows the first product (only PD2R has a release);
    // pick the most urgent overlay once a second product ships.
    let overlay = views.first().map_or(Overlay::Idle, |v| v.overlay);
    let _ = tray.set_icon(Some(icon::draw(overlay)));
    let _ = tray.set_tooltip(Some(tooltip(views)));
    if let Ok(menu) = menu(app, views) {
        bold_first_item(&menu);
        let _ = tray.set_menu(Some(menu));
    }
}

/// Open, separator, the product items, separator, Quit (spec order).
fn menu<R: Runtime>(app: &AppHandle<R>, views: &[ProductView]) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    let open = format!("Open {UMBRELLA_DISPLAY_NAME}");
    menu.append(&MenuItem::with_id(app, "open", open, true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let items: Vec<&Item> = views.iter().flat_map(|v| &v.items).collect();
    for item in &items {
        match item {
            Item::Action { id, label } => {
                menu.append(&MenuItem::with_id(app, id, label, true, None::<&str>)?)?
            }
            Item::Disabled(label) => {
                menu.append(&MenuItem::new(app, label, false, None::<&str>)?)?
            }
        }
    }
    if !items.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok(menu)
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

    #[test]
    fn quit_asks_only_while_an_operation_runs() {
        let idle = state(Readiness::Ready, Some("0.4.2"), "0.4.2");
        let game = state(Readiness::Running { since: 1 }, Some("0.4.2"), "0.4.2");
        assert_eq!(quit_plan(&[idle.clone(), game]), Quit::Exit);
        let mut busy = idle;
        busy.running = Some(OperationKind::Update);
        assert_eq!(quit_plan(&[busy]), Quit::Busy(ProductId("pd2r".into())));
    }
}
