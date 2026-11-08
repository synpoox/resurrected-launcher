// The launcher's own update (spec: Launcher self-update, variant C; ADR 0003). Slice 4 runs on the
// fixtures below only. Slice 5 implements SelfUpdateApi with the signed Tauri updater.
import type { Unlisten } from "../api/types";

/** Why Restart to update waits: a product is running or downloading. It still installs on the next Quit. */
export interface RestartHold {
  reason: "product_running" | "product_downloading";
  product_id: string;
}

/** Where the launcher's own update is. `current` is the running launcher's version. */
export type SelfUpdateStatus =
  | { phase: "up_to_date"; current: string }
  | { phase: "available"; current: string; next: string }
  | { phase: "downloading"; current: string; next: string; percent: number }
  | { phase: "ready"; current: string; next: string; hold: RestartHold | null }
  | { phase: "restarting"; current: string; next: string }
  /** After the restart that installed it: `current` is the new version. */
  | { phase: "updated"; current: string }
  | { phase: "download_failed"; current: string; next: string }
  /** The downloaded update failed its signature check; it was deleted and nothing was installed. */
  | { phase: "signature_rejected"; current: string; next: string };

export type SelfUpdatePhase = SelfUpdateStatus["phase"];

export interface SelfUpdateApi {
  getStatus(): Promise<SelfUpdateStatus>;
  /** Checks now. The result arrives as a phase event (available, downloading, or nothing new). */
  checkForUpdates(): Promise<void>;
  /** Starts (or retries) downloading the update the last check found. */
  download(): Promise<void>;
  /** Installs the ready update and restarts the launcher. Refused while a restart hold applies. */
  restartToUpdate(): Promise<void>;
  /** Opens the launcher's release notes outside the launcher. The native side owns the address. */
  openWhatsNew(): Promise<void>;
  getAutoDownload(): Promise<boolean>;
  setAutoDownload(enabled: boolean): Promise<void>;
  onPhase(handler: (status: SelfUpdateStatus) => void): Promise<Unlisten>;
}

// ---- Fixtures ----

export const FIXTURE_CURRENT = "1.2.0";
export const FIXTURE_NEXT = "1.3.0";

export interface SelfUpdateFixture {
  key: string;
  name: string;
  status: SelfUpdateStatus;
  autoDownload: boolean;
}

const v = { current: FIXTURE_CURRENT, next: FIXTURE_NEXT };

/** One fixture per phase. In dev, `?selfupdate=<key>` selects one. */
export const SELF_UPDATE_FIXTURES: readonly SelfUpdateFixture[] = [
  { key: "up-to-date", name: "Up to date", status: { phase: "up_to_date", current: FIXTURE_CURRENT }, autoDownload: true },
  { key: "available", name: "Available (automatic downloads off)", status: { phase: "available", ...v }, autoDownload: false },
  { key: "downloading", name: "Downloading", status: { phase: "downloading", ...v, percent: 38 }, autoDownload: true },
  { key: "ready", name: "Ready", status: { phase: "ready", ...v, hold: null }, autoDownload: true },
  {
    key: "held-running",
    name: "Ready, held: product running",
    status: { phase: "ready", ...v, hold: { reason: "product_running", product_id: "pd2r" } },
    autoDownload: true,
  },
  {
    key: "held-downloading",
    name: "Ready, held: product downloading",
    status: { phase: "ready", ...v, hold: { reason: "product_downloading", product_id: "pd2r" } },
    autoDownload: true,
  },
  { key: "restarting", name: "Restarting", status: { phase: "restarting", ...v }, autoDownload: true },
  { key: "updated", name: "Updated", status: { phase: "updated", current: FIXTURE_NEXT }, autoDownload: true },
  { key: "download-failed", name: "Download failed", status: { phase: "download_failed", ...v }, autoDownload: true },
  { key: "signature-rejected", name: "Signature rejected", status: { phase: "signature_rejected", ...v }, autoDownload: true },
];

export const DEFAULT_SELF_UPDATE_FIXTURE = "up-to-date";

export const selfUpdateFixtureByKey = (key: string) => SELF_UPDATE_FIXTURES.find((f) => f.key === key);

export interface SelfUpdateCall {
  name: keyof SelfUpdateApi;
  args: unknown[];
}

export interface SelfUpdateFixtureApi extends SelfUpdateApi {
  fixture: SelfUpdateFixture;
  calls: SelfUpdateCall[];
  /** Sends a phase event to every subscriber (tests). */
  emit(status: SelfUpdateStatus): void;
  /** Stops pending timers. */
  dispose(): void;
}

export interface SelfUpdateFixtureOptions {
  /** Milliseconds between simulated steps (default 1000). */
  tick?: number;
}

const DOWNLOAD_STEPS = [12, 38, 64, 90];

/**
 * A fresh fixture for one phase. Check finds 1.3.0 unless it is already installed, Download walks
 * the percentages to ready, and Restart to update goes through restarting to updated.
 */
export function createSelfUpdateFixture(key: string, options: SelfUpdateFixtureOptions = {}): SelfUpdateFixtureApi {
  const found = selfUpdateFixtureByKey(key);
  if (!found) throw new Error(`Unknown self-update fixture "${key}"`);
  const fixture = structuredClone(found);
  const tick = options.tick ?? 1000;
  const calls: SelfUpdateCall[] = [];
  const handlers = new Set<(s: SelfUpdateStatus) => void>();
  const timers = new Set<ReturnType<typeof setTimeout>>();
  let status = fixture.status;
  let auto = fixture.autoDownload;

  const record = (name: keyof SelfUpdateApi, ...args: unknown[]) => calls.push({ name, args });
  const emit = (s: SelfUpdateStatus) => {
    status = s;
    handlers.forEach((h) => h(structuredClone(s)));
  };
  const later = (ms: number, fn: () => void) => {
    const t = setTimeout(() => {
      timers.delete(t);
      fn();
    }, ms);
    timers.add(t);
  };
  const clear = () => {
    timers.forEach(clearTimeout);
    timers.clear();
  };
  const startDownload = (next: string) => {
    clear();
    const current = status.current;
    DOWNLOAD_STEPS.forEach((percent, i) => later(i * tick, () => emit({ phase: "downloading", current, next, percent })));
    later(DOWNLOAD_STEPS.length * tick, () => emit({ phase: "ready", current, next, hold: null }));
  };
  const refuse = (message: string) => Promise.reject(new Error(message));

  return {
    fixture,
    calls,
    emit,
    dispose: clear,
    getStatus() {
      record("getStatus");
      return Promise.resolve(structuredClone(status));
    },
    checkForUpdates() {
      record("checkForUpdates");
      return new Promise((resolve) =>
        later(tick, () => {
          const idle = status.phase === "up_to_date" || status.phase === "updated";
          if (idle && status.current !== FIXTURE_NEXT) {
            if (auto) startDownload(FIXTURE_NEXT);
            else emit({ phase: "available", current: status.current, next: FIXTURE_NEXT });
          }
          resolve();
        }),
      );
    },
    download() {
      record("download");
      const s = status;
      if (s.phase !== "available" && s.phase !== "download_failed" && s.phase !== "signature_rejected") {
        return refuse(`Nothing to download in phase ${s.phase}`);
      }
      startDownload(s.next);
      return Promise.resolve();
    },
    restartToUpdate() {
      record("restartToUpdate");
      const s = status;
      if (s.phase !== "ready") return refuse(`Not ready: ${s.phase}`);
      if (s.hold) return refuse("Restart is held");
      emit({ phase: "restarting", current: s.current, next: s.next });
      later(tick * 2, () => emit({ phase: "updated", current: s.next }));
      return Promise.resolve();
    },
    openWhatsNew() {
      record("openWhatsNew");
      return Promise.resolve();
    },
    getAutoDownload() {
      record("getAutoDownload");
      return Promise.resolve(auto);
    },
    setAutoDownload(enabled) {
      record("setAutoDownload", enabled);
      auto = enabled;
      return Promise.resolve();
    },
    onPhase(handler) {
      handlers.add(handler);
      return Promise.resolve(() => void handlers.delete(handler));
    },
  };
}
