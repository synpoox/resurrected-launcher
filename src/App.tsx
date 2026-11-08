import { useEffect, useMemo, useState } from "react";
import { ApiContext } from "./api/context";
import { FIXTURE_KEYS, selectApi } from "./api/index";
import { sceneByKey } from "./api/fixtures";
import { Shell } from "./shell/Shell";
import { ToastProvider } from "./ui/Toast";

export interface AppProps {
  search: string;
  /** Dev builds allow `?fixture=<key>` (plus `&tab=notes`, and `&chrome=0` to hide the bar) and ← → cycling. */
  dev: boolean;
}

/** Arrow keys cycle fixtures only when nothing focusable claims them. */
function claimsArrows(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    Boolean(target.closest("input, textarea, select, [role=tablist], [role=menu], [role=dialog], .products"))
  );
}

export function App({ search, dev }: AppProps) {
  const [query, setQuery] = useState(search);
  const selection = useMemo(() => selectApi(query, dev), [query, dev]);
  useEffect(() => () => void (selection.ok && selection.api.dispose()), [selection]);

  useEffect(() => {
    if (!dev || !selection.ok) return;
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
  const scene = sceneByKey(selection.key)!;
  const params = new URLSearchParams(query);
  const tab = dev && params.get("tab") === "notes" ? "notes" : undefined;
  return (
    <ApiContext.Provider value={selection.api}>
      <ToastProvider>
        <Shell key={selection.key} product={scene.product} open={scene.open} tab={tab} />
      </ToastProvider>
      {dev && params.get("chrome") !== "0" && (
        <p className="fixture-bar" aria-live="polite">
          Fixture: {selection.key} · ← →
        </p>
      )}
    </ApiContext.Provider>
  );
}
