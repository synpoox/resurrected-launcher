// Variant C (#28): a toast when something needs the player, then a dot on Settings if they choose
// Later. The notice never enters a product's hero, notice or action row, and there is no tray item.
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useApi } from "../api/context";
import { useProducts } from "../product/store";
import { DiagnosticsDialog } from "../settings/DiagnosticsDialog";
import { useToast, type ToastInput } from "../ui/Toast";
import type { SelfUpdateApi, SelfUpdateStatus } from "./api";
import { availableText, downloadFailedText, readyText, rejectedText, updatedText } from "./copy";
import { restartHold, type RestartHold } from "./hold";

export interface SelfUpdate {
  /** Null until the first status arrives. */
  status: SelfUpdateStatus | null;
  /** Why Restart to update waits, from product state; null when it can restart now. */
  hold: RestartHold | null;
  /** Null until read. */
  autoDownload: boolean | null;
  /** The red dot on the Settings sidebar item. */
  dot: boolean;
  clearDot(): void;
  check(): Promise<void>;
  download(): void;
  restart(): void;
  whatsNew(): void;
  setAutoDownload(enabled: boolean): void;
}

const SelfUpdateContext = createContext<SelfUpdate | null>(null);

/** The self-update state, or null when the flag is off: every mount point then renders nothing. */
export function useSelfUpdate(): SelfUpdate | null {
  return useContext(SelfUpdateContext);
}

/** Which toast a status gets, so one shows once per phase and release. */
const toastKey = (s: SelfUpdateStatus) => `${s.phase}:${"next" in s ? s.next : s.current}`;

export function SelfUpdateProvider({ api, children }: { api: SelfUpdateApi; children: ReactNode }) {
  const toast = useToast();
  const { products } = useProducts(useApi());
  const [status, setStatus] = useState<SelfUpdateStatus | null>(null);
  const [autoDownload, setAuto] = useState<boolean | null>(null);
  const [dot, setDot] = useState(false);
  const [diagnostics, setDiagnostics] = useState(false);
  /** The self-update toast on screen now, if any. */
  const openToast = useRef<{ id: number; key: string } | null>(null);
  /** The key of the last toast shown, so a re-sent status doesn't show it twice. */
  const lastShownKey = useRef<string | null>(null);

  // Compared by value, so unrelated product changes don't rebuild the toast.
  const computed = restartHold(products);
  const reason = computed?.reason;
  const holder = computed?.product_id;
  const hold = useMemo<RestartHold | null>(
    () => (reason && holder ? { reason, product_id: holder } : null),
    [reason, holder],
  );

  useEffect(() => {
    let live = true;
    const unlisten = api.onPhase((s) => {
      if (!live) return;
      setStatus(s);
      // The update installed: nothing is waiting any more.
      if (s.phase === "updated" || s.phase === "up_to_date") setDot(false);
    });
    api.getStatus().then(
      (s) => live && setStatus((prev) => prev ?? s),
      () => {},
    );
    api.getAutoDownload().then(
      (on) => live && setAuto(on),
      () => live && setAuto(true),
    );
    return () => {
      live = false;
      void unlisten.then((u) => u());
    };
  }, [api]);

  const failed = useCallback(
    () => toast.show({ tone: "error", text: "Couldn't start the launcher update. Try again from Settings → Launcher." }),
    [toast],
  );
  const download = useCallback(() => {
    // A new attempt: if it fails again, its failure toast shows again.
    lastShownKey.current = null;
    void api.download().catch(failed);
  }, [api, failed]);
  const restart = useCallback(() => void api.restartToUpdate().catch(failed), [api, failed]);
  const whatsNew = useCallback(() => void api.openWhatsNew().catch(() => {}), [api]);

  // Only Later leaves the dot (owner decision on PR 105): not What's new, Export diagnostics or a timeout.
  const later = useCallback((by: "player" | "timeout") => {
    if (by === "player") setDot(true);
  }, []);

  useEffect(() => {
    if (!status) return;
    const key = toastKey(status);
    const input = toastFor(status, hold, { download, restart, whatsNew, later, diagnostics: () => setDiagnostics(true) });
    if (openToast.current?.key === key) {
      // Same phase and release, new details (a restart hold began or ended).
      if (input) toast.update(openToast.current.id, input);
      return;
    }
    if (openToast.current) toast.dismiss(openToast.current.id);
    openToast.current = null;
    if (!input) {
      // A silent phase (downloading, restarting) ends the last toast's turn: a later failure shows again.
      lastShownKey.current = null;
      return;
    }
    if (lastShownKey.current !== key) {
      lastShownKey.current = key;
      openToast.current = { id: toast.show(input), key };
    }
  }, [status, hold, toast, download, restart, whatsNew, later]);

  const setAutoDownload = useCallback(
    (on: boolean) => {
      setAuto(on);
      api.setAutoDownload(on).catch(() => {
        setAuto(!on);
        toast.show({ tone: "error", text: "Couldn't change automatic launcher downloads. Try again." });
      });
    },
    [api, toast],
  );

  const check = useCallback(
    () =>
      api.checkForUpdates().catch(() => {
        toast.show({ tone: "error", text: "Couldn't check for launcher updates. Check your connection, then try again." });
      }),
    [api, toast],
  );
  const clearDot = useCallback(() => setDot(false), []);

  const value = useMemo<SelfUpdate>(
    () => ({ status, hold, autoDownload, dot, clearDot, check, download, restart, whatsNew, setAutoDownload }),
    [status, hold, autoDownload, dot, clearDot, check, download, restart, whatsNew, setAutoDownload],
  );
  return (
    <SelfUpdateContext.Provider value={value}>
      {children}
      {diagnostics && <DiagnosticsDialog onClose={() => setDiagnostics(false)} />}
    </SelfUpdateContext.Provider>
  );
}

interface ToastHandlers {
  download(): void;
  restart(): void;
  whatsNew(): void;
  later(by: "player" | "timeout"): void;
  diagnostics(): void;
}

/**
 * The toast for a status, or null: downloading and restarting are silent, and so is up to date.
 * The Settings dot comes only from Later, on available, ready and failure toasts.
 */
function toastFor(s: SelfUpdateStatus, hold: RestartHold | null, h: ToastHandlers): ToastInput | null {
  const notes = { label: "What's new", onSelect: h.whatsNew };
  const pending = { dismissLabel: "Later", onDismiss: h.later };
  switch (s.phase) {
    case "available":
      // Information: auto-dismisses (spec: Toast), unlike ready.
      return { tone: "info", text: availableText(s.next), actions: [{ label: "Download", onSelect: h.download }, notes], ...pending };
    case "ready":
      return {
        tone: "info",
        text: readyText(s.next, hold),
        actions: [{ label: "Restart to update", onSelect: h.restart, disabled: hold !== null }, notes],
        autoDismiss: false,
        ...pending,
      };
    case "updated":
      return { tone: "info", text: updatedText(s.current), actions: [{ label: "What's new", onSelect: h.whatsNew }] };
    case "download_failed":
      return { tone: "error", text: downloadFailedText(s.current), actions: [{ label: "Try again", onSelect: h.download }], ...pending };
    case "signature_rejected":
      return {
        tone: "error",
        text: rejectedText(s.next, s.current),
        actions: [{ label: "Export diagnostics", onSelect: h.diagnostics }],
        ...pending,
      };
    case "up_to_date":
    case "downloading":
    case "restarting":
      return null;
  }
}
