import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

export interface ToastAction {
  label: string;
  onSelect: () => void;
}

export interface ToastInput {
  tone: "info" | "error";
  text: string;
  actions?: ToastAction[];
  /** Information toasts auto-dismiss unless this is false. Errors always stay until dismissed. */
  autoDismiss?: boolean;
}

interface Toast extends ToastInput {
  id: number;
}

interface ToastApi {
  show(toast: ToastInput): number;
  dismiss(id: number): void;
}

const ToastContext = createContext<ToastApi | null>(null);

export const TOAST_MS = 6000;

export function useToast(): ToastApi {
  const api = useContext(ToastContext);
  if (!api) throw new Error("useToast outside ToastProvider");
  return api;
}

/** Bottom-right toasts. Information is muted and auto-dismisses; errors are accent and stay. */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const next = useRef(1);
  const dismiss = useCallback((id: number) => setToasts((t) => t.filter((x) => x.id !== id)), []);
  const show = useCallback((toast: ToastInput) => {
    const id = next.current++;
    setToasts((t) => [...t, { ...toast, id }]);
    return id;
  }, []);
  const api = useMemo(() => ({ show, dismiss }), [show, dismiss]);
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
  const latest = useRef(dismiss);
  useEffect(() => {
    latest.current = dismiss;
  });
  useEffect(() => {
    if (!auto) return;
    const t = setTimeout(() => latest.current(), TOAST_MS);
    return () => clearTimeout(t);
  }, [auto]);
  return (
    <div className={`toast ${toast.tone}`} role={toast.tone === "error" ? "alert" : "status"}>
      <p>{toast.text}</p>
      <div className="toast-actions">
        {toast.actions?.map((a) => (
          <button
            key={a.label}
            type="button"
            className="link"
            onClick={() => {
              dismiss();
              a.onSelect();
            }}
          >
            {a.label}
          </button>
        ))}
        <button type="button" className="link" onClick={dismiss}>
          Dismiss
        </button>
      </div>
    </div>
  );
}
