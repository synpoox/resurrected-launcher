import { useEffect, useMemo, useState } from "react";
import { ApiContext } from "./api/context";
import { FIXTURE_KEYS, selectApi } from "./api/index";
import { sceneByKey } from "./api/fixtures";
import { isTauri } from "./ipc";
import { selectSelfUpdate } from "./selfupdate/flag";
import { SelfUpdateProvider } from "./selfupdate/SelfUpdate";
import { Shell } from "./shell/Shell";
import { ToastProvider } from "./ui/Toast";

export interface AppProps {
  search: string;
  /** Dev builds allow `?fixture=<key>` (plus `&tab=notes`, and `&chrome=0` to hide the bar) and ← → cycling. */
  dev: boolean;
  /** Use the live adapter (default: running under Tauri). `?fixture=` in dev still wins. */
  live?: boolean;
  /** The launcher self-update flag (`VITE_SELF_UPDATE=1`, read in main.tsx); dev builds then allow `?selfupdate=<phase>`. Off by default. */
  selfUpdate?: boolean;
}

/** Arrow keys cycle fixtures only when nothing focusable claims them. */
function claimsArrows(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    Boolean(target.closest("input, textarea, select, [role=tablist], [role=menu], [role=dialog], .products"))
  );
}

export function App({ search, dev, live = isTauri(), selfUpdate = false }: AppProps) {
  const [query, setQuery] = useState(search);
  const selection = useMemo(() => selectApi(query, dev, live), [query, dev, live]);
  useEffect(() => () => void (selection.ok && selection.api.dispose()), [selection]);
  // Keyed on its own parameter, so ← → through product fixtures keeps the self-update phase.
  const selfUpdateKey = dev ? new URLSearchParams(query).get("selfupdate") : null;
  const update = useMemo(() => selectSelfUpdate(selfUpdateKey, selfUpdate, live), [selfUpdateKey, selfUpdate, live]);
  useEffect(() => () => void (update?.ok && update.api.dispose()), [update]);

  useEffect(() => {
    if (!dev || !selection.ok || selection.mode !== "fixture") return;
    const key = selection.key;
    const onKey = (e: KeyboardEvent) => {
      if ((e.key !== "ArrowLeft" && e.key !== "ArrowRight") || e.defaultPrevented || claimsArrows(e.target)) return;
      const i = FIXTURE_KEYS.indexOf(key);
      const n = (i + (e.key === "ArrowRight" ? 1 : -1) + FIXTURE_KEYS.length) % FIXTURE_KEYS.length;
      const next = `?fixture=${FIXTURE_KEYS[n]}${selfUpdateKey ? `&selfupdate=${encodeURIComponent(selfUpdateKey)}` : ""}`;
      history.replaceState(null, "", next);
      setQuery(next);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dev, selection, selfUpdateKey]);

  const error = !selection.ok ? selection.error : update?.ok === false ? update.error : null;
  // `!selection.ok` is implied by the error but narrows `selection` for the code below.
  if (error !== null || !selection.ok) {
    return (
      <div className="fixture-error" role="alert">
        <h1>Fixture error</h1>
        <p>{error}</p>
      </div>
    );
  }
  const fixture = selection.mode === "fixture" ? selection.key : null;
  const scene = fixture === null ? undefined : sceneByKey(fixture);
  const params = new URLSearchParams(query);
  const tab = dev && params.get("tab") === "notes" ? "notes" : undefined;
  const shell = (
    <Shell
      key={fixture ?? "live"}
      product={scene?.product}
      open={scene?.open}
      tab={tab}
    />
  );
  return (
    <ApiContext.Provider value={selection.api}>
      <ToastProvider>
        {update?.ok ? <SelfUpdateProvider api={update.api}>{shell}</SelfUpdateProvider> : shell}
      </ToastProvider>
      {dev && fixture !== null && params.get("chrome") !== "0" && (
        <p className="fixture-bar" aria-live="polite">
          Fixture: {fixture}
          {update?.ok ? ` · launcher ${update.key}` : ""} · ← →
        </p>
      )}
    </ApiContext.Provider>
  );
}
