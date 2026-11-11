// Fixture implementation of `LauncherApi`: one named scene per journey key in
// docs/specs/launcher-ui.md, plus `quit-busy`. Copy, versions and paths are illustrative.
// Nothing here talks to native code; every call is recorded in `calls` for tests.
import type {
  GameLocation,
  Inspection,
  LauncherApi,
  LauncherError,
  NewsState,
  OperationKind,
  OperationOutcome,
  OperationRequest,
  ProductState,
  ProgressEvent,
  ReleaseNotes,
  TrayRequest,
  Unlisten,
} from "./types";

/** What the UI opens on its own when a scene loads (a dialog, the menu, or an operation). */
export type SceneOpen = "d2r-choose" | "save-breaking" | "uninstall" | "manage" | "repair" | "clean-install" | "quit";

export interface Scene {
  key: string;
  label: string;
  /** The product the window opens on (defaults to PD2R). */
  product?: string;
  state: ProductState;
  notes: ReleaseNotes[];
  news: Record<string, NewsState>;
  locations: GameLocation[];
  /** Events replayed to `onProgress`: the first at once, then one per tick. */
  progress?: ProgressEvent[];
  /** Start at the last progress event instead of the first (replaying the ones before it). */
  progressAtEnd?: boolean;
  open?: SceneOpen;
  /** Overrides of what `inspectOperation` answers, per kind (an existing install, issue 126). */
  inspections?: Partial<Record<OperationKind, Partial<Inspection>>>;
}

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const DAY = 86_400_000;
const NOW = Date.now();
const iso = (daysAgo: number) => new Date(NOW - daysAgo * DAY).toISOString().slice(0, 10);

const NOTES_042 = `Loot filter fixes, corruption lines on items and a stash crash fix.

#### Loot filter

- Rune names highlight again in the default filter.
- Custom filters keep their colours after an update.

#### Fixes

- Fixed a crash when opening the stash with a full cube.
- Fixed mercenary equipment resetting after a zone change.`;

const NOTES_041 = `TCP/IP co-op join fixes and early Act 1 drop rates.

#### Co-op

- TCP/IP games now list correctly on the join screen.
- Players joining mid-game keep their party.

#### Balance

- Rebalanced early Act 1 drop rates.`;

const NOTES_050 = `New save format for the ladder preview. Read the save-breaking warning before updating.

#### Saves

- Characters use a new save format.`;

const r = (version: string, daysAgo: number, notes: string | null): ReleaseNotes => ({
  release_id: `pd2r-${version}-alpha`,
  display_version: version,
  release_date: iso(daysAgo),
  status: "alpha",
  notes,
});

const installed = (version: string) => ({
  product_id: "pd2r",
  release_id: `pd2r-${version}-alpha`,
  display_version: version,
  manifest_sha256: "0".repeat(64),
  installed_at: Math.floor(NOW / 1000) - 20 * 86_400,
  verified: true,
});

const summary = (version: string, saveBreaking = false) => ({
  release_id: `pd2r-${version}-alpha`,
  display_version: version,
  save_breaking: saveBreaking,
});

const LOCATION: GameLocation = {
  path: "C:\\Program Files (x86)\\Diablo II Resurrected",
  build: "3.3.0",
  store: "battle_net",
  installed_products: ["pd2r"],
};

const LOCATIONS: GameLocation[] = [
  LOCATION,
  { path: "D:\\Games\\Diablo II Resurrected", build: "3.3.0", store: "unknown", installed_products: [] },
];

const NEWS: Record<string, NewsState> = {
  pd2r: {
    status: "ok",
    items: [
      {
        tag: "Announcement",
        title: "TCP/IP co-op testing weekend",
        summary: "Two days of open direct co-op games on the alpha.",
        published: iso(7),
      },
      {
        tag: "Guide",
        title: "Backing up your saves before an update",
        summary: "Where D2R keeps characters and stash, and how to copy them somewhere safe first.",
        published: iso(14),
      },
      { tag: "Guide", title: "Setting up D2RLoader", published: iso(30) },
    ],
  },
  "project-forever": { status: "ok", items: [{ tag: "Dev update", title: "Legacy items: first look", published: iso(5) }] },
  "project-09": { status: "ok", items: [] },
};

const OFFLINE_NEWS: Record<string, NewsState> = {
  pd2r: { status: "offline" },
  "project-forever": { status: "offline" },
  "project-09": { status: "offline" },
};

function base(overrides: Partial<ProductState> = {}): ProductState {
  return {
    product_id: "pd2r",
    installed: installed("0.4.2"),
    recommended: summary("0.4.2"),
    running: null,
    location: LOCATION,
    readiness: { status: "ready" },
    running_game: null,
    loader_guidance: null,
    catalog: { status: "refreshed" },
    cancelling: false,
    startup_failed: null,
    ...overrides,
  };
}

const progress = (
  kind: OperationKind,
  phase: ProgressEvent["phase"],
  bytes: [number, number] | null,
  files: [number, number] | null = null,
): ProgressEvent => ({
  product_id: "pd2r",
  kind,
  phase,
  boundary: null,
  bytes_done: bytes?.[0] ?? null,
  bytes_total: bytes?.[1] ?? null,
  files_done: files?.[0] ?? null,
  files_total: files?.[1] ?? null,
});

const download = (startMb: number, steps: number): ProgressEvent[] =>
  Array.from({ length: steps }, (_, i) => progress("install", "downloading", [(startMb + i * 12) * MB, 1.2 * GB]));

const offlineCatalog: LauncherError = { code: "download_failed", message: "The release catalog couldn't be reached." };

const NOTES_READY = [r("0.4.2", 3, NOTES_042)];
const NOTES_UPDATE = [r("0.4.2", 3, NOTES_042), r("0.4.1", 21, NOTES_041)];

const scene = (s: Omit<Scene, "notes" | "news" | "locations"> & Partial<Scene>): Scene => ({
  notes: NOTES_READY,
  news: NEWS,
  locations: LOCATIONS,
  ...s,
});

const notInstalled = { installed: null, readiness: { status: "not_installed" } as const };

/** In the spec's journey table order, then the extra scenes. */
export const SCENES: readonly Scene[] = [
  scene({
    key: "d2r-missing",
    label: "D2R not found",
    state: base({ ...notInstalled, location: null, readiness: { status: "d2r_missing" } }),
    notes: [],
  }),
  scene({
    key: "d2r-choose",
    label: "Choose D2R location",
    state: base({ ...notInstalled, location: null, readiness: { status: "d2r_missing" } }),
    notes: [],
    open: "d2r-choose",
  }),
  scene({
    key: "loader-missing",
    label: "Loader not ready",
    state: base({
      readiness: { status: "loader_missing" },
      loader_guidance: { found: null, allowed: ["1.3.1-beta"], download_page: "https://d2rloader.net/download.html" },
    }),
  }),
  scene({
    key: "loader-modified",
    label: "Loader not official",
    state: base({
      readiness: { status: "loader_modified", file: "D2RLoader.exe", version: "1.3.1-beta" },
      loader_guidance: { found: "1.3.1-beta", allowed: ["1.3.1-beta"], download_page: "https://d2rloader.net/download.html" },
    }),
  }),
  scene({ key: "install", label: "Ready to install", state: base(notInstalled), notes: [r("0.4.2", 3, NOTES_042)] }),
  scene({
    key: "adopt",
    label: "Existing install to adopt",
    state: base(notInstalled),
    notes: [r("0.4.2", 3, NOTES_042)],
    inspections: { install: { files_added: 0, files_replaced_or_removed: 0, files_kept: 2341, space_needed: 0 } },
  }),
  scene({
    key: "clean-install",
    label: "Existing install that differs",
    state: base(notInstalled),
    notes: [r("0.4.2", 3, NOTES_042)],
    inspections: {
      install: {
        files_added: 1200,
        files_replaced_or_removed: 0,
        files_kept: 1141,
        conflicts: [{ type: "unknown_file", path: "PD2Resurrected.mpq/data/global/excel/charstats.txt" }],
      },
      clean_install: {
        files_added: 2341,
        files_replaced_or_removed: 0,
        preserved: ["lootfilter/default.filter"],
        backup_only: [
          "d2rloader/config/d2rloader.toml",
          ...Array.from({ length: 100 }, (_, i) => `PD2Resurrected.mpq/data/global/excel/old-${i}.txt`),
        ],
        settings_reset: [
          {
            type: "setting",
            file: "d2rloader/config/d2rloader.toml",
            key: "d2rcore.save_format.mode",
            required: "pd2r",
            found: "vanilla",
          },
        ],
      },
    },
    open: "clean-install",
  }),
  scene({
    key: "downloading",
    label: "Install progress",
    state: base({ ...notInstalled, running: "install" }),
    notes: [r("0.4.2", 3, NOTES_042)],
    progress: download(540, 40),
  }),
  scene({
    key: "applying",
    label: "Final application",
    state: base({ ...notInstalled, running: "install" }),
    notes: [r("0.4.2", 3, NOTES_042)],
    progressAtEnd: true,
    progress: [progress("install", "downloading", [1.2 * GB, 1.2 * GB]), progress("install", "activating", null)],
  }),
  scene({
    key: "cancelling",
    label: "Safe stop",
    state: base({ ...notInstalled, running: "install", cancelling: true }),
    notes: [r("0.4.2", 3, NOTES_042)],
    progressAtEnd: true,
    progress: [progress("install", "downloading", [1.2 * GB, 1.2 * GB]), progress("install", "recovering", null)],
  }),
  scene({ key: "ready", label: "Ready", state: base() }),
  scene({
    key: "update",
    label: "Update vs play installed",
    state: base({ installed: installed("0.4.1"), readiness: { status: "update_available" } }),
    notes: NOTES_UPDATE,
  }),
  scene({
    key: "save-breaking",
    label: "Save-breaking",
    state: base({
      installed: installed("0.4.1"),
      recommended: summary("0.5.0", true),
      readiness: { status: "update_available" },
    }),
    notes: [r("0.5.0", 1, NOTES_050), r("0.4.1", 21, NOTES_041)],
    open: "save-breaking",
  }),
  scene({
    // 0.4.1 -> 0.5.1 skips the save-breaking 0.5.0: the target carries no notice of its own (#213).
    key: "save-breaking-skipped",
    label: "Save-breaking (skipped release)",
    state: base({
      installed: installed("0.4.1"),
      recommended: summary("0.5.1"),
      readiness: { status: "update_available" },
    }),
    notes: [r("0.5.1", 1, NOTES_050), r("0.4.1", 21, NOTES_041)],
    open: "save-breaking",
    inspections: { update: { breaks_saves: true, save_breaking_notice: null } },
  }),
  scene({
    key: "blocked",
    label: "Blocked prerequisites",
    state: base({ readiness: { status: "build_unsupported", found: "3.3.1" } }),
  }),
  scene({
    key: "running",
    label: "Running",
    state: base({
      readiness: { status: "running", since: Math.floor(NOW / 1000) - 12 * 60 },
      running_game: { pid: 4242, image: "D2R.exe", since: Math.floor(NOW / 1000) - 12 * 60 },
    }),
  }),
  scene({ key: "startup-failed", label: "Startup failure", state: base({ startup_failed: { exit_code: 1 } }) }),
  scene({ key: "repair", label: "Repair", state: base(), open: "repair" }),
  scene({
    key: "repair-needed",
    label: "Repair didn't finish",
    state: base({ installed: { ...installed("0.4.2"), verified: false }, readiness: { status: "repair_needed" } }),
  }),
  scene({ key: "manage", label: "Manage menu", state: base(), open: "manage" }),
  scene({ key: "uninstall", label: "Uninstall", state: base(), open: "uninstall" }),
  scene({
    key: "offline",
    label: "Offline",
    state: base({
      installed: installed("0.4.1"),
      recommended: null,
      catalog: { status: "failed", error: offlineCatalog },
    }),
    notes: [r("0.4.2", 3, null), r("0.4.1", 21, NOTES_041)],
    news: OFFLINE_NEWS,
  }),
  scene({
    key: "offline-empty",
    label: "Offline, never installed",
    state: base({ ...notInstalled, recommended: null, catalog: { status: "failed", error: offlineCatalog } }),
    notes: [],
    news: OFFLINE_NEWS,
  }),
  scene({ key: "coming-soon", label: "Coming soon", product: "project-forever", state: base() }),
  scene({
    key: "quit-busy",
    label: "Quit while busy",
    state: base({ ...notInstalled, running: "install" }),
    notes: [r("0.4.2", 3, NOTES_042)],
    progress: download(540, 40),
    open: "quit",
  }),
];

export const DEFAULT_SCENE = "ready";

export function sceneByKey(key: string): Scene | undefined {
  return SCENES.find((s) => s.key === key);
}

export interface Call {
  name: keyof LauncherApi;
  args: unknown[];
}

export interface FixtureApi extends LauncherApi {
  scene: Scene;
  calls: Call[];
  /** Stops the progress timer. */
  dispose(): void;
  /** Sends one progress event to every subscriber (tests). */
  emitProgress(event: ProgressEvent): void;
  /** Sends an operation outcome to every subscriber (tests). */
  emitOutcome(outcome: OperationOutcome): void;
  /** Sends a tray request to every subscriber (tests). */
  emitTrayRequest(request: TrayRequest): void;
  /** The folder the next `chooseGameLocation` picker returns; null cancels (the default). */
  picked: GameLocation | null;
}

export interface FixtureOptions {
  /** Milliseconds between progress events (default 1000). */
  tick?: number;
}

const refuse = (code: LauncherError["code"], message: string): Promise<never> =>
  Promise.reject({ code, message } satisfies LauncherError);

/** A fresh fixture API for one scene. Mutations stay inside this instance. */
export function createFixtureApi(key: string, options: FixtureOptions = {}): FixtureApi {
  const found = sceneByKey(key);
  if (!found) throw new Error(`Unknown fixture "${key}"`);
  const s: Scene = structuredClone(found);
  const tick = options.tick ?? 1000;
  const calls: Call[] = [];
  let state = s.state;
  let detected: GameLocation[] = [];
  let script = s.progress ?? [];
  let step = s.progressAtEnd ? script.length - 1 : 0;
  let timer: ReturnType<typeof setInterval> | undefined;
  const changed = new Set<() => void>();
  const progressed = new Set<(e: ProgressEvent) => void>();
  const outcomes = new Set<(o: OperationOutcome) => void>();
  const trays = new Set<(r: TrayRequest) => void>();

  const record = (name: keyof LauncherApi, ...args: unknown[]) => calls.push({ name, args });
  const set = (next: Partial<ProductState>) => {
    state = { ...state, ...next };
    changed.forEach((h) => h());
  };
  const emit = (o: OperationOutcome) => outcomes.forEach((h) => h(o));
  const run = () => {
    clearInterval(timer);
    if (script.length > 1) {
      timer = setInterval(() => {
        if (step < script.length - 1) {
          step += 1;
          progressed.forEach((h) => h(script[step]));
        } else clearInterval(timer);
      }, tick);
    }
  };
  const subscribe = <T>(set: Set<T>, handler: T): Promise<Unlisten> => {
    set.add(handler);
    return Promise.resolve(() => void set.delete(handler));
  };
  const productOnly = (id: string) =>
    id === state.product_id ? null : refuse("unknown_product", `No release for ${id}.`);
  const openedPlugins = new Set<string>();
  const done = (name: keyof LauncherApi, ...args: unknown[]) => {
    record(name, ...args);
    return Promise.resolve();
  };

  const inspection = (request: OperationRequest): Inspection => {
    const release = state.recommended ?? {
      release_id: state.installed?.release_id ?? "",
      display_version: state.installed?.display_version ?? "",
      save_breaking: false,
    };
    return {
      product_id: request.product_id,
      kind: request.kind,
      release,
      files_replaced_or_removed: request.kind === "repair" ? 3 : 120,
      files_added: request.kind === "install" ? 2341 : 4,
      files_kept: 0,
      preserved: request.kind === "uninstall" ? ["d2rloader/config.ini", "lootfilter/default.filter"] : [],
      backup_only: [],
      settings_reset: [],
      conflicts: [],
      breaks_saves: release.save_breaking,
      save_breaking_notice: release.save_breaking
        ? "alpha 0.5.0 changes the save format. Characters from 0.4.x may not load."
        : null,
      space_needed: Math.round(3.6 * GB),
      token: `${request.kind}-token`,
      ...s.inspections?.[request.kind],
    };
  };

  const api: FixtureApi = {
    scene: s,
    calls,
    emitProgress: (e) => progressed.forEach((h) => h(e)),
    emitOutcome: emit,
    emitTrayRequest: (r) => trays.forEach((h) => h(r)),
    dispose: () => {
      clearInterval(timer);
      timer = undefined;
    },

    getProductState(id) {
      record("getProductState", id);
      return productOnly(id) ?? Promise.resolve(structuredClone(state));
    },
    inspectOperation(request) {
      record("inspectOperation", request);
      if (state.running) return refuse("operation_in_progress", "An operation is running.");
      return productOnly(request.product_id) ?? Promise.resolve(inspection(request));
    },
    startOperation(request, insp) {
      record("startOperation", request, insp);
      if (insp.token !== `${request.kind}-token`) return refuse("release_changed", "Inspect again.");
      if (insp.breaks_saves && request.kind === "update" && !request.acknowledge_save_breaking) {
        return refuse("save_breaking_not_acknowledged", "Acknowledge the save-breaking release first.");
      }
      if (insp.conflicts.length > 0) {
        return refuse("inventory_mismatch", "The product folder has files the launcher did not install.");
      }
      if (request.kind === "clean_install" && !request.consent_clean_install) {
        return refuse("clean_install_consent_required", "Clean install needs the player's consent.");
      }
      if (request.kind === "uninstall") {
        set({ installed: null, readiness: { status: "not_installed" } });
        emit({
          status: "succeeded",
          product_id: request.product_id,
          kind: "uninstall",
          report: { preserved: insp.preserved, remaining: ["lootfilter/my-filter.filter"], clean_install_backups: [] },
        });
        return Promise.resolve();
      }
      script =
        request.kind === "repair"
          ? Array.from({ length: 12 }, (_, i) => progress("repair", "verifying", null, [Math.min(2341, 1478 + i * 120), 2341]))
          : download(0, 100).map((e) => ({ ...e, kind: request.kind }));
      step = 0;
      set({ running: request.kind });
      progressed.forEach((h) => h(script[0]));
      run();
      return Promise.resolve();
    },
    cancelOperation(id) {
      record("cancelOperation", id);
      if (!state.running) return refuse("invalid_request", `${id} has no operation running`);
      const kind = state.running;
      set({ cancelling: true });
      setTimeout(() => {
        clearInterval(timer);
        set({ running: null, cancelling: false });
        emit({ status: "cancelled", product_id: id, kind });
      }, tick);
      return Promise.resolve();
    },
    recoverProduct(id) {
      record("recoverProduct", id);
      return Promise.resolve("nothing");
    },
    playProduct(id) {
      record("playProduct", id);
      const game = { pid: 4242, image: "D2R.exe", since: Math.floor(Date.now() / 1000) };
      set({ readiness: { status: "running", since: game.since }, running_game: game, startup_failed: null });
      return Promise.resolve(game);
    },
    refreshCatalog: () => done("refreshCatalog"),

    detectGameLocations() {
      record("detectGameLocations");
      detected = structuredClone(s.locations);
      return Promise.resolve(structuredClone(detected));
    },
    selectGameLocation(index) {
      record("selectGameLocation", index);
      if (state.readiness.status === "running") return refuse("game_running", "Quit the game first.");
      const chosen = detected[index];
      if (!chosen) return refuse("invalid_request", "No such detected location.");
      set({ location: chosen, readiness: state.installed ? { status: "ready" } : { status: "not_installed" } });
      return Promise.resolve(structuredClone(chosen));
    },
    chooseGameLocation() {
      record("chooseGameLocation");
      if (state.readiness.status === "running") return refuse("game_running", "Quit the game first.");
      const chosen = api.picked;
      if (!chosen) return Promise.resolve(null);
      // Like the backend, a remembered choice is followed by product-state-changed.
      set({ location: chosen, readiness: state.installed ? { status: "ready" } : { status: "not_installed" } });
      return Promise.resolve(structuredClone(chosen));
    },
    openLoaderDownloadPage: () => done("openLoaderDownloadPage"),

    getReleaseNotes(id) {
      record("getReleaseNotes", id);
      return productOnly(id) ?? Promise.resolve(structuredClone(s.notes));
    },
    openFolder(id, folder) {
      record("openFolder", id, folder);
      // Like the backend: a plugins folder is created on its first open only.
      const key = `${id}/${folder}`;
      const created = folder.endsWith("plugins") && !openedPlugins.has(key);
      openedPlugins.add(key);
      return Promise.resolve(created);
    },
    previewDiagnostics(includeLog) {
      record("previewDiagnostics", includeLog);
      return Promise.resolve({
        text: `Launcher diagnostics (fixture)\nUser: <you>\nLog included: ${includeLog ? "yes" : "no"}`,
        never_included: ["Saves", "Account details"],
      });
    },
    saveDiagnostics(includeLog) {
      record("saveDiagnostics", includeLog);
      return Promise.resolve(true);
    },
    showDiagnosticsInFolder: () => done("showDiagnosticsInFolder"),
    copyLauncherLog() {
      record("copyLauncherLog");
      return Promise.resolve("[1760000000] launcher started by <you>\n");
    },
    openLogsFolder: () => done("openLogsFolder"),
    getStartWithWindows() {
      record("getStartWithWindows");
      return Promise.resolve(false);
    },
    setStartWithWindows: (enabled) => done("setStartWithWindows", enabled),

    minimizeWindow: () => done("minimizeWindow"),
    closeWindow: () => done("closeWindow"),
    startWindowDrag: () => done("startWindowDrag"),
    quitLauncher() {
      record("quitLauncher");
      if (state.running) return refuse("operation_in_progress", "An operation is running.");
      return Promise.resolve();
    },

    getNews(id) {
      record("getNews", id);
      return Promise.resolve(structuredClone(s.news[id] ?? { status: "ok", items: [] }));
    },
    openNewsLink: (id, index) => done("openNewsLink", id, index),
    openProductPage: (id) => done("openProductPage", id),
    openCommunityPage: () => done("openCommunityPage"),

    onStateChanged: (h) => subscribe(changed, h),
    onProgress(h) {
      const un = subscribe(progressed, h);
      if (state.running && script.length) {
        script.slice(0, step + 1).forEach(h);
        if (!timer) run();
      }
      return un;
    },
    onOutcome: (h) => subscribe(outcomes, h),
    onTrayRequest: (h) => subscribe(trays, h),
    picked: null,
  };
  return api;
}
