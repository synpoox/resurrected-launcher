import { useEffect, useMemo, useState } from "react";
import { ApiContext } from "./api/context";
import { FIXTURE_KEYS, selectApi } from "./api/index";
import { sceneByKey } from "./api/fixtures";
import { isTauri } from "./ipc";
import { Shell } from "./shell/Shell";
import { ToastProvider } from "./ui/Toast";

export interface AppProps {
  search: string;
  /** Dev builds allow `?fixture=<key>` (plus `&tab=notes`, and `&chrome=0` to hide the bar) and ← → cycling. */
  dev: boolean;
  /** Use the live adapter (default: running under Tauri). `?fixture=` in dev still wins. */
  live?: boolean;
}

/** Arrow keys cycle fixtures only when nothing focusable claims them. */
function claimsArrows(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    Boolean(target.closest("input, textarea, select, [role=tablist], [role=menu], [role=dialog], .products"))
  );
}

export function App({ search, dev, live = isTauri() }: AppProps) {
  const [query, setQuery] = useState(search);
  const selection = useMemo(() => selectApi(query, dev, live), [query, dev, live]);
  useEffect(() => () => void (selection.ok && selection.api.dispose()), [selection]);

  useEffect(() => {
    if (!dev || !selection.ok || selection.key === null) return;
    const key = selection.key;
    const onKey = (e: KeyboardEvent) => {
      if ((e.key !== "ArrowLeft" && e.key !== "ArrowRight") || e.defaultPrevented || claimsArrows(e.target)) return;
      const i = FIXTURE_KEYS.indexOf(key);
      const n = (i + (e.key === "ArrowRight" ? 1 : -1) + FIXTURE_KEYS.length) % FIXTURE_KEYS.length;
      const next = `?fixture=${FIXTURE_KEYS[n]}`;
      history.replaceState(null, "", next);
      setQuery(next);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dev, selection]);

  if (!selection.ok) {
    return (
      <div className="fixture-error" role="alert">
        <h1>Fixture error</h1>
        <p>{selection.error}</p>
      </div>
    );
  }
  const scene = selection.key === null ? undefined : sceneByKey(selection.key);
  const params = new URLSearchParams(query);
  const tab = dev && params.get("tab") === "notes" ? "notes" : undefined;
  return (
    <ApiContext.Provider value={selection.api}>
      <ToastProvider>
        <Shell key={selection.key ?? "live"} product={scene?.product} open={scene?.open} tab={tab} />
      </ToastProvider>
      {dev && selection.key !== null && params.get("chrome") !== "0" && (
        <p className="fixture-bar" aria-live="polite">
          Fixture: {selection.key} · ← →
        </p>
      )}
    </ApiContext.Provider>
  );
}
