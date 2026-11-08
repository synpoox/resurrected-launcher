import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Must equal `contract::CONTRACT_VERSION` in Rust. */
export const CONTRACT_VERSION = 1;

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
  preserved: string[];
  backup_only: string[];
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
  bytes_done: number | null;
  bytes_total: number | null;
}

/** Mirrors `contract::OperationOutcome`, sent as `operation-outcome`. */
export type OperationOutcome =
  | { status: "succeeded"; product_id: string; kind: OperationKind }
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
