// The one seam between the UI and native code: everything the UI calls is on `LauncherApi`.
// `src/api/live.ts` implements it over `src/ipc.ts`; `src/api/fixtures.ts` implements it for
// tests, screenshots and `?fixture=` in dev.
//
// Every native type has one definition, in `src/ipc.ts`; these are type-only re-exports, so
// nothing here loads `@tauri-apps/api`.
import type {
  DiagnosticsPreview,
  Folder,
  GameLocation,
  Inspection,
  LauncherError,
  NewsState,
  OperationOutcome,
  OperationRequest,
  ProductState,
  ProgressEvent,
  Recovery,
  ReleaseNotes,
  RunningGame,
  TrayRequest,
} from "../ipc";

export type {
  CatalogState,
  DiagnosticsPreview,
  ErrorCode,
  Folder,
  GameLocation,
  Inspection,
  InstalledRelease,
  LauncherError,
  LoaderGuidance,
  NewsItem,
  NewsState,
  OperationKind,
  OperationOutcome,
  OperationRequest,
  OutcomeReport,
  Phase,
  ProductState,
  ProgressEvent,
  Readiness,
  Recovery,
  ReleaseNotes,
  ReleaseSummary,
  RunningGame,
  TrayRequest,
} from "../ipc";

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
  /** `true` when the call created the (plugins) folder. */
  openFolder(productId: string | null, folder: Folder): Promise<boolean>;
  previewDiagnostics(includeLog: boolean): Promise<DiagnosticsPreview>;
  saveDiagnostics(includeLog: boolean): Promise<boolean>;
  showDiagnosticsInFolder(): Promise<void>;
  getStartWithWindows(): Promise<boolean>;
  setStartWithWindows(enabled: boolean): Promise<void>;

  // Window (#74)
  minimizeWindow(): Promise<void>;
  closeWindow(): Promise<void>;
  startWindowDrag(): Promise<void>;
  quitLauncher(): Promise<void>;

  // News (#80)
  getNews(productId: string): Promise<NewsState>;
  /** By index into the last getNews reply, never a URL. */
  openNewsLink(productId: string, index: number): Promise<void>;
  /** "About <product>". */
  openProductPage(productId: string): Promise<void>;
  /** The project Discord (issue 135), from "Follow for updates on Discord" on products without a page. */
  openCommunityPage(): Promise<void>;

  // Events
  onStateChanged(handler: () => void): Promise<Unlisten>;
  onProgress(handler: (event: ProgressEvent) => void): Promise<Unlisten>;
  onOutcome(handler: (outcome: OperationOutcome) => void): Promise<Unlisten>;
  onTrayRequest(handler: (request: TrayRequest) => void): Promise<Unlisten>;
}

export function isLauncherError(e: unknown): e is LauncherError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}
