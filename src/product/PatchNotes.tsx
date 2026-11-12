import { useEffect, useId, useState } from "react";
import type { ReleaseNotes } from "../api/types";
import { HERO_ART } from "../art";
import type { Product } from "../products";
import { MOTION, motionAllowed } from "../ui/motion";
import { patchVersion, statusTag } from "./format";
import { type Block, type Inline, type List, type NotesSection, parseNotes } from "./notesMarkdown";

/** "2026-11-04" → "4 November 2026". */
function longDate(day: string): string {
  const t = Date.parse(`${day}T00:00:00Z`);
  if (Number.isNaN(t)) return day;
  return new Date(t).toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
}

export function Text({ inlines }: { inlines: Inline[] }) {
  return inlines.map((n, i) =>
    n.kind === "strong" ? (
      <strong key={i}>{n.text}</strong>
    ) : n.kind === "em" ? (
      <em key={i}>{n.text}</em>
    ) : n.kind === "code" ? (
      <code key={i}>{n.text}</code>
    ) : (
      n.text
    ),
  );
}

function NotesList({ list }: { list: List }) {
  const Tag = list.ordered ? "ol" : "ul";
  return (
    <Tag>
      {list.items.map((item, i) => (
        <li key={i}>
          <Text inlines={item.inlines} />
          {item.sublist && <NotesList list={item.sublist} />}
        </li>
      ))}
    </Tag>
  );
}

function NotesBlock({ block }: { block: Block }) {
  switch (block.kind) {
    case "heading":
      return (
        <h4 className="notes-sub">
          <Text inlines={block.inlines} />
        </h4>
      );
    case "paragraph":
      return (
        <p>
          <Text inlines={block.inlines} />
        </p>
      );
    case "quote":
      return (
        <blockquote className="notes-quote">
          <p>
            <Text inlines={block.inlines} />
          </p>
        </blockquote>
      );
    case "code":
      return <pre>{block.text}</pre>;
    case "list":
      return <NotesList list={block} />;
  }
}

/** A section's box: its opening description or quote, a divider, then sub-headings and bullets. */
function SectionBody({ blocks }: { blocks: Block[] }) {
  let k = 0;
  while (k < blocks.length && (blocks[k].kind === "paragraph" || blocks[k].kind === "quote")) k += 1;
  const intro = blocks.slice(0, k);
  const rest = blocks.slice(k);
  return (
    <div className="notes-box">
      {intro.map((b, j) => (
        <NotesBlock key={j} block={b} />
      ))}
      {intro.length > 0 && rest.length > 0 && <hr className="notes-rule" />}
      {rest.map((b, j) => (
        <NotesBlock key={k + j} block={b} />
      ))}
    </div>
  );
}

const sectionId = (releaseKey: string, i: number) => `${releaseKey}-s${i}`;

/** Each section: a large italic heading over its own bordered box. */
export function NotesSections({ releaseKey, sections }: { releaseKey: string; sections: NotesSection[] }) {
  return sections.map((section, i) => (
    <section
      key={i}
      className="notes-section"
      aria-labelledby={section.heading ? sectionId(releaseKey, i) : undefined}
    >
      {section.heading && (
        // tabIndex -1: the table of contents moves focus here.
        <h3 id={sectionId(releaseKey, i)} className="notes-heading" tabIndex={-1}>
          <Text inlines={section.heading} />
        </h3>
      )}
      {section.blocks.length > 0 && <SectionBody blocks={section.blocks} />}
    </section>
  ));
}

/** The nearest ancestor that scrolls vertically (the notes panel), or the page. */
function scrollParent(el: HTMLElement): HTMLElement {
  for (let p = el.parentElement; p; p = p.parentElement) {
    const o = getComputedStyle(p).overflowY;
    if ((o === "auto" || o === "scroll") && p.scrollHeight > p.clientHeight) return p;
  }
  return (document.scrollingElement as HTMLElement | null) ?? document.documentElement;
}

/** Stops the running ease, if any (a new jump, user input, a release switch, unmount). */
let cancelEase: (() => void) | null = null;

/**
 * Scrolls `target` to the top of its scroller over MOTION.slow with --ease-standard (cubic-bezier .4,0,.2,1).
 * One ease runs at a time: a new jump restarts from the current position, and wheel, pointer, touch or
 * key input stops it so the user's scrolling is never fought.
 */
function smoothScrollTo(target: HTMLElement) {
  cancelEase?.();
  const box = scrollParent(target);
  const isPage = box === document.scrollingElement || box === document.documentElement;
  const top = isPage ? 0 : box.getBoundingClientRect().top;
  const margin = parseFloat(getComputedStyle(target).scrollMarginTop) || 0;
  const from = box.scrollTop;
  // Clamped to the room left, so the ease is not cut short against the end.
  const max = Math.max(0, box.scrollHeight - box.clientHeight);
  const delta = Math.max(-from, Math.min(target.getBoundingClientRect().top - top - margin, max - from));
  if (Math.abs(delta) < 1) return;
  const ease = bezier(0.4, 0, 0.2, 1);
  const start = performance.now();
  const inputs = ["wheel", "pointerdown", "touchstart", "keydown"] as const;
  const listenOn: EventTarget[] = isPage ? [window] : [box, window];
  let raf = 0;
  const stop = () => {
    cancelAnimationFrame(raf);
    for (const el of listenOn) for (const e of inputs) el.removeEventListener(e, stop);
    if (cancelEase === stop) cancelEase = null;
  };
  const step = (now: number) => {
    const t = Math.min(1, (now - start) / MOTION.slow);
    box.scrollTop = from + delta * ease(t);
    if (t < 1) raf = requestAnimationFrame(step);
    else stop();
  };
  for (const el of listenOn) for (const e of inputs) el.addEventListener(e, stop, { passive: true });
  cancelEase = stop;
  raf = requestAnimationFrame(step);
}

/** CSS cubic-bezier(x1, y1, x2, y2) as a function of time, solved by bisection. */
function bezier(x1: number, y1: number, x2: number, y2: number) {
  const at = (a: number, b: number, u: number) => 3 * a * u * (1 - u) ** 2 + 3 * b * u * u * (1 - u) + u ** 3;
  return (x: number) => {
    let lo = 0;
    let hi = 1;
    for (let i = 0; i < 24; i++) {
      const mid = (lo + hi) / 2;
      if (at(x1, x2, mid) < x) lo = mid;
      else hi = mid;
    }
    return at(y1, y2, (lo + hi) / 2);
  };
}

/** Collapsed by default; each entry scrolls to its section and moves focus to its heading. */
function TableOfContents({ releaseKey, sections }: { releaseKey: string; sections: NotesSection[] }) {
  // Closing: the list drops back up (height and opacity) before it is hidden; it is already out of reach.
  // Opening and closing clip the list (product.css); steady open doesn't, so focus rings show whole.
  const [phase, setPhase] = useState<"closed" | "opening" | "open" | "closing">("closed");
  const open = phase === "open" || phase === "opening";
  useEffect(() => {
    if (phase !== "closing" && phase !== "opening") return;
    const t = setTimeout(() => setPhase(phase === "closing" ? "closed" : "open"), MOTION.base);
    return () => clearTimeout(t);
  }, [phase]);
  const toggle = () => setPhase(open ? (motionAllowed() ? "closing" : "closed") : motionAllowed() ? "opening" : "open");
  const listId = useId();
  // A release switch or unmount must not leave an ease running onto the old offsets.
  useEffect(() => () => cancelEase?.(), [releaseKey]);
  const entries = sections.flatMap((s, i) => (s.heading ? [{ i, heading: s.heading }] : []));
  if (entries.length === 0) return null;
  const go = (id: string) => {
    const target = document.getElementById(id);
    if (!target) return;
    // Eases over --dur-slow (spec: Motion); instant under reduced motion. Focus never scrolls itself.
    if (motionAllowed()) smoothScrollTo(target);
    // jsdom has no scrollIntoView.
    else if (typeof target.scrollIntoView === "function") target.scrollIntoView({ block: "start" });
    target.focus({ preventScroll: true });
  };
  return (
    <nav className="notes-toc" aria-label="Table of contents">
      <button type="button" className="notes-toc-toggle" aria-expanded={open} aria-controls={listId} onClick={toggle}>
        <span>Table of contents</span>
        <span className="notes-toc-icon" aria-hidden="true">
          {open ? "−" : "+"}
        </span>
      </button>
      <div className={`notes-toc-panel${phase === "closing" ? " closing" : ""}${phase === "opening" || phase === "closing" ? " animating" : ""}`} hidden={phase === "closed"} inert={!open || undefined}>
        <ol id={listId}>
          {entries.map(({ i, heading }) => {
            const id = sectionId(releaseKey, i);
            return (
              <li key={i}>
                {/* A button, not a link: the webview has no anchors (remote.gate). In-page only. */}
                <button type="button" onClick={() => go(id)}>
                  <Text inlines={heading} />
                </button>
              </li>
            );
          })}
        </ol>
        </div>
    </nav>
  );
}

export interface PatchNotesProps {
  product: Product;
  releases: ReleaseNotes[] | null;
  onRetry: () => void;
}

/**
 * Bundled banner art, then releases newest first in a centred column: a two-line title, the
 * summary, a divider and the tag · date row, a table of contents, then the sections.
 * Notes are a Markdown subset, rendered as text only.
 */
export function PatchNotes({ product, releases, onRetry }: PatchNotesProps) {
  const sorted = [...(releases ?? [])].sort((a, b) => b.release_date.localeCompare(a.release_date));
  const art = HERO_ART[product.hero];
  // #179: the newest release's header image, else the bundled hero art. The URL is
  // validated by the manifest reader and the CSP's img-src allows only those hosts.
  const header = sorted[0]?.header_image ?? null;
  const [failed, setFailed] = useState<string | null>(null);
  const remote = header !== null && header !== failed;
  return (
    <div className="notes" tabIndex={0} aria-label={`${product.short} Patch Notes`}>
      <img
        key={remote ? header : "hero"}
        className="notes-banner"
        src={remote ? header : art.src}
        alt={remote ? `${product.name} Patch Notes header` : art.accessibleName}
        onError={remote ? () => setFailed(header) : undefined}
      />
      {releases !== null && sorted.length === 0 && <p className="notes-empty">No Patch Notes yet.</p>}
      {sorted.map((r) => {
        const parts = r.notes === null ? null : parseNotes(r.notes);
        const key = `rel-${r.release_id}`;
        return (
          <article key={r.release_id} aria-labelledby={key}>
            <header className="notes-head">
              <h2 id={key} className="notes-title">
                <span>{product.name}</span>{" "}
                <span>Patch {patchVersion(r.display_version, r.status)} notes</span>
              </h2>
              {parts?.summary && (
                <p className="notes-summary">
                  <Text inlines={parts.summary} />
                </p>
              )}
              <p className="notes-meta">
                <span className="k">{statusTag(r.status)}</span>
                <time dateTime={r.release_date}>{longDate(r.release_date)}</time>
              </p>
            </header>
            {parts ? (
              (parts.sections.length > 0 || parts.truncated) && (
                <div className="notes-body">
                  <TableOfContents releaseKey={key} sections={parts.sections} />
                  <NotesSections releaseKey={key} sections={parts.sections} />
                  {/* Also when the cut left only a summary (one huge paragraph), not just after sections. */}
                  {parts.truncated && <p className="notes-truncated">These notes are too long to show in full.</p>}
                </div>
              )
            ) : (
              <p className="notes-offline">
                Patch notes for newer releases need a connection.{" "}
                <button type="button" className="link" onClick={onRetry}>
                  Retry
                </button>
              </p>
            )}
          </article>
        );
      })}
    </div>
  );
}
