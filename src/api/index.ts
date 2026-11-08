// Picks the LauncherApi implementation. Fixtures only for now; #79 adds the live adapter under
// Tauri. In dev builds, `?fixture=<key>` selects a scene.
import { createFixtureApi, DEFAULT_SCENE, SCENES, sceneByKey, type FixtureApi } from "./fixtures";

export type Selection = { ok: true; api: FixtureApi; key: string } | { ok: false; error: string };

export const FIXTURE_KEYS: readonly string[] = SCENES.map((s) => s.key);

export function selectApi(search: string, dev: boolean): Selection {
  const asked = dev ? new URLSearchParams(search).get("fixture") : null;
  const key = asked ?? DEFAULT_SCENE;
  if (!sceneByKey(key)) {
    return { ok: false, error: `Unknown fixture "${key}". Known fixtures: ${FIXTURE_KEYS.join(", ")}.` };
  }
  return { ok: true, api: createFixtureApi(key), key };
}

export type { LauncherApi } from "./types";
