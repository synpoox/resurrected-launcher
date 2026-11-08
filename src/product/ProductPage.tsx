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
  | { kind: "uninstall"; inspection: Inspection; report: OutcomeReport | null }
  | { kind: "clean-install"; inspection: Inspection }
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
  /** Clears this product's clean-install backup notice. */
  onDismissBackups?: () => void;
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

/** Forever and 09 today: ribbon, description and, when it has a page, "About <product>"; no tabs. */
function ComingSoon({ product }: { product: Product }) {
  const api = useApi();
  const toast = useToast();
  const about = () =>
    void api.openProductPage(product.id).catch((e: unknown) => toast.show({ tone: "error", text: asError(e).message }));
  const feed = useNews(product.id);
  return (
    <div className="page overview">
      <HeroArt product={product} />
      <HeroCopy product={product} />
      <div className="bottom">
        <div className="action-col">
          {product.aboutPage && (
            <div className="act">
              <button type="button" className="btn ghost" onClick={about}>
                About {product.short}
              </button>
            </div>
          )}
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
  const opened = useRef(false);
  const focused = useRef(0);
  const ids = { overview: useId(), notes: useId(), notice: useId(), line: useId(), why: useId() };

  const samples = data?.samples ?? [];
  const backups = data?.backups ?? [];
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
        if (o.status === "failed") fail(o.error);
        if (o.status === "succeeded" && o.kind === "uninstall") {
          setDialog((d) => (d?.kind === "uninstall" ? { ...d, report: o.report } : d));
        }
        void refresh();
      }),
    ).stop;
  }, [api, product.id, fail, refresh]);

  const start = useCallback(
    async (kind: OperationKind, inspection: Inspection, consent: Partial<OperationRequest> = {}) => {
      try {
        await api.startOperation({ product_id: product.id, kind, ...consent }, inspection);
        await refresh();
      } catch (e) {
        fail(e);
      }
    },
    [api, product.id, refresh, fail],
  );

  const run = useCallback(
    async (kind: OperationKind) => {
      try {
        const inspection = await api.inspectOperation({ product_id: product.id, kind });
        if (kind === "update" && inspection.release.save_breaking) {
          setDialog({ kind: "save-breaking", inspection });
          return;
        }
        if (kind === "uninstall") {
          setDialog({ kind: "uninstall", inspection, report: null });
          return;
        }
        if (kind === "clean_install") {
          setDialog({ kind: "clean-install", inspection });
          return;
        }
        if (kind === "repair") setRepairInspection(inspection);
        await start(kind, inspection);
      } catch (e) {
        fail(e);
      }
    },
    [api, product.id, start, fail],
  );

  const play = async () => {
    try {
      await api.playProduct(product.id);
      await refresh();
    } catch (e) {
      fail(e);
    }
  };
  const cancel = () => void api.cancelOperation(product.id).then(refresh, fail);
  const checkAgain = async () => {
    try {
      if (state.readiness.status === "recovery_required") await api.recoverProduct(product.id);
      await refresh();
    } catch (e) {
      fail(e);
    }
  };
  const tryCatalog = () => void api.refreshCatalog().then(refresh, fail);

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
    if (kind) void Promise.resolve(kind).then(run);
  }, [open, run]);

  // Tray requests: the save-breaking dialog (through the Update path), or the Overview's notice.
  useEffect(() => {
    if (!trayFocus || focused.current === trayFocus.seq) return;
    focused.current = trayFocus.seq;
    const action = trayFocus.action;
    void Promise.resolve().then(() => {
      setTab("overview");
      if (action === "save_breaking") return run("update");
    });
  }, [trayFocus, run]);

  const label = (releaseId: string, version: string) => releaseLabel(releaseId, version, releases ?? []);
  const installed = state.installed;
  const recommended = state.recommended;
  const offered = recommended ? label(recommended.release_id, recommended.display_version) : null;

  const manageItems: MenuItem[] = [
    { label: "Repair", onSelect: () => void run("repair") },
    { label: "Open saves folder", onSelect: () => void api.openFolder(product.id, "saves").catch(fail) },
    { label: "Open settings folder", onSelect: () => void api.openFolder(product.id, "settings").catch(fail) },
    { label: "Uninstall…", tone: "danger", divider: true, onSelect: () => void run("uninstall") },
  ];
  const manage = <Menu label={`Manage ${product.short}`} items={manageItems} defaultOpen={open === "manage"} />;
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
            {primary("Get D2RLoader", () => void api.openLoaderDownloadPage().catch(fail))}
            {!compact && ghost("Check again", () => void checkAgain())}
          </>
        );
      case "install": {
        // issue 126: the install inspection also says whether the product folder already holds a release.
        const existing = existingOf(installInspection);
        if (existing === "adoptable") {
          return (
            <>
              {primary("Adopt", () => void run("adopt"))}
              {!compact && line("ok", `${offered} is already in your game folder · Adopt uses it without downloading`)}
            </>
          );
        }
        if (existing === "different") {
          return (
            <>
              {primary("Clean install…", () => void run("clean_install"))}
              {!compact &&
                line("warn", `The ${product.short} in your game folder isn't ${offered} · older or changed files`)}
            </>
          );
        }
        const missing = existing === "incomplete" && installInspection ? installInspection.files_added : 0;
        return (
          <>
            {primary("Install", () => void run("install"))}
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
            onCancel={cancel}
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
            onCancel={cancel}
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
          <Progress compact={compact} label="Repairing" percent={percent} detail={parts.join(" · ")} cancelLabel="Cancel" onCancel={cancel} />
        );
      }
      case "ready":
      case "offline": {
        const ver = installed ? label(installed.release_id, installed.display_version) : "";
        const latest = installed && recommended?.release_id === installed.release_id;
        return (
          <>
            {primary("Play", () => void play())}
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
            {primary("Update", () => void run("update"))}
            {!compact && installed && ghost(`Play ${installed.display_version}`, () => void play())}
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
            {!compact && ghost("Check again", () => void checkAgain())}
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
            {!compact &&
              state.readiness.status === "running" &&
              line("none", `Started ${startedAgo(state.readiness.since)} · Repair and updates wait until you quit`)}
          </>
        );
      case "startup-failed":
        return (
          <>
            {primary("Try again", () => void play())}
            {!compact && ghost("Repair", () => void run("repair"))}
            {!compact && ghost("Export diagnostics", onOpenSettings)}
          </>
        );
      case "offline-empty":
        return (
          <>
            {primary("Try again", tryCatalog)}
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
                  <b>Clean install finished.</b> Your previous files were moved to{" "}
                  {backups.map((p, i) => (
                    <span key={p}>
                      {i > 0 && ", "}
                      <code>{p}</code>
                    </span>
                  ))}
                  .{" "}
                  <button type="button" className="link" onClick={onDismissBackups}>
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
            void start("update", dialog.inspection, { acknowledge_save_breaking: true });
          }}
        />
      )}
      {dialog?.kind === "uninstall" && (
        <UninstallDialog
          product={product}
          inspection={dialog.inspection}
          report={dialog.report}
          onClose={closeDialog}
          onConfirm={() => void start("uninstall", dialog.inspection)}
        />
      )}
      {dialog?.kind === "clean-install" && (
        <CleanInstallDialog
          product={product}
          inspection={dialog.inspection}
          onClose={closeDialog}
          onConfirm={() => {
            closeDialog();
            void start("clean_install", dialog.inspection, { consent_clean_install: true });
          }}
        />
      )}
    </div>
  );
}
