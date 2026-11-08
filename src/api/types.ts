// The one seam between the UI and native code: everything the UI calls is on `LauncherApi`.
// `src/api/fixtures.ts` implements it now; #79 adds the live adapter over `src/ipc.ts`.
//
// The #69 types are reused from `src/ipc.ts` unchanged (type-only imports, so nothing here loads
// `@tauri-apps/api`). #74's frozen additions are declared below with the names and shapes copied
// from #74; #79 replaces these copies with re-exports from `src/ipc.ts` once #74 merges.
import type {
  CatalogState,
  ErrorCode,
  GameLocation as GameLocationV1,
  Inspection,
  InstalledRelease,
  LauncherError,
  LoaderGuidance,
  OperationKind,
  OperationRequest,
  Phase,
  ProductState as ProductStateV1,
  ProgressEvent as ProgressEventV1,
  Readiness,
  Recovery,
  ReleaseSummary,
  RunningGame,
} from "../ipc";

export type {
  CatalogState,
  ErrorCode,
  Inspection,
  InstalledRelease,
  LauncherError,
  LoaderGuidance,
  OperationKind,
  OperationRequest,
  Phase,
  Readiness,
  Recovery,
  ReleaseSummary,
  RunningGame,
};

// ---- #74 (frozen) ----

export interface GameLocation extends GameLocationV1 {
  /** Product ids whose install folder exists under this root. */
  installed_products: string[];
}

export interface ProductState extends Omit<ProductStateV1, "location"> {
  location: GameLocation | null;
  /** A cancel was requested and the operation hasn't ended yet. */
  cancelling: boolean;
  /** The tracked game exited within 30 s of Play; the next Play clears it. */
  startup_failed: { exit_code: number | null } | null;
}

export interface ProgressEvent extends ProgressEventV1 {
  /** Filled while Repair verifies. */
  files_done: number | null;
  files_total: number | null;
}

export interface OutcomeReport {
  /** Relative paths kept or carried forward. */
  preserved: string[];
  /** Uninstall: files left in the product root. */
  remaining: string[];
  /** Absolute paths, display only. */
  clean_install_backups: string[];
}

export type OperationOutcome =
  | { status: "succeeded"; product_id: string; kind: OperationKind; report: OutcomeReport }
  | { status: "cancelled"; product_id: string; kind: OperationKind }
  | { status: "failed"; product_id: string; kind: OperationKind; error: LauncherError };

export interface ReleaseNotes {
  release_id: string;
  display_version: string;
  /** YYYY-MM-DD */
  release_date: string;
  status: "alpha" | "beta" | "release";
  /** null: a newer release whose manifest isn't cached (offline). */
  notes: string | null;
}

export type Folder = "saves" | "settings" | "product_plugins" | "global_plugins";

export interface DiagnosticsPreview {
  text: string;
  never_included: string[];
}

export interface TrayRequest {
  action: "save_breaking" | "show_what_happened" | "quit_while_busy";
  product_id: string;
  error: LauncherError | null;
}

// ---- News view types (#80 produces them later) ----

export interface NewsItem {
  tag: string;
  title: string;
  /** ISO date */
  published: string;
}

export type NewsState = { status: "ok"; items: NewsItem[] } | { status: "offline" };

/** Stops a subscription. Compatible with Tauri's `UnlistenFn`. */
export type Unlisten = () => void;

export interface LauncherApi {
  // Product state and operations (#40, #60)
  getProductState(productId: string): Promise<ProductState>;
  inspectOperation(request: OperationRequest): Promise<Inspection>;
  startOperation(request: OperationRequest, inspection: Inspection): Promise<void>;
  cancelOperation(productId: string): Promise<void>;
  recoverProduct(productId: string): Promise<Recovery>;
  playProduct(productId: string): Promise<RunningGame>;
  refreshCatalog(): Promise<void>;

  // D2R location (#60, #74)
  detectGameLocations(): Promise<GameLocation[]>;
  /** Selects entry `index` of the latest detectGameLocations() reply. */
  selectGameLocation(index: number): Promise<GameLocation>;
  chooseGameLocation(): Promise<GameLocation | null>;
  openLoaderDownloadPage(): Promise<void>;

  // Patch Notes, folders, diagnostics, Start with Windows (#74)
  getReleaseNotes(productId: string): Promise<ReleaseNotes[]>;
  openFolder(productId: string | null, folder: Folder): Promise<void>;
  previewDiagnostics(includeLog: boolean): Promise<DiagnosticsPreview>;
  saveDiagnostics(includeLog: boolean): Promise<boolean>;
  showDiagnosticsInFolder(): Promise<void>;
  getStartWithWindows(): Promise<boolean>;
  setStartWithWindows(enabled: boolean): Promise<void>;

  // Window (#74)
  minimizeWindow(): Promise<void>;
  toggleMaximizeWindow(): Promise<void>;
  closeWindow(): Promise<void>;
  startWindowDrag(): Promise<void>;
  quitLauncher(): Promise<void>;

  // News (#80)
  getNews(productId: string): Promise<NewsState>;
  /** By index into the last getNews reply, never a URL. */
  openNewsLink(productId: string, index: number): Promise<void>;
  /** "About <product>". */
  openProductPage(productId: string): Promise<void>;

  // Events
  onStateChanged(handler: () => void): Promise<Unlisten>;
  onProgress(handler: (event: ProgressEvent) => void): Promise<Unlisten>;
  onOutcome(handler: (outcome: OperationOutcome) => void): Promise<Unlisten>;
  onTrayRequest(handler: (request: TrayRequest) => void): Promise<Unlisten>;
}

export function isLauncherError(e: unknown): e is LauncherError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}
