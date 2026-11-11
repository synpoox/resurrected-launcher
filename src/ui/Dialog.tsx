import { useId, useRef, type ReactNode } from "react";
import { useFocusTrap } from "./focus";
import { useExitGhost } from "./motion";

export interface DialogProps {
  title: string;
  /** "risk" draws the accent top rule, for destructive or risky actions. */
  tone?: "info" | "risk";
  /** Esc and the backdrop call this; omit it while the dialog must stay open. */
  onClose?: () => void;
  children: ReactNode;
  /** The button row. */
  actions: ReactNode;
}

/** A modal dialog: square, on `--panel`, over a backdrop. Traps focus and returns it to its opener. */
export function Dialog({ title, tone = "info", onClose, children, actions }: DialogProps) {
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();
  useFocusTrap(ref);
  const backdrop = useRef<HTMLDivElement>(null);
  useExitGhost(backdrop);
  return (
    <div ref={backdrop} className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose?.()}>
      <div
        ref={ref}
        className={`dialog ${tone}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        onKeyDown={(e) => {
          if (e.key === "Escape" && onClose) {
            e.stopPropagation();
            onClose();
          }
        }}
      >
        <h2 id={titleId}>{title}</h2>
        {children}
        <div className="dialog-actions">{actions}</div>
      </div>
    </div>
  );
}
