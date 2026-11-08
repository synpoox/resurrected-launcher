// Picks the LauncherApi implementation: live under Tauri, fixtures in tests and a plain browser.
// In dev builds, `?fixture=<key>` selects a fixture scene even under Tauri.
import { createFixtureApi, DEFAULT_SCENE, SCENES, sceneByKey } from "./fixtures";
import { createLiveApi } from "./live";
import type { LauncherApi } from "./types";

export interface SelectedApi extends LauncherApi {
  dispose(): void;
}

/** `key` is the fixture scene, or null for the live adapter. */
export type Selection = { ok: true; api: SelectedApi; key: string | null } | { ok: false; error: string };

export const FIXTURE_KEYS: readonly string[] = SCENES.map((s) => s.key);

export function selectApi(search: string, dev: boolean, live = false): Selection {
  const asked = dev ? new URLSearchParams(search).get("fixture") : null;
  if (asked === null && live) {
    // News stays on fixtures until #80.
    const news = createFixtureApi(DEFAULT_SCENE);
    return { ok: true, api: { ...createLiveApi(news), dispose: () => news.dispose() }, key: null };
  }
  const key = asked ?? DEFAULT_SCENE;
  if (!sceneByKey(key)) {
    return { ok: false, error: `Unknown fixture "${key}". Known fixtures: ${FIXTURE_KEYS.join(", ")}.` };
  }
  return { ok: true, api: createFixtureApi(key), key };
}

export type { LauncherApi } from "./types";
