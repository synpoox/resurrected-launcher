import { MOTION, motionAllowed } from "./motion";
import { createContext, useCallback, useContext, useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";

export interface ToastAction {
  label: string;
  onSelect: () => void;
  /**
   * Shows the action disabled (`aria-disabled`, still focusable, so focus isn't lost when an
   * update() disables it); the toast's text says why.
   */
  disabled?: boolean;
}

export interface ToastInput {
  tone: "info" | "error";
  text: string;
  actions?: ToastAction[];
  /** Information toasts auto-dismiss unless this is false. Errors always stay until dismissed. */
  autoDismiss?: boolean;
  /** The dismiss button's label (default "Dismiss"). */
  dismissLabel?: string;
  /**
   * Called when the toast closes without an action: `"player"` for the dismiss button, `"timeout"`
   * for auto-dismiss. Choosing an action or a programmatic dismiss() doesn't call it.
   */
  onDismiss?: (by: "player" | "timeout") => void;
}

interface Toast extends ToastInput {
  id: number;
  /** Playing its exit animation (issue 156); no longer interactive. */
  leaving?: boolean;
}

interface ToastApi {
  show(toast: ToastInput): number;
  /**
   * Replaces an open toast's content in place. Does nothing if it has closed. The auto-dismiss
   * timer is not restarted unless the update changes whether the toast auto-dismisses.
   */
  update(id: number, toast: ToastInput): void;
  dismiss(id: number): void;
}

const ToastContext = createContext<ToastApi | null>(null);

export const TOAST_MS = 6000;
/** The exit animation's length (`.toast.leaving` in ui.css). */
export const TOAST_EXIT_MS = MOTION.base;

export function useToast(): ToastApi {
  const api = useContext(ToastContext);
  if (!api) throw new Error("useToast outside ToastProvider");
  return api;
}

/** Bottom-right toasts. Information is muted and auto-dismisses; errors are accent and stay. */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const next = useRef(1);
  const remove = useCallback((id: number) => setToasts((t) => t.filter((x) => x.id !== id)), []);
  const dismiss = useCallback(
    (id: number) => {
      if (!motionAllowed()) return remove(id);
      setToasts((t) => t.map((x) => (x.id === id ? { ...x, leaving: true } : x)));
      setTimeout(() => remove(id), TOAST_EXIT_MS);
    },
    [remove],
  );
  const show = useCallback((toast: ToastInput) => {
    const id = next.current++;
    setToasts((t) => [...t, { ...toast, id }]);
    return id;
  }, []);
  const update = useCallback(
    (id: number, toast: ToastInput) => setToasts((t) => t.map((x) => (x.id === id && !x.leaving ? { ...toast, id } : x))),
    [],
  );
  const api = useMemo(() => ({ show, update, dismiss }), [show, update, dismiss]);
  return (
    <ToastContext.Provider value={api}>
      {children}
      <div className="toasts" role="region" aria-label="Notifications">
        {toasts.map((t) => (
          <ToastView key={t.id} toast={t} dismiss={() => dismiss(t.id)} />
        ))}
      </div>
    </ToastContext.Provider>
  );
}

function ToastView({ toast, dismiss }: { toast: Toast; dismiss: () => void }) {
  const auto = toast.tone === "info" && toast.autoDismiss !== false;
  const text = useId();
  const latest = useRef({ dismiss, onDismiss: toast.onDismiss });
  useEffect(() => {
    latest.current = { dismiss, onDismiss: toast.onDismiss };
  });
  useEffect(() => {
    if (!auto || toast.leaving) return;
    const t = setTimeout(() => {
      latest.current.dismiss();
      latest.current.onDismiss?.("timeout");
    }, TOAST_MS);
    return () => clearTimeout(t);
  }, [auto, toast.leaving]);
  return (
    <div
      className={`toast ${toast.tone}${toast.leaving ? " leaving" : ""}`}
      role={toast.tone === "error" ? "alert" : "status"}
      // Closing: out of the tab order and the accessibility tree while it fades.
      inert={toast.leaving || undefined}
      aria-hidden={toast.leaving || undefined}
    >
      <p id={text}>{toast.text}</p>
      <div className="toast-actions">
        {toast.actions?.map((a, i) => (
          <button
            // By position, so an update() that relabels an action keeps its button (and focus).
            key={i}
            type="button"
            className="link"
            aria-disabled={a.disabled || undefined}
            aria-describedby={a.disabled ? text : undefined}
            onClick={() => {
              if (a.disabled) return;
              dismiss();
              a.onSelect();
            }}
          >
            {a.label}
          </button>
        ))}
        <button
          type="button"
          className="link"
          onClick={() => {
            dismiss();
            toast.onDismiss?.("player");
          }}
        >
          {toast.dismissLabel ?? "Dismiss"}
        </button>
      </div>
    </div>
  );
}
