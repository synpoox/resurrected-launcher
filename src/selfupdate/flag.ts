// The self-update notice ships behind a build flag until slice 5 brings the real updater (ADR 0003).
// Off, nothing self-update-related renders or runs.
import { createSelfUpdateFixture, DEFAULT_SELF_UPDATE_FIXTURE, SELF_UPDATE_FIXTURES, selfUpdateFixtureByKey, type SelfUpdateFixtureApi } from "./api";

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
  | { ok: true; api: SelfUpdateFixtureApi; key: string }
  | { ok: false; error: string }
  | null;

/** Null when the flag is off. In dev builds, `?selfupdate=<key>` picks the fixture phase. */
export function selectSelfUpdate(search: string, dev: boolean, enabled: boolean): SelfUpdateSelection {
  if (!enabled) return null;
  const key = (dev ? new URLSearchParams(search).get("selfupdate") : null) ?? DEFAULT_SELF_UPDATE_FIXTURE;
  if (!selfUpdateFixtureByKey(key)) {
    const known = SELF_UPDATE_FIXTURES.map((f) => f.key).join(", ");
    return { ok: false, error: `Unknown self-update fixture "${key}". Known fixtures: ${known}.` };
  }
  return { ok: true, api: createSelfUpdateFixture(key), key };
}
