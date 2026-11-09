import type { LauncherApi } from "./types";

/** The calls a player's click waits on (issue 138); `?slow=` delays these to preview busy states.
 * Not recoverProduct: startup awaits it before any state shows. */
export const SLOW_CALLS = [
  "inspectOperation",
  "startOperation",
  "cancelOperation",
  "playProduct",
  "refreshCatalog",
  "openFolder",
  "openLoaderDownloadPage",
] as const satisfies readonly (keyof LauncherApi)[];

/** Dev fixtures only: answers each click-driven call `ms` later, so pending states stay visible. */
export function withDelay<T extends LauncherApi>(api: T, ms: number): T {
  const slow = new Set<PropertyKey>(SLOW_CALLS);
  return new Proxy(api, {
    get(target, prop, receiver) {
      const value: unknown = Reflect.get(target, prop, receiver);
      if (!slow.has(prop) || typeof value !== "function") return value;
      return (...args: unknown[]) =>
        new Promise((r) => setTimeout(r, ms)).then(() => (value as (...a: unknown[]) => unknown).apply(target, args));
    },
  });
}
