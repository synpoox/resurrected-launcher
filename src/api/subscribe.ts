import type { Unlisten } from "./types";

export interface Subscription {
  /** Resolves once every subscription is in place. */
  ready: Promise<void>;
  /** Stops every subscription, including one that resolves after this is called. Use as an effect's cleanup. */
  stop: () => void;
}

/** Holds `LauncherApi.on*` subscriptions for an effect's lifetime. */
export function subscribe(...subs: Promise<Unlisten>[]): Subscription {
  let live = true;
  const stops: Unlisten[] = [];
  const ready = Promise.all(subs.map((s) => s.then((un) => (live ? void stops.push(un) : un())))).then(() => undefined);
  return {
    ready,
    stop: () => {
      live = false;
      stops.splice(0).forEach((un) => un());
    },
  };
}
