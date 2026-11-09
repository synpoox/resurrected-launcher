import { useCallback, useMemo, useState, useSyncExternalStore, type ReactNode } from "react";

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

/** A pending slot that components subscribe to. */
class Slot {
  key: string | null = null;
  private listeners = new Set<() => void>();
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => void this.listeners.delete(listener);
  };
  get = () => this.key;
  set(key: string | null) {
    this.key = key;
    this.listeners.forEach((l) => l());
  }
}

/** Shared slots, per owner (the LauncherApi) and scope (a product id), so they outlive a page. */
const slots = new WeakMap<object, Map<string, Slot>>();

function slotFor(owner: object, scope: string): Slot {
  let scopes = slots.get(owner);
  if (!scopes) slots.set(owner, (scopes = new Map()));
  let slot = scopes.get(scope);
  if (!slot) scopes.set(scope, (slot = new Slot()));
  return slot;
}

/**
 * One action at a time: every action button is busy-disabled from its click until its result.
 * With `shared`, the slot belongs to that owner and scope instead of this component, so leaving a
 * product page and coming back mid-request keeps its buttons busy (issue 138).
 */
export function usePending(shared?: { owner: object; scope: string }): Pending {
  const [local] = useState(() => new Slot());
  const slot = shared ? slotFor(shared.owner, shared.scope) : local;
  const key = useSyncExternalStore(slot.subscribe, slot.get);
  const run = useCallback(
    async (k: string, fn: () => Promise<unknown> | void) => {
      if (slot.get() !== null) return;
      slot.set(k);
      try {
        await fn();
      } finally {
        slot.set(null);
      }
    },
    [slot],
  );
  return useMemo(() => ({ key, run }), [key, run]);
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
