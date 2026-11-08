import { useCallback, useRef, useState } from "react";

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

/**
 * Props for a button that takes part in a pending group: busy (`aria-busy`) when it is the action
 * in progress, and `aria-disabled` (still focusable, so focus isn't lost) while any action is.
 */
export function pendingProps(pending: string | null, key: string) {
  return {
    "aria-busy": pending === key || undefined,
    "aria-disabled": pending !== null || undefined,
  } as const;
}
