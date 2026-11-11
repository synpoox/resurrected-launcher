import { useEffect, useId, useRef, useState } from "react";
import { arrowKeys } from "./focus";
import { useExitGhost } from "./motion";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  /** "danger" uses `--detail`. */
  tone?: "danger";
  /** Draws a divider above this item. */
  divider?: boolean;
  /**
   * Why the item is unavailable right now (issue 156): it shows `aria-disabled` (still focusable),
   * this text under its label, and choosing it does nothing.
   */
  disabledReason?: string;
}

export interface MenuProps {
  /** The trigger's accessible name, e.g. "Manage PD2R". */
  label: string;
  items: MenuItem[];
  defaultOpen?: boolean;
  /** Extra class names for the trigger button. */
  className?: string;
  /** Another action is running (issue 138): the trigger is `aria-disabled` and the menu won't open. */
  blocked?: boolean;
}

/** The square `⋯` menu. Arrow keys move, Esc closes, focus returns to `⋯`. */
export function Menu({ label, items, defaultOpen = false, className = "", blocked = false }: MenuProps) {
  const [wantOpen, setOpen] = useState(defaultOpen);
  // A menu open when an action starts closes; it doesn't reopen by itself afterwards.
  const open = wantOpen && !blocked;
  const [wasBlocked, setWasBlocked] = useState(blocked);
  if (blocked !== wasBlocked) {
    setWasBlocked(blocked);
    if (blocked) setOpen(false);
  }
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const id = useId();
  useExitGhost(list, open);

  useEffect(() => {
    if (open) list.current?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      const t = e.target as Node;
      if (!list.current?.contains(t) && !trigger.current?.contains(t)) setOpen(false);
    };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [open]);

  const close = (refocus: boolean) => {
    setOpen(false);
    if (refocus) trigger.current?.focus();
  };

  return (
    <span className="menu-wrap">
      <button
        ref={trigger}
        type="button"
        className={`btn ghost square ${className}`}
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        aria-disabled={blocked || undefined}
        onClick={() => !blocked && setOpen((o) => !o)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            if (!blocked) setOpen(true);
          }
        }}
      >
        <span aria-hidden="true">⋯</span>
      </button>
      {open && (
        <div
          ref={list}
          id={id}
          className="menu"
          role="menu"
          aria-label={label}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              close(true);
            } else if (e.key === "Tab") close(false);
            else arrowKeys("[role=menuitem]", "vertical")(e);
          }}
        >
          {items.map((item, i) => (
            <div key={item.label} role="none">
              {item.divider && <hr />}
              <button
                type="button"
                role="menuitem"
                tabIndex={-1}
                className={item.tone ?? undefined}
                aria-disabled={item.disabledReason ? true : undefined}
                aria-describedby={item.disabledReason ? `${id}-why-${i}` : undefined}
                onClick={() => {
                  if (item.disabledReason) return;
                  close(true);
                  item.onSelect();
                }}
              >
                {item.label}
                {item.disabledReason && (
                  <span id={`${id}-why-${i}`} className="why">
                    {item.disabledReason}
                  </span>
                )}
              </button>
            </div>
          ))}
        </div>
      )}
    </span>
  );
}
