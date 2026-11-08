// The real SelfUpdateApi (#88): every call passes straight through to `src/ipc.ts`, and the native
// side owns the key, the endpoint and every path. Only built while VITE_SELF_UPDATE is on (flag.ts),
// so with the flag off the launcher never checks for its own updates.
import * as ipc from "../ipc";
import type { Unlisten } from "../api/types";
import type { SelfUpdateApi, SelfUpdateStatus } from "./api";

/** The first background check waits this long after the notice subscribes. */
export const FIRST_CHECK_MS = 5_000;
/** Then it checks again this often while the launcher runs. */
export const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

export interface LiveSelfUpdateApi extends SelfUpdateApi {
  /** Stops background checks and every subscription. */
  dispose(): void;
}

export interface LiveSelfUpdateOptions {
  firstCheck?: number;
  checkEvery?: number;
}

/**
 * Background checks run while anything listens for phases, so a remount (StrictMode, a fixture
 * switch) never leaves a timer behind. A failed background check is silent; Check for updates in
 * Settings reports its own failure.
 */
export function createLiveSelfUpdate(options: LiveSelfUpdateOptions = {}): LiveSelfUpdateApi {
  const firstCheck = options.firstCheck ?? FIRST_CHECK_MS;
  const checkEvery = options.checkEvery ?? CHECK_EVERY_MS;
  const listeners = new Set<Unlisten>();
  let first: ReturnType<typeof setTimeout> | null = null;
  let every: ReturnType<typeof setInterval> | null = null;
  const background = () => void ipc.checkForLauncherUpdate().catch(() => {});
  const start = () => {
    first = setTimeout(background, firstCheck);
    every = setInterval(background, checkEvery);
  };
  const stop = () => {
    if (first !== null) clearTimeout(first);
    if (every !== null) clearInterval(every);
    first = every = null;
  };

  return {
    getStatus: ipc.getLauncherUpdateStatus,
    checkForUpdates: ipc.checkForLauncherUpdate,
    download: ipc.downloadLauncherUpdate,
    restartToUpdate: ipc.restartToUpdateLauncher,
    openWhatsNew: ipc.openLauncherWhatsNew,
    getAutoDownload: ipc.getLauncherAutoDownload,
    setAutoDownload: ipc.setLauncherAutoDownload,
    async onPhase(handler: (status: SelfUpdateStatus) => void) {
      const unlisten = await ipc.onLauncherUpdatePhase(handler);
      let done = false;
      const stopThis = () => {
        if (done) return;
        done = true;
        unlisten();
        listeners.delete(stopThis);
        if (listeners.size === 0) stop();
      };
      if (listeners.size === 0) start();
      listeners.add(stopThis);
      return stopThis;
    },
    dispose() {
      [...listeners].forEach((un) => un());
      stop();
    },
  };
}
