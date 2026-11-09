// The self-update notice ships behind a build flag (ADR 0003). Off, nothing self-update-related
// renders or runs, and the launcher never checks for updates. The signed release and gate builds
// turn it on (release.yml; #157, after the slice 5 gate #89 passed); dev builds leave it off.
import { createSelfUpdateFixture, DEFAULT_SELF_UPDATE_FIXTURE, SELF_UPDATE_FIXTURES, selfUpdateFixtureByKey, type SelfUpdateApi } from "./api";
import { createLiveSelfUpdate } from "./live";

declare global {
  interface ImportMetaEnv {
    /** "1" turns the launcher self-update notice on. */
    readonly VITE_SELF_UPDATE?: string;
  }
}

/** Read on each call so tests can stub the environment. */
export function selfUpdateEnabled(): boolean {
  return import.meta.env.VITE_SELF_UPDATE === "1";
}

export type SelfUpdateSelection =
  /** `key` is the fixture's, or "live" for the signed Tauri updater. */
  | { ok: true; api: SelfUpdateApi & { dispose(): void }; key: string }
  | { ok: false; error: string }
  | null;

/**
 * Null when the flag is off. `asked` is the dev `?selfupdate=<key>` value (null outside dev), which
 * picks a fixture even under Tauri; otherwise `live` (running under Tauri) uses the real updater.
 */
export function selectSelfUpdate(asked: string | null, enabled: boolean, live = false): SelfUpdateSelection {
  if (!enabled) return null;
  if (asked === null && live) return { ok: true, api: createLiveSelfUpdate(), key: "live" };
  const key = asked ?? DEFAULT_SELF_UPDATE_FIXTURE;
  if (!selfUpdateFixtureByKey(key)) {
    const known = SELF_UPDATE_FIXTURES.map((f) => f.key).join(", ");
    return { ok: false, error: `Unknown self-update fixture "${key}". Known fixtures: ${known}.` };
  }
  return { ok: true, api: createSelfUpdateFixture(key), key };
}
