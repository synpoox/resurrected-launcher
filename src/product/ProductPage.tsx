import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { useApi } from "../api/context";
import { subscribe } from "../api/subscribe";
import type { SceneOpen } from "../api/fixtures";
import type {
  Inspection,
  NewsState,
  OperationKind,
  OperationRequest,
  OutcomeReport,
  ProductState,
  ReleaseNotes,
  TrayRequest,
} from "../api/types";
import type { Product } from "../products";
import { arrowKeys } from "../ui/focus";
import { Menu, type MenuItem } from "../ui/Menu";
import { Notice } from "../ui/Notice";
import { BUSY_SAFETY_MS, BusyButton, usePending } from "../ui/pending";
import { useToast } from "../ui/Toast";
import { CleanInstallDialog, LocationDialog, SaveBreakingDialog, UninstallDialog } from "./dialogs";
import { formatBytes, formatCount, formatTimeLeft, releaseLabel, startedAgo } from "./format";
import { HeroArt, HeroCopy } from "./Hero";
import { existingOf, hasNotice, journeyOf, type Journey } from "./journey";
import { NewsFeed } from "./NewsFeed";
import { PatchNotes } from "./PatchNotes";
import { Progress } from "./Progress";
import { progressStats } from "./stats";
import { asError, type ProductData } from "./store";

export type Tab = "overview" | "notes";

/** A tray request for this product: the save-breaking dialog, or the Overview with its notice. */
export interface TrayFocus {
  product_id: string;
  action: Exclude<TrayRequest["action"], "quit_while_busy">;
  /** Increments per request, so the same request twice acts twice. */
  seq: number;
}

/** localStorage: the installed release whose Patch Notes were last opened automatically. */
export const seenKey = (productId: string) => `launcher.patchNotesSeen.${productId}`;

type DialogState =
  | { kind: "locate" }
  | { kind: "save-breaking"; inspection: Inspection }
  // Issue 138: consent dialogs open on the click, with a null inspection until it answers.
  // `started`: Uninstall was accepted and its outcome hasn't arrived, so the confirm stays busy.
  | { kind: "uninstall"; inspection: Inspection | null; report: OutcomeReport | null; started?: boolean }
  | { kind: "clean-install"; inspection: Inspection | null }
  | null;

export interface ProductPageProps {
  product: Product;
  data: ProductData | undefined;
  refresh: () => Promise<void>;
  /** A dialog, the menu or an operation to open once state has loaded (fixture scenes). */
  open?: SceneOpen;
  /** Overrides the default tab (dev fixtures). */
  tab?: Tab;
  trayFocus?: TrayFocus;
  onOpenSettings: () => void;
  /** Clears a product's clean-install backup notice; with `only`, just if it still shows those. */
  onDismissBackups?: (productId: string, only?: string[]) => void;
}

export function ProductPage(props: ProductPageProps) {
  return props.product.hasRelease ? <ReleasedProduct {...props} /> : <ComingSoon product={props.product} />;
}

/** Shows the latest request's reply only: Rust resolves `openNewsLink` indexes against the
 *  latest-started `getNews` too, so an index means the same item on both sides. */
function useNews(productId: string) {
  const api = useApi();
  const toast = useToast();
  const [news, setNews] = useState<NewsState | null>(null);
  const latest = useRef(0);
  const load = useCallback(() => {
    const request = ++latest.current;
    const show = (s: NewsState) => {
      if (request === latest.current) setNews(s);
    };
    api.getNews(productId).then(show, () => show({ status: "offline" }));
  }, [api, productId]);
  useEffect(load, [load]);
  return {
    news,
    retry: load,
    open: (i: number) =>
      void api.openNewsLink(productId, i).catch((e: unknown) => {
        toast.show({ tone: "error", text: asError(e).message });
        load();
      }),
  };
}

/** Forever and 09 today: ribbon, description and one outlined link; no tabs. "About <product>"
 *  opens the product's public page when it has one; otherwise "Follow for updates on Discord"
 *  opens the project Discord (issue 135). */
function ComingSoon({ product }: { product: Product }) {
  const api = useApi();
  const toast = useToast();
  const fail = (e: unknown) => toast.show({ tone: "error", text: asError(e).message });
  const about = () => void api.openProductPage(product.id).catch(fail);
  const follow = () => void api.openCommunityPage().catch(fail);
  const feed = useNews(product.id);
  return (
    <div className="page overview">
      <HeroArt product={product} />
      <HeroCopy product={product} />
      <div className="bottom">
        <div className="action-col">
          <div className="act">
            {product.aboutPage ? (
              <button type="button" className="btn ghost" onClick={about}>
                About {product.short}
              </button>
            ) : (
              <button type="button" className="btn ghost" onClick={follow}>
                Follow for updates on Discord <span aria-hidden="true">↗</span>
              </button>
            )}
          </div>
        </div>
        <NewsFeed news={feed.news} onOpen={feed.open} onRetry={feed.retry} />
      </div>
    </div>
  );
}

function ReleasedProduct(props: ProductPageProps) {
  const { product, data, refresh } = props;
  const state = data?.state ?? null;
  if (!state) {
    return (
      <div className="page overview">
        <HeroArt product={product} />
        <HeroCopy product={product} />
        {data?.error && (
          <div className="bottom">
            <div className="action-col">
              <Notice tone="error">
                <b>{product.short} couldn't be read.</b> {data.error.message}{" "}
                <button type="button" className="link" onClick={() => void refresh()}>
                  Try again
                </button>
              </Notice>
            </div>
          </div>
        )}
      </div>
    );
  }
  return <Loaded {...props} state={state} />;
}

/** Opens Patch Notes when the installed release changed since the last launch (stored in localStorage). */
function initialTab(productId: string, installedId: string | undefined): Tab {
  const seen = localStorage.getItem(seenKey(productId));
  return installedId && seen !== null && seen !== installedId ? "notes" : "overview";
}

/** Failure toasts from each product's outcomes, per toast host: they close when a newer
 *  operation on that product starts (issue 156). Kept outside the page, so they survive leaving it. */
const outcomeToasts = new WeakMap<object, Map<string, number[]>>();
function outcomeToastsOf(host: object, productId: string): number[] {
  let byProduct = outcomeToasts.get(host);
  if (!byProduct) outcomeToasts.set(host, (byProduct = new Map()));
  let ids = byProduct.get(productId);
  if (!ids) byProduct.set(productId, (ids = []));
  return ids;
}

function Loaded({ product, data, refresh, open, tab: forcedTab, trayFocus, onOpenSettings, onDismissBackups, state }: ProductPageProps & { state: ProductState }) {
  const api = useApi();
  const toast = useToast();
  const feed = useNews(product.id);
  const installedId = state.installed?.release_id;
  const [tab, setTab] = useState<Tab>(() => forcedTab ?? initialTab(product.id, installedId));
  const [releases, setReleases] = useState<ReleaseNotes[] | null>(null);
  const [dialog, setDialog] = useState<DialogState>(() => (open === "d2r-choose" ? { kind: "locate" } : null));
  const [installInspection, setInstallInspection] = useState<Inspection | null>(null);
  const [repairInspection, setRepairInspection] = useState<Inspection | null>(null);
  const [showFix, setShowFix] = useState(false);
  // Issue 138: one action at a time, busy from the click until its result or dialog arrives.
  // Shared per API and product, so leaving the page mid-request and coming back keeps it busy.
  const pending = usePending({ owner: api, scope: product.id });
  /** Uninstall attempts: bumped by each confirm and by each uninstall outcome, so an outcome that
   *  arrives before startOperation answers isn't overwritten by that attempt's late `started`. */
  const uninstallAttempt = useRef(0);
  const opened = useRef(false);
  const focused = useRef(0);
  const ids = { overview: useId(), notes: useId(), notice: useId(), line: useId(), why: useId() };

  const samples = data?.samples ?? [];
  const shownBackups = data?.backups;
  const backups = shownBackups ?? [];
  const last = samples.at(-1)?.event ?? null;
  const journey: Journey = journeyOf(state, last);
  const stats = progressStats(samples);

  const fail = useCallback((e: unknown) => toast.show({ tone: "error", text: asError(e).message }), [toast]);

  const loadNotes = useCallback(() => {
    api.getReleaseNotes(product.id).then(setReleases, () => setReleases([]));
  }, [api, product.id]);
  const recommendedId = state.recommended?.release_id;
  useEffect(loadNotes, [loadNotes, installedId, recommendedId]);

  // The first launch after an update opens Patch Notes once, on the new release (see initialTab).
  useEffect(() => {
    if (installedId) localStorage.setItem(seenKey(product.id), installedId);
  }, [installedId, product.id]);

  // Install shows the space it needs, from a read-only inspection.
  const needsInstallInspection = journey === "install";
  useEffect(() => {
    if (!needsInstallInspection) return;
    let live = true;
    api.inspectOperation({ product_id: product.id, kind: "install" }).then(
      (i) => live && setInstallInspection(i),
      () => live && setInstallInspection(null),
    );
    return () => {
      live = false;
    };
  }, [api, product.id, needsInstallInspection]);

  // Outcomes: failures toast; an uninstall's report fills its dialog.
  useEffect(() => {
    return subscribe(
      api.onOutcome((o) => {
        if (o.product_id !== product.id) return;
        if (o.status === "failed") outcomeToastsOf(toast, product.id).push(fail(o.error));
        if (o.kind === "uninstall") {
          uninstallAttempt.current++;
          // The report fills the dialog; a failed or cancelled uninstall frees its confirm again.
          const report = o.status === "succeeded" ? o.report : null;
          setDialog((d) => (d?.kind === "uninstall" ? { ...d, report, started: false } : d));
        }
        void refresh();
      }),
    ).stop;
  }, [api, product.id, fail, refresh, toast]);

  /** Starts an operation and re-reads state (which then shows it running). True if it started. */
  const start = useCallback(
    async (kind: OperationKind, inspection: Inspection, consent: Partial<OperationRequest> = {}) => {
      // Toasts from earlier outcomes only: this operation's own outcome can arrive before start answers.
      const shown = outcomeToastsOf(toast, product.id);
      const stale = [...shown];
      try {
        await api.startOperation({ product_id: product.id, kind, ...consent }, inspection);
      } catch (e) {
        fail(e);
        return false;
      }
      // A newer operation started: the last one's outcome toasts and notice are stale.
      shown.splice(0, stale.length);
      stale.forEach(toast.dismiss);
      if (shownBackups?.length) onDismissBackups?.(product.id, shownBackups);
      await refresh().catch(() => undefined);
      return true;
    },
    [api, product.id, refresh, fail, toast, onDismissBackups, shownBackups],
  );

  /** Opens a consent dialog at once, then fills it when its inspection answers (issue 138). It
   *  doesn't hold the pending slot: the modal dialog blocks the page, and Not now frees it at once.
   *  The answer fills only this dialog object, so a late one never fills a closed or newer dialog. */
  const openConsent = useCallback(
    (kind: "uninstall" | "clean_install") => {
      const opened: DialogState =
        kind === "uninstall" ? { kind: "uninstall", inspection: null, report: null } : { kind: "clean-install", inspection: null };
      setDialog(opened);
      api.inspectOperation({ product_id: product.id, kind }).then(
        (inspection) => setDialog((d) => (d === opened ? { ...opened, inspection } : d)),
        (e: unknown) => {
          setDialog((d) => (d === opened ? null : d));
          fail(e);
        },
      );
    },
    [api, product.id, fail],
  );

  /** Inspects, then starts the operation or opens its dialog. Doesn't take the pending slot: the
   *  caller holds it (a `busy` button, or `runAction`). */
  const operate = useCallback(
    async (kind: OperationKind) => {
      if (kind === "uninstall" || kind === "clean_install") return openConsent(kind);
      let inspection: Inspection;
      try {
        inspection = await api.inspectOperation({ product_id: product.id, kind });
      } catch (e) {
        fail(e);
        return;
      }
      if (kind === "update" && inspection.release.save_breaking) {
        setDialog({ kind: "save-breaking", inspection });
        return;
      }
      if (kind === "repair") setRepairInspection(inspection);
      await start(kind, inspection);
    },
    [api, product.id, start, fail, openConsent],
  );
  /** `operate` in the pending slot: for the menu, tray and fixture paths, which have no busy button. */
  const runAction = useCallback((kind: OperationKind) => pending.run(kind, () => operate(kind)), [pending, operate]);

  const play = async () => {
    try {
      await api.playProduct(product.id);
      await refresh();
    } catch (e) {
      fail(e);
    }
  };
  const cancel = () => api.cancelOperation(product.id).then(refresh, fail);
  const checkAgain = async () => {
    try {
      if (state.readiness.status === "recovery_required") await api.recoverProduct(product.id);
      await refresh();
    } catch (e) {
      fail(e);
    }
  };
  const tryCatalog = () => api.refreshCatalog().then(refresh, fail);

  // Fixture scenes open a dialog, the menu or an operation once state has loaded.
  useEffect(() => {
    if (opened.current || !open) return;
    opened.current = true;
    const kinds: Partial<Record<SceneOpen, OperationKind>> = {
      "save-breaking": "update",
      uninstall: "uninstall",
      repair: "repair",
      "clean-install": "clean_install",
    };
    const kind = kinds[open];
    // The same path the button takes; it only sets state after the API answers.
    if (kind) void Promise.resolve(kind).then(runAction);
  }, [open, runAction]);

  // Tray requests: the save-breaking dialog (through the Update path), or the Overview's notice.
  useEffect(() => {
    if (!trayFocus || focused.current === trayFocus.seq) return;
    focused.current = trayFocus.seq;
    const action = trayFocus.action;
    void Promise.resolve().then(() => {
      setTab("overview");
      if (action === "save_breaking") return runAction("update");
    });
  }, [trayFocus, runAction]);

  const label = (releaseId: string, version: string) => releaseLabel(releaseId, version, releases ?? []);
  const installed = state.installed;
  const recommended = state.recommended;
  const offered = recommended ? label(recommended.release_id, recommended.display_version) : null;

  // While the game runs the menu stays, and what needs it closed says why (issue 156).
  const quitFirst = journey === "running" ? `Quit ${product.short} first` : undefined;
  const manageItems: MenuItem[] = [
    { label: "Repair", disabledReason: quitFirst, onSelect: () => void runAction("repair") },
    { label: "Open saves folder", onSelect: () => void pending.run("saves", () => api.openFolder(product.id, "saves").catch(fail)) },
    {
      label: "Open settings folder",
      onSelect: () => void pending.run("settings", () => api.openFolder(product.id, "settings").catch(fail)),
    },
    {
      label: "Uninstall…",
      tone: "danger",
      divider: true,
      disabledReason: quitFirst,
      onSelect: () => void runAction("uninstall"),
    },
  ];
  const manage = (
    <Menu label={`Manage ${product.short}`} items={manageItems} defaultOpen={open === "manage"} blocked={pending.key !== null} />
  );
  const line = (dot: string, text: ReactNode) => (
    <span id={ids.line} className="ready-line">
      <span className={`dot ${dot}`} aria-hidden="true" />
      {text}
    </span>
  );
  const primary = (text: string, onClick?: () => void, describedBy?: string) => (
    <button
      type="button"
      className="btn primary"
      disabled={!onClick}
      aria-describedby={onClick ? undefined : describedBy}
      onClick={onClick}
    >
      {text}
    </button>
  );
  const ghost = (text: string, onClick: () => void) => (
    <button type="button" className="btn ghost" onClick={onClick}>
      {text}
    </button>
  );
  /** An action button in the pending group: a spinner while it runs, and no second submit. */
  const busy = (tone: "primary" | "ghost", text: string, key: string, fn: () => Promise<unknown>) => (
    <BusyButton
      className={tone}
      busy={pending.key === key}
      blocked={pending.key !== null}
      onClick={() => void pending.run(key, fn)}
    >
      {text}
    </BusyButton>
  );
  const cancelProps = { busy: pending.key === "cancel", onCancel: () => void pending.run("cancel", cancel) };

  const percent = stats.percent;
  const sizeDetail = () => {
    const parts: string[] = [];
    if (stats.bytesDone !== null && stats.bytesTotal !== null) {
      parts.push(`${formatBytes(stats.bytesDone)} of ${formatBytes(stats.bytesTotal)}`);
    }
    if (stats.speed !== null) parts.push(`${formatBytes(stats.speed)}/s`);
    if (stats.secondsLeft !== null) parts.push(formatTimeLeft(stats.secondsLeft));
    return parts.join(" · ") || "Starting…";
  };
  const opVerb = state.running === "update" ? "Updating to" : "Downloading";

  /** The action row; `compact` is the Patch Notes top-left version (main action only). */
  const actions = (compact: boolean): ReactNode => {
    switch (journey) {
      case "d2r-missing":
        return (
          <>
            {primary("Locate D2R", () => setDialog({ kind: "locate" }))}
            {!compact && line("warn", "Diablo II: Resurrected wasn't found on this PC")}
          </>
        );
      case "loader-missing":
        return (
          <>
            {busy("primary", "Get D2RLoader", "loader", () => api.openLoaderDownloadPage().catch(fail))}
            {!compact && busy("ghost", "Check again", "check", checkAgain)}
          </>
        );
      case "install": {
        // issue 126: the install inspection also says whether the product folder already holds a release.
        const existing = existingOf(installInspection);
        if (existing === "adoptable") {
          return (
            <>
              {busy("primary", "Adopt", "adopt", () => operate("adopt"))}
              {!compact && line("ok", `${offered} is already in your game folder · Adopt uses it without downloading`)}
            </>
          );
        }
        if (existing === "different") {
          return (
            <>
              {busy("primary", "Clean install…", "clean_install", () => operate("clean_install"))}
              {!compact &&
                line("warn", `The ${product.short} in your game folder isn't ${offered} · older or changed files`)}
            </>
          );
        }
        const missing = existing === "incomplete" && installInspection ? installInspection.files_added : 0;
        return (
          <>
            {busy("primary", "Install", "install", () => operate("install"))}
            {!compact &&
              line(
                "idle",
                [
                  offered,
                  missing > 0 &&
                    `${formatCount(missing)} ${missing === 1 ? "file" : "files"} missing from the ${product.short} in your game folder`,
                  installInspection && `${formatBytes(installInspection.space_needed)} free space needed`,
                ]
                  .filter(Boolean)
                  .join(" · "),
              )}
          </>
        );
      }
      case "downloading":
        return (
          <Progress
            compact={compact}
            label={`${opVerb} ${offered ?? product.short}`}
            percent={percent}
            detail={sizeDetail()}
            cancelLabel="Cancel"
            {...cancelProps}
          />
        );
      case "applying":
        return (
          <Progress
            compact={compact}
            label={state.running === "uninstall" ? "Removing files" : "Installing files"}
            percent={percent}
            detail="Cancelling now waits until it's safe to stop, or rolls back"
            cancelLabel="Cancel safely"
            {...cancelProps}
          />
        );
      case "cancelling":
        return (
          <Progress
            compact={compact}
            striped
            label="Stopping safely…"
            percent={percent}
            detail="Restoring the previous files. Don't close the launcher."
            cancelLabel="Cancel"
          />
        );
      case "repair": {
        const parts: string[] = [];
        if (stats.filesDone !== null && stats.filesTotal !== null) {
          parts.push(`Checked ${formatCount(stats.filesDone)} of ${formatCount(stats.filesTotal)} files`);
        }
        if (repairInspection) parts.push(`${formatCount(repairInspection.files_replaced_or_removed)} to restore`);
        parts.push("saves and edited filters are kept");
        return (
          <Progress compact={compact} label="Repairing" percent={percent} detail={parts.join(" · ")} cancelLabel="Cancel" {...cancelProps} />
        );
      }
      case "ready":
      case "offline": {
        const ver = installed ? label(installed.release_id, installed.display_version) : "";
        const latest = installed && recommended?.release_id === installed.release_id;
        return (
          <>
            {busy("primary", "Play", "play", play)}
            {!compact && manage}
            {!compact &&
              (journey === "offline"
                ? line("idle", `Offline · ${ver}${installed?.verified ? " verified" : ""} · can't check for updates`)
                : line("ok", `Ready · ${ver}${latest ? " (latest)" : ""}`))}
          </>
        );
      }
      case "update":
        return (
          <>
            {busy("primary", "Update", "update", () => operate("update"))}
            {!compact && installed && busy("ghost", `Play ${installed.display_version}`, "play", play)}
            {!compact && manage}
            {!compact && offered && line("new", `${offered} available`)}
          </>
        );
      case "blocked":
        return (
          <>
            {primary("Play", undefined, compact ? ids.why : ids.notice)}
            {compact && (
              <span id={ids.why} className="why">
                Can't play yet. Overview says why.
              </span>
            )}
            {!compact && busy("ghost", "Check again", "check", checkAgain)}
            {!compact && manage}
          </>
        );
      case "running":
        return (
          <>
            <button type="button" className="btn primary playing" disabled aria-describedby={compact ? undefined : ids.line}>
              <span className="pulse" aria-hidden="true" />
              Playing
            </button>
            {!compact && manage}
            {!compact &&
              state.readiness.status === "running" &&
              line("none", `Started ${startedAgo(state.readiness.since)} · Repair and updates wait until you quit`)}
          </>
        );
      case "repair-needed":
        return (
          <>
            {busy("primary", "Repair", "repair", () => operate("repair"))}
            {!compact && installed && busy("ghost", `Play ${installed.display_version}`, "play", play)}
            {!compact && manage}
          </>
        );
      case "startup-failed":
        return (
          <>
            {busy("primary", "Try again", "play", play)}
            {!compact && busy("ghost", "Repair", "repair", () => operate("repair"))}
            {!compact && ghost("Export diagnostics", onOpenSettings)}
          </>
        );
      case "offline-empty":
        return (
          <>
            {busy("primary", "Try again", "catalog", tryCatalog)}
            {!compact && line("warn", "Can't reach the release catalog")}
          </>
        );
    }
  };

  const notice = (): ReactNode => {
    const r = state.readiness;
    switch (journey) {
      case "d2r-missing":
        return (
          <Notice>
            Choose the folder that contains <code>D2R.exe</code>. The launcher won't move or copy any files.
          </Notice>
        );
      case "loader-missing": {
        const g = state.loader_guidance;
        const found = g?.found ?? (r.status === "loader_unsupported" ? r.found : null);
        const allowed = g?.allowed ?? (r.status === "loader_unsupported" ? r.allowed : []);
        if (r.status === "loader_modified") {
          return (
            <Notice>
              <b>
                {r.file} doesn't match the official D2RLoader {r.version} download
              </b>
              , so it may be damaged or changed. Reinstall D2RLoader from the official download page, then choose Check
              again. The launcher never downloads or runs the installer.
            </Notice>
          );
        }
        return (
          <Notice>
            {found ? (
              <>
                <b>D2RLoader {found} was found</b>, but this release doesn't allow it.
              </>
            ) : (
              <>
                <b>{product.short} needs D2RLoader</b>, and it wasn't found.
              </>
            )}{" "}
            {allowed.length > 0 && <>This release allows {allowed.join(", ")}. </>}
            Get it from the official download page, install it, then choose Check again. The launcher never downloads or
            runs the installer.
          </Notice>
        );
      }
      case "blocked":
        return (
          <div id={ids.notice}>
            <Notice tone="error">
              {r.status === "build_unsupported" ? (
                <>
                  <b>Can't play:</b> D2R build {r.found} isn't supported by {offered ?? "this release"} yet. Wait for a{" "}
                  {product.short} update, or restore a supported D2R build.
                </>
              ) : (
                <>
                  <b>Can't play:</b> the last change to {product.short}'s files didn't finish. Check again restores a
                  complete installation. Saves aren't touched.
                </>
              )}{" "}
              <button type="button" className="link" aria-expanded={showFix} onClick={() => setShowFix((s) => !s)}>
                How to fix
              </button>
              {showFix && (
                <ol className="fix">
                  <li>Quit Diablo II: Resurrected if it's open.</li>
                  <li>
                    {r.status === "build_unsupported"
                      ? "Check for a launcher or PD2R update, or restore the supported D2R build."
                      : "Choose Check again. If it keeps failing, use Repair from the ⋯ menu."}
                  </li>
                  <li>Choose Check again.</li>
                </ol>
              )}
            </Notice>
          </div>
        );
      case "repair-needed":
        return (
          <Notice tone="error">
            <b>Repair didn't finish</b>, so some of {product.short}'s files may still be missing or damaged. Repair
            restores them and keeps your saves and edited filters.
          </Notice>
        );
      case "startup-failed": {
        const code = state.startup_failed?.exit_code;
        return (
          <Notice tone="error">
            <b>The game closed while starting</b>
            {code !== null && code !== undefined ? ` (exit code ${code})` : ""}. Try again. If it keeps happening, Repair
            restores {product.short}'s files and keeps your saves and edited filters.
          </Notice>
        );
      }
      default:
        return null;
    }
  };

  const reading = tab === "notes";
  const tabs = (
    <div
      className="tabs"
      role="tablist"
      aria-label={`${product.short} pages`}
      onKeyDown={arrowKeys("[role=tab]", "horizontal", true)}
    >
      {(["overview", "notes"] as const).map((t) => (
        <button
          key={t}
          type="button"
          role="tab"
          id={`${ids[t]}-tab`}
          aria-selected={tab === t}
          aria-controls={ids[t]}
          tabIndex={tab === t ? 0 : -1}
          onClick={() => setTab(t)}
        >
          {t === "overview" ? "Overview" : "Patch Notes"}
          {t === "overview" && reading && hasNotice(journey) && (
            <span className="attention" role="img" aria-label="needs attention" />
          )}
        </button>
      ))}
    </div>
  );

  const closeDialog = () => setDialog(null);
  return (
    <div className={`page ${reading ? "reading" : "overview"}`}>
      {tabs}
      {reading ? (
        <div className="tabpanel" role="tabpanel" id={ids.notes} aria-labelledby={`${ids.notes}-tab`}>
          <div className="topact">
            <div className="act">{actions(true)}</div>
          </div>
          <PatchNotes product={product} releases={releases} onRetry={loadNotes} />
          <div className="fade" aria-hidden="true" />
        </div>
      ) : (
        <div className="tabpanel" role="tabpanel" id={ids.overview} aria-labelledby={`${ids.overview}-tab`}>
          <HeroArt product={product} />
          <HeroCopy product={product} />
          <div className="bottom">
            <div className="action-col">
              {backups.length > 0 && (
                <Notice>
                  <b>Clean install finished.</b> Your previous files were backed up.{" "}
                  <button
                    type="button"
                    className="link"
                    onClick={() => void api.openFolder(product.id, "clean_install_backup").catch(fail)}
                  >
                    Open backup
                  </button>{" "}
                  <button type="button" className="link" onClick={() => onDismissBackups?.(product.id)}>
                    Dismiss
                  </button>
                </Notice>
              )}
              {notice()}
              <div className="act">{actions(false)}</div>
            </div>
            <NewsFeed news={feed.news} onOpen={feed.open} onRetry={feed.retry} />
          </div>
        </div>
      )}
      {dialog?.kind === "locate" && (
        <LocationDialog
          product={product}
          onClose={(chosen) => {
            closeDialog();
            if (chosen) void refresh();
          }}
        />
      )}
      {dialog?.kind === "save-breaking" && (
        <SaveBreakingDialog
          product={product}
          inspection={dialog.inspection}
          installedVersion={installed?.display_version ?? null}
          onClose={closeDialog}
          onConfirm={() => {
            closeDialog();
            void pending.run("update", () => start("update", dialog.inspection, { acknowledge_save_breaking: true }));
          }}
        />
      )}
      {dialog?.kind === "uninstall" && (
        <UninstallDialog
          product={product}
          inspection={dialog.inspection}
          report={dialog.report}
          busy={pending.key === "uninstall-confirm" || dialog.started === true}
          onClose={closeDialog}
          onConfirm={(inspection) =>
            void pending.run("uninstall-confirm", async () => {
              const attempt = ++uninstallAttempt.current;
              if (!(await start("uninstall", inspection))) return;
              // The outcome already arrived (a fast failure): nothing is left to wait for.
              if (attempt !== uninstallAttempt.current) return;
              // Busy until the outcome: its report (or a failure) usually arrives after start answers.
              const mark = (started: boolean) =>
                setDialog((d) => (d?.kind === "uninstall" && d.inspection === inspection && !d.report ? { ...d, started } : d));
              mark(true);
              setTimeout(() => attempt === uninstallAttempt.current && mark(false), BUSY_SAFETY_MS);
            })
          }
        />
      )}
      {dialog?.kind === "clean-install" && (
        <CleanInstallDialog
          product={product}
          inspection={dialog.inspection}
          label={label}
          onClose={closeDialog}
          onConfirm={(inspection) => {
            closeDialog();
            void pending.run("clean_install", () => start("clean_install", inspection, { consent_clean_install: true }));
          }}
        />
      )}
    </div>
  );
}
