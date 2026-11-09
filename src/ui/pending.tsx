import { useCallback, useRef, useState, type ReactNode } from "react";

/** A small square spinner for a busy button or a loading dialog body. Static under reduced motion. */
export function Spinner() {
  return <span className="spinner" aria-hidden="true" />;
}

export interface Pending {
  /** The key of the action in progress, or null. */
  key: string | null;
  /**
   * Runs `fn` as the action `key` unless an action is already in progress (issue 138: no
   * double-submit). The key stays set from the click until `fn` settles.
   */
  run: (key: string, fn: () => Promise<unknown> | void) => Promise<void>;
}

/** One action at a time: every action button is busy-disabled from its click until its result. */
export function usePending(): Pending {
  const [key, setKey] = useState<string | null>(null);
  const current = useRef<string | null>(null);
  const run = useCallback(async (k: string, fn: () => Promise<unknown> | void) => {
    if (current.current !== null) return;
    current.current = k;
    setKey(k);
    try {
      await fn();
    } finally {
      current.current = null;
      setKey(null);
    }
  }, []);
  return { key, run };
}

export interface BusyButtonProps {
  /** Extra classes after `btn`, e.g. "primary" or "ghost small". */
  className: string;
  /** This button's action is running: a spinner and `aria-busy`. */
  busy?: boolean;
  /** Another action (or this one) is running, or it can't run yet: `aria-disabled`. */
  blocked?: boolean;
  onClick: () => void;
  children: ReactNode;
  "aria-describedby"?: string;
}

/**
 * Issue 138: a button that is busy from its click until its result. Busy and blocked use
 * `aria-disabled`, not `disabled`, so focus stays on the button; clicks are ignored meanwhile.
 */
export function BusyButton({ className, busy = false, blocked = false, onClick, children, ...rest }: BusyButtonProps) {
  const off = busy || blocked;
  return (
    <button
      type="button"
      className={`btn ${className}`}
      aria-busy={busy || undefined}
      aria-disabled={off || undefined}
      aria-describedby={rest["aria-describedby"]}
      onClick={() => !off && onClick()}
    >
      {busy && <Spinner />}
      {children}
    </button>
  );
}
