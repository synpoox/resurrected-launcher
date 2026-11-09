// Picks the LauncherApi implementation: live under Tauri, fixtures in tests and a plain browser.
// In dev builds, `?fixture=<key>` selects a fixture scene even under Tauri.
import { createFixtureApi, DEFAULT_SCENE, SCENES, sceneByKey } from "./fixtures";
import { createLiveApi } from "./live";
import { withDelay } from "./slow";
import type { LauncherApi } from "./types";

export interface SelectedApi extends LauncherApi {
  dispose(): void;
}

export type Selection =
  | { ok: true; mode: "live"; api: SelectedApi }
  | { ok: true; mode: "fixture"; api: SelectedApi; key: string }
  | { ok: false; error: string };

export const FIXTURE_KEYS: readonly string[] = SCENES.map((s) => s.key);

export function selectApi(search: string, dev: boolean, live = false): Selection {
  const asked = dev ? new URLSearchParams(search).get("fixture") : null;
  if (asked === null && live) {
    return { ok: true, mode: "live", api: { ...createLiveApi(), dispose: () => {} } };
  }
  const key = asked ?? DEFAULT_SCENE;
  if (!sceneByKey(key)) {
    return { ok: false, error: `Unknown fixture "${key}". Known fixtures: ${FIXTURE_KEYS.join(", ")}.` };
  }
  // `&slow=<ms>` delays click-driven calls, to preview busy states (issue 138).
  const slow = dev ? Number(new URLSearchParams(search).get("slow")) : 0;
  const api = createFixtureApi(key);
  return { ok: true, mode: "fixture", api: slow > 0 ? withDelay(api, slow) : api, key };
}

export type { LauncherApi } from "./types";
