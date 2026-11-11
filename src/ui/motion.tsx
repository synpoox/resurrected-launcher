import { useEffect, useRef, type ComponentProps, type RefObject } from "react";

/** The motion durations in ms; keep equal to `--dur-*` in tokens.css (motion.test.tsx checks). */
export const MOTION = { fast: 150, base: 200, slow: 300 } as const;

/** False under prefers-reduced-motion (and without matchMedia): nothing animates, so nothing waits. */
export function motionAllowed(): boolean {
  return typeof window.matchMedia === "function" && !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** A ghost never outlives this, whatever the stylesheet says (an exit never blocks anything). */
export const GHOST_MAX_MS = MOTION.slow + 100;

/**
 * Exit animation without keeping anything mounted. When the element in `ref` leaves the DOM (its
 * owner unmounts, or `active` turns false), an inert, id-free copy stays in its place with
 * `.ghost-out` (ui.css) and fades away. The real UI is gone at once, so clicks, focus and state
 * act immediately; the ghost can't be reached. Skipped under reduced motion.
 */
export function useExitGhost(ref: RefObject<HTMLElement | null>, active = true): void {
  useEffect(() => {
    const el = ref.current;
    const parent = el?.parentNode;
    if (!active || !el || !parent) return;
    return () => {
      if (!motionAllowed()) return;
      const ghost = el.cloneNode(true) as HTMLElement;
      // After the commit: still connected means this was only a Strict Mode re-run, not an exit.
      queueMicrotask(() => {
        if (el.isConnected || !parent.isConnected) return;
        ghost.removeAttribute("id");
        ghost.querySelectorAll("[id]").forEach((n) => n.removeAttribute("id"));
        ghost.setAttribute("inert", "");
        ghost.setAttribute("aria-hidden", "true");
        ghost.classList.add("ghost-out");
        parent.appendChild(ghost);
        const done = () => ghost.remove();
        ghost.addEventListener("animationend", (e) => e.target === ghost && done());
        setTimeout(done, GHOST_MAX_MS);
      });
    };
  }, [ref, active]);
}

/** A div whose departure fades out (see `useExitGhost`): a tab's panel, a whole product page. */
export function Ghosted(props: ComponentProps<"div">) {
  const ref = useRef<HTMLDivElement>(null);
  useExitGhost(ref);
  return <div {...props} ref={ref} />;
}
