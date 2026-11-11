import { useId, useState } from "react";
import type { ReleaseNotes } from "../api/types";
import { HERO_ART } from "../art";
import type { Product } from "../products";
import { patchVersion, statusTag } from "./format";
import { type Block, type Inline, type List, type NotesSection, parseNotes } from "./notesMarkdown";

/** "2026-11-04" → "4 November 2026". */
function longDate(day: string): string {
  const t = Date.parse(`${day}T00:00:00Z`);
  if (Number.isNaN(t)) return day;
  return new Date(t).toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
}

function Text({ inlines }: { inlines: Inline[] }) {
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
function NotesSections({ releaseKey, sections }: { releaseKey: string; sections: NotesSection[] }) {
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

/** Collapsed by default; each entry scrolls to its section and moves focus to its heading. */
function TableOfContents({ releaseKey, sections }: { releaseKey: string; sections: NotesSection[] }) {
  const [open, setOpen] = useState(false);
  const listId = useId();
  const entries = sections.flatMap((s, i) => (s.heading ? [{ i, heading: s.heading }] : []));
  if (entries.length === 0) return null;
  const go = (id: string) => {
    const target = document.getElementById(id);
    if (!target) return;
    // Instant, no smooth scrolling (spec: Motion). jsdom has no scrollIntoView.
    if (typeof target.scrollIntoView === "function") target.scrollIntoView({ block: "start" });
    target.focus({ preventScroll: true });
  };
  return (
    <nav className="notes-toc" aria-label="Table of contents">
      <button type="button" className="notes-toc-toggle" aria-expanded={open} aria-controls={listId} onClick={() => setOpen(!open)}>
        <span>Table of contents</span>
        <span className="notes-toc-icon" aria-hidden="true">
          {open ? "−" : "+"}
        </span>
      </button>
      <ol id={listId} hidden={!open}>
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
