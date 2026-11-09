import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** True inside the Tauri webview; false in tests and a plain browser. */
export { isTauri } from "@tauri-apps/api/core";

/** Must equal `contract::CONTRACT_VERSION` in Rust. */
export const CONTRACT_VERSION = 2;

/** Mirrors `contract::Versioned<T>`. */
export interface Versioned<T> {
  contract_version: number;
  data: T;
}

/** Mirrors `contract::PingResponse`. */
export interface PingResponse {
  app_version: string;
  display_name: string;
}

function current<T>(reply: Versioned<T>): T {
  if (reply.contract_version !== CONTRACT_VERSION) {
    throw new Error(`Unsupported contract version ${reply.contract_version}`);
  }
  return reply.data;
}

const versioned = <T>(data: T): Versioned<T> => ({ contract_version: CONTRACT_VERSION, data });

export async function ping(): Promise<PingResponse> {
  return current(await invoke<Versioned<PingResponse>>("ping"));
}

// ---- Install operation commands (#40). Mirror `app::operations` and `contract`. ----

/** Mirrors `contract::OperationKind`. */
export type OperationKind = "install" | "adopt" | "repair" | "update" | "uninstall" | "clean_install";

/** Mirrors `contract::ErrorCode`. */
export type ErrorCode =
  | "invalid_request"
  | "unsupported_contract_version"
  | "unknown_product"
  | "metadata_invalid"
  | "launcher_update_required"
  | "release_changed"
  | "product_root_not_disposable"
  | "operation_in_progress"
  | "game_running"
  | "insufficient_space"
  | "access_denied"
  | "download_failed"
  | "verification_failed"
  | "unsafe_archive_entry"
  | "inventory_mismatch"
  | "save_breaking_not_acknowledged"
  | "clean_install_consent_required"
  | "mandatory_setting_conflict"
  | "cancelled"
  | "recovery_required"
  | "internal"
  | "d2r_not_found"
  | "game_build_unsupported"
  | "loader_not_ready"
  | "product_not_installed"
  | "launch_failed";

/**
 * Mirrors `contract::LauncherError`. Every command rejects with one, except a
 * request Tauri cannot deserialize at all (a frontend bug), which rejects with a string.
 */
export interface LauncherError {
  code: ErrorCode;
  message: string;
}

/** Mirrors `contract::OperationRequest`. */
export interface OperationRequest {
  product_id: string;
  kind: OperationKind;
  acknowledge_save_breaking?: boolean;
  consent_clean_install?: boolean;
}

/** Mirrors `state::InstalledRelease`. */
export interface InstalledRelease {
  product_id: string;
  release_id: string;
  display_version: string;
  manifest_sha256: string;
  installed_at: number;
  verified: boolean;
}

/** Mirrors `app::ReleaseSummary`. */
export interface ReleaseSummary {
  release_id: string;
  display_version: string;
  save_breaking: boolean;
}

/** Mirrors `contract::GameLocation`. */
export interface GameLocation {
  path: string;
  /** `D2R.exe`'s ProductVersion, for display only. */
  build: string;
  store: "battle_net" | "unknown";
  /** Product ids whose install folder exists under this root. */
  installed_products: string[];
}

/** Mirrors `contract::Readiness`: the first reason Play is refused, or ready. */
export type Readiness =
  | { status: "d2r_missing" }
  | { status: "running"; since: number }
  | { status: "recovery_required" }
  | { status: "build_unsupported"; found: string }
  | { status: "loader_missing" }
  | { status: "loader_unsupported"; found: string; allowed: string[] }
  | { status: "not_installed" }
  | { status: "update_available" }
  | { status: "ready" };

/** Mirrors `contract::LoaderGuidance`. Open the page with `openLoaderDownloadPage`. */
export interface LoaderGuidance {
  found: string | null;
  allowed: string[];
  download_page: string;
}

/** Mirrors `contract::RunningGame`. */
export interface RunningGame {
  pid: number;
  image: string;
  since: number;
}

/** Mirrors `app::CatalogState`: the last catalog refresh in this launcher run. */
export type CatalogState =
  | { status: "not_refreshed" }
  | { status: "refreshing" }
  | { status: "refreshed" }
  | { status: "failed"; error: LauncherError };

/**
 * Mirrors `app::ProductState`. Offline/no-catalog: `installed` and `recommended`
 * are both null, so there is no release to offer; `catalog` says why.
 */
export interface ProductState {
  product_id: string;
  installed: InstalledRelease | null;
  /** Null when the feed or manifest is unavailable; never implies readiness. */
  recommended: ReleaseSummary | null;
  running: OperationKind | null;
  location: GameLocation | null;
  readiness: Readiness;
  running_game: RunningGame | null;
  loader_guidance: LoaderGuidance | null;
  catalog: CatalogState;
  /** A cancel was requested and the operation hasn't ended yet. */
  cancelling: boolean;
  /** The tracked game exited within 30 s of Play; the next Play clears it. */
  startup_failed: { exit_code: number | null } | null;
}

/** Mirrors `app::ConflictView`. */
export type Conflict =
  | { type: "setting"; file: string; key: string; required: string; found: string | null }
  | { type: "unknown_file"; path: string };

/** Mirrors `app::InspectionView`. `startOperation` needs this exact inspection. */
export interface Inspection {
  product_id: string;
  kind: OperationKind;
  release: ReleaseSummary;
  files_replaced_or_removed: number;
  files_added: number;
  /** Release files already in place and left as they are; with no installed release, an
   *  existing installation the launcher didn't make (issue 126). */
  files_kept: number;
  preserved: string[];
  backup_only: string[];
  /** Clean install: mandatory settings an edited file in `backup_only` fails; the release's
   *  default goes live instead (issue 134). Always `type: "setting"`. */
  settings_reset: Conflict[];
  conflicts: Conflict[];
  save_breaking_notice: string | null;
  space_needed: number;
  token: string;
}

/** Mirrors `contract::Phase`. */
export type Phase =
  | "planning"
  | "downloading"
  | "extracting"
  | "verifying"
  | "activating"
  | "committing"
  | "cleaning_up"
  | "recovering";

/** Mirrors `contract::ProgressEvent`, sent as `operation-progress`. */
export interface ProgressEvent {
  product_id: string;
  kind: OperationKind;
  phase: Phase;
  boundary: string | null;
  /** Filled while downloading, at most 10 events per second plus the final one. */
  bytes_done: number | null;
  bytes_total: number | null;
  /** Filled while Repair verifies. */
  files_done: number | null;
  files_total: number | null;
}

/** Mirrors `contract::OutcomeReport`. */
export interface OutcomeReport {
  /** Relative paths kept or carried forward. */
  preserved: string[];
  /** Uninstall: files left in the product root. */
  remaining: string[];
  /** Absolute paths, display only. */
  clean_install_backups: string[];
}

/** Mirrors `contract::OperationOutcome`, sent as `operation-outcome`. */
export type OperationOutcome =
  | { status: "succeeded"; product_id: string; kind: OperationKind; report: OutcomeReport }
  | { status: "cancelled"; product_id: string; kind: OperationKind }
  | { status: "failed"; product_id: string; kind: OperationKind; error: LauncherError };

/** Mirrors `app::RecoveryView`. */
export type Recovery = "nothing" | "rolled_back" | "rolled_forward";

export async function getProductState(productId: string): Promise<ProductState> {
  return current(await invoke<Versioned<ProductState>>("get_product_state", { product_id: productId }));
}

export async function inspectOperation(request: OperationRequest): Promise<Inspection> {
  return current(await invoke<Versioned<Inspection>>("inspect_operation", { request: versioned(request) }));
}

/** Starts the operation the player confirmed; progress and the outcome arrive as events. */
export async function startOperation(request: OperationRequest, inspection: Inspection): Promise<void> {
  await invoke("start_operation", { request: versioned(request), inspection_token: inspection.token });
}

export async function cancelOperation(productId: string): Promise<void> {
  await invoke("cancel_operation", { request: versioned({ product_id: productId }) });
}

/** Startup recovery before Play is enabled, and the way out of `recovery_required`. */
export async function recoverProduct(productId: string): Promise<Recovery> {
  return current(await invoke<Versioned<Recovery>>("recover_product", { product_id: productId }));
}

// ---- D2R location, readiness, catalog and Play (#60). Mirror `app::game`. ----

/** Valid D2R locations, the remembered one first. */
export async function detectGameLocations(): Promise<GameLocation[]> {
  return current(await invoke<Versioned<GameLocation[]>>("detect_game_locations"));
}

/** Selects entry `index` of the latest detectGameLocations() reply, validated again.
 *  Refused with invalid_request if the index is out of range or nothing was detected,
 *  and with game_running while the game runs. */
export async function selectGameLocation(index: number): Promise<GameLocation> {
  return current(await invoke<Versioned<GameLocation>>("select_game_location", { index }));
}

/** Opens the native folder picker; null when the player cancels. Sends no path. */
export async function chooseGameLocation(): Promise<GameLocation | null> {
  return current(await invoke<Versioned<GameLocation | null>>("choose_game_location"));
}

/** Starts a background catalog refresh; `onStateChanged` fires when it ends. */
export async function refreshCatalog(): Promise<void> {
  await invoke("refresh_catalog");
}

/** Launches the installed release; `onStateChanged` fires when the game exits. */
export async function playProduct(productId: string): Promise<RunningGame> {
  return current(await invoke<Versioned<RunningGame>>("play_product", { product_id: productId }));
}

/** Opens the official D2RLoader download page in the default browser. */
export async function openLoaderDownloadPage(): Promise<void> {
  await invoke("open_loader_download_page");
}

/** `product-state-changed`: read `getProductState` again. */
export function onStateChanged(handler: () => void): Promise<UnlistenFn> {
  return listen<Versioned<null>>("product-state-changed", (e) => {
    current(e.payload);
    handler();
  });
}

export function onProgress(handler: (event: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<Versioned<ProgressEvent>>("operation-progress", (e) => handler(current(e.payload)));
}

export function onOutcome(handler: (outcome: OperationOutcome) => void): Promise<UnlistenFn> {
  return listen<Versioned<OperationOutcome>>("operation-outcome", (e) => handler(current(e.payload)));
}

// ---- Slice 4 command gaps (#74). Mirror `app::settings`, `app::window`. ----

/** Mirrors `app::ReleaseNotes`. */
export interface ReleaseNotes {
  release_id: string;
  display_version: string;
  /** YYYY-MM-DD */
  release_date: string;
  status: "alpha" | "beta" | "release";
  /** null: a newer release whose manifest isn't cached (offline). */
  notes: string | null;
}

/** Newest first: the recommended release (if newer) and the installed release.
 *  The installed release's notes come from its cached manifest, so they work offline. */
export async function getReleaseNotes(productId: string): Promise<ReleaseNotes[]> {
  return current(await invoke<Versioned<ReleaseNotes[]>>("get_release_notes", { product_id: productId }));
}

/** Mirrors `app::Folder`. */
export type Folder = "saves" | "settings" | "product_plugins" | "global_plugins";

/** Opens a folder in Explorer; pass `null` as the product for global plugins only.
 *  A missing plugins folder is created first; resolves `true` when this call
 *  created it. Rejects with `access_denied` or `product_not_installed`. */
export async function openFolder(productId: string | null, folder: Folder): Promise<boolean> {
  return current(await invoke<Versioned<boolean>>("open_folder", { product_id: productId, folder }));
}

/** Mirrors `state::DiagnosticsReport`. */
export interface DiagnosticsPreview {
  text: string;
  never_included: string[];
}

export async function previewDiagnostics(includeLog: boolean): Promise<DiagnosticsPreview> {
  return current(await invoke<Versioned<DiagnosticsPreview>>("preview_diagnostics", { include_log: includeLog }));
}

/** Native save dialog, opened from Rust; false when the player cancels. */
export async function saveDiagnostics(includeLog: boolean): Promise<boolean> {
  return current(await invoke<Versioned<boolean>>("save_diagnostics", { include_log: includeLog }));
}

/** Opens Explorer with the last saved report selected. */
export async function showDiagnosticsInFolder(): Promise<void> {
  await invoke("show_diagnostics_in_folder");
}

export async function getStartWithWindows(): Promise<boolean> {
  return current(await invoke<Versioned<boolean>>("get_start_with_windows"));
}

export async function setStartWithWindows(enabled: boolean): Promise<void> {
  await invoke("set_start_with_windows", { enabled });
}

export async function minimizeWindow(): Promise<void> {
  await invoke("minimize_window");
}

/** A normal close, which hides the launcher to the tray. */
export async function closeWindow(): Promise<void> {
  await invoke("close_window");
}

export async function startWindowDrag(): Promise<void> {
  await invoke("start_window_drag");
}

/** Exits the launcher; refused with `operation_in_progress` while an operation runs. */
export async function quitLauncher(): Promise<void> {
  await invoke("quit_launcher");
}

/** Mirrors `contract::TrayRequest`. */
export interface TrayRequest {
  action: "save_breaking" | "show_what_happened" | "quit_while_busy";
  product_id: string;
  /** Why a tray Play was refused (show_what_happened); null otherwise. */
  error: LauncherError | null;
}

/** Event "tray-request", Versioned like the others. */
export function onTrayRequest(handler: (r: TrayRequest) => void): Promise<UnlistenFn> {
  return listen<Versioned<TrayRequest>>("tray-request", (e) => handler(current(e.payload)));
}

// ---- Launcher self-update (#88). Mirror `app::selfupdate`. ----

/** Mirrors `app::SelfUpdateStatus`; the same union as `SelfUpdateStatus` in `selfupdate/api.ts`. */
export type LauncherUpdateStatus =
  | { phase: "up_to_date"; current: string }
  | { phase: "available"; current: string; next: string }
  | { phase: "downloading"; current: string; next: string; percent: number }
  | { phase: "ready"; current: string; next: string }
  | { phase: "restarting"; current: string; next: string }
  /** Quit is installing the ready update; the launcher closes and doesn't relaunch. */
  | { phase: "quitting"; current: string; next: string }
  | { phase: "updated"; current: string }
  | { phase: "download_failed"; current: string; next: string }
  | { phase: "signature_rejected"; current: string; next: string };

export async function getLauncherUpdateStatus(): Promise<LauncherUpdateStatus> {
  return current(await invoke<Versioned<LauncherUpdateStatus>>("get_launcher_update_status"));
}

/** Checks the launcher's configured update endpoint; rejects with `download_failed` when it can't. */
export async function checkForLauncherUpdate(): Promise<void> {
  await invoke("check_for_launcher_update");
}

/** Starts downloading the update the last check found; progress arrives as phase events. */
export async function downloadLauncherUpdate(): Promise<void> {
  await invoke("download_launcher_update");
}

/** Installs the ready update and restarts. Refused with `game_running` or
 *  `operation_in_progress` while held, and `verification_failed` if the kept installer changed. */
export async function restartToUpdateLauncher(): Promise<void> {
  await invoke("restart_to_update_launcher");
}

/** Opens the launcher release page; the address is built natively. */
export async function openLauncherWhatsNew(): Promise<void> {
  await invoke("open_launcher_whats_new");
}

export async function getLauncherAutoDownload(): Promise<boolean> {
  return current(await invoke<Versioned<boolean>>("get_launcher_auto_download"));
}

export async function setLauncherAutoDownload(enabled: boolean): Promise<void> {
  await invoke("set_launcher_auto_download", { enabled });
}

/** Event "self-update-phase", Versioned like the others. */
export function onLauncherUpdatePhase(handler: (status: LauncherUpdateStatus) => void): Promise<UnlistenFn> {
  return listen<Versioned<LauncherUpdateStatus>>("self-update-phase", (e) => handler(current(e.payload)));
}

// ---- News feed and outbound links (#80). Mirror `app::news`. ----

/** Mirrors `app::NewsItem`: text only; the link stays native. */
export interface NewsItem {
  /** "Announcement" or "Guide". */
  tag: string;
  title: string;
  /** Optional short description under the title (issue 139); absent when news.json has none. */
  summary?: string;
  /** YYYY-MM-DD or YYYY-MM-DDTHH:MM:SSZ */
  published: string;
}

/** Mirrors `app::NewsState`. Offline also covers a missing or invalid news.json. */
export type NewsState = { status: "ok"; items: NewsItem[] } | { status: "offline" };

/** Up to three announcements and guides, newest first. Never rejects for offline. */
export async function getNews(productId: string): Promise<NewsState> {
  return current(await invoke<Versioned<NewsState>>("get_news", { product_id: productId }));
}

/** Opens item `index` of the last getNews reply for the product. Takes no URL. */
export async function openNewsLink(productId: string, index: number): Promise<void> {
  await invoke("open_news_link", { product_id: productId, index });
}

/** "About <product>": the bundled page. Rejects for a product without one. */
export async function openProductPage(productId: string): Promise<void> {
  await invoke("open_product_page", { product_id: productId });
}

/** The project Discord, bundled in the launcher. Takes nothing. */
export async function openCommunityPage(): Promise<void> {
  await invoke("open_community_page");
}
