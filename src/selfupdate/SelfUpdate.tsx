// Variant C (#28): a toast when something needs the player, then a dot on Settings if they choose
// Later. The notice never enters a product's hero, notice or action row, and there is no tray item.
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { DiagnosticsDialog } from "../settings/DiagnosticsDialog";
import { useToast, type ToastInput } from "../ui/Toast";
import type { SelfUpdateApi, SelfUpdateStatus } from "./api";
import { availableText, downloadFailedText, readyText, rejectedText, updatedText } from "./copy";

export interface SelfUpdate {
  /** Null until the first status arrives. */
  status: SelfUpdateStatus | null;
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
  const [status, setStatus] = useState<SelfUpdateStatus | null>(null);
  const [autoDownload, setAuto] = useState<boolean | null>(null);
  const [dot, setDot] = useState(false);
  const [diagnostics, setDiagnostics] = useState(false);
  const open = useRef<{ id: number; key: string } | null>(null);
  const shown = useRef<string | null>(null);

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
  const download = useCallback(() => void api.download().catch(failed), [api, failed]);
  const restart = useCallback(() => void api.restartToUpdate().catch(failed), [api, failed]);
  const whatsNew = useCallback(() => void api.openWhatsNew().catch(() => {}), [api]);

  // Closing a toast without starting the update leaves the dot.
  const later = useCallback(() => setDot(true), []);

  useEffect(() => {
    if (!status) return;
    const key = toastKey(status);
    const input = toastFor(status, { download, restart, whatsNew, later, diagnostics: () => setDiagnostics(true) });
    if (open.current?.key === key) {
      // Same phase and release, new details (a restart hold began or ended).
      if (input) toast.update(open.current.id, input);
      return;
    }
    if (open.current) toast.dismiss(open.current.id);
    open.current = null;
    if (input && shown.current !== key) {
      shown.current = key;
      open.current = { id: toast.show(input), key };
    }
  }, [status, toast, download, restart, whatsNew, later]);

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
    () => ({ status, autoDownload, dot, clearDot, check, download, restart, whatsNew, setAutoDownload }),
    [status, autoDownload, dot, clearDot, check, download, restart, whatsNew, setAutoDownload],
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
  later(): void;
  diagnostics(): void;
}

/** The toast for a status, or null: downloading and restarting are silent, and so is up to date. */
function toastFor(s: SelfUpdateStatus, h: ToastHandlers): ToastInput | null {
  const notes = { label: "What's new", onSelect: () => (h.whatsNew(), h.later()) };
  const pending = { dismissLabel: "Later", onDismiss: h.later };
  switch (s.phase) {
    case "available":
      return { tone: "info", text: availableText(s.next), actions: [{ label: "Download", onSelect: h.download }, notes], ...pending };
    case "ready":
      return {
        tone: "info",
        text: readyText(s.next, s.hold),
        actions: [{ label: "Restart to update", onSelect: h.restart, disabled: Boolean(s.hold) }, notes],
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
        actions: [{ label: "Export diagnostics", onSelect: () => (h.diagnostics(), h.later()) }],
        ...pending,
      };
    default:
      return null;
  }
}
