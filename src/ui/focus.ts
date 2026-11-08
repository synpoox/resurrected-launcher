import { useEffect, type KeyboardEvent, type RefObject } from "react";

const TABBABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** Elements inside `root` reachable with Tab, in DOM order. */
export function tabbables(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(TABBABLE)].filter((el) => !el.closest("[inert]") && !el.hidden);
}

/**
 * While mounted, keeps Tab inside `ref` and focuses its `[data-autofocus]` element (or the first
 * tabbable). On unmount, returns focus to whatever was focused before: the control that opened it.
 */
export function useFocusTrap(ref: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const root = ref.current;
    if (!root) return;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const first = root.querySelector<HTMLElement>("[data-autofocus]") ?? tabbables(root)[0] ?? root;
    first.focus();
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const items = tabbables(root);
      if (items.length === 0) {
        e.preventDefault();
        return;
      }
      const i = items.indexOf(document.activeElement as HTMLElement);
      const next = e.shiftKey ? (i <= 0 ? items.length - 1 : i - 1) : i === items.length - 1 || i < 0 ? 0 : i + 1;
      e.preventDefault();
      items[next].focus();
    };
    root.addEventListener("keydown", onKey);
    return () => {
      root.removeEventListener("keydown", onKey);
      if (opener?.isConnected) opener.focus();
    };
  }, [ref]);
}

/** `both` takes ↑ ← as previous and ↓ → as next, for a vertical list people also expect ← → on. */
export type Orientation = "vertical" | "horizontal" | "both";

/**
 * Keydown handler that moves focus between `selector` matches inside the current target with the
 * arrow keys (and Home/End). Wraps at the ends. With `activate`, the newly focused item is clicked
 * too, as tabs do.
 */
export function arrowKeys(selector: string, orientation: Orientation, activate = false) {
  return (e: KeyboardEvent<HTMLElement>) => {
    const prev = orientation === "vertical" ? ["ArrowUp"] : orientation === "horizontal" ? ["ArrowLeft"] : ["ArrowUp", "ArrowLeft"];
    const next = orientation === "vertical" ? ["ArrowDown"] : orientation === "horizontal" ? ["ArrowRight"] : ["ArrowDown", "ArrowRight"];
    if (![...prev, ...next, "Home", "End"].includes(e.key)) return;
    const items = [...e.currentTarget.querySelectorAll<HTMLElement>(selector)].filter(
      (el) => !(el as HTMLButtonElement).disabled,
    );
    if (items.length === 0) return;
    const i = items.indexOf(document.activeElement as HTMLElement);
    let n: number;
    if (e.key === "Home") n = 0;
    else if (e.key === "End") n = items.length - 1;
    else if (next.includes(e.key)) n = (i + 1) % items.length;
    else n = (i - 1 + items.length) % items.length;
    e.preventDefault();
    e.stopPropagation();
    items[n].focus();
    if (activate) items[n].click();
  };
}
