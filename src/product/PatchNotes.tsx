import type { ReleaseNotes } from "../api/types";
import type { Product } from "../products";
import { releaseLabel } from "./format";
import { type Block, type Inline, type NotesSection, parseNotes } from "./notesMarkdown";

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
    case "code":
      return <pre>{block.text}</pre>;
    case "list": {
      const List = block.ordered ? "ol" : "ul";
      return (
        <List>
          {block.items.map((item, i) => (
            <li key={i}>
              <Text inlines={item} />
            </li>
          ))}
        </List>
      );
    }
  }
}

/** Each section of the notes in its own bordered box, under its heading. */
function NotesSections({ id, sections }: { id: string; sections: NotesSection[] }) {
  return sections.map((section, i) => (
    <section key={i} className="notes-section" aria-labelledby={section.heading ? `${id}-${i}` : undefined}>
      {section.heading && (
        <h3 id={`${id}-${i}`} className="notes-heading">
          <Text inlines={section.heading} />
        </h3>
      )}
      {section.blocks.map((b, j) => (
        <NotesBlock key={j} block={b} />
      ))}
    </section>
  ));
}

export interface PatchNotesProps {
  product: Product;
  releases: ReleaseNotes[] | null;
  onRetry: () => void;
}

/** Releases newest first in a centred column. Notes are a Markdown subset, rendered as text only. */
export function PatchNotes({ product, releases, onRetry }: PatchNotesProps) {
  const sorted = [...(releases ?? [])].sort((a, b) => b.release_date.localeCompare(a.release_date));
  return (
    <div className="notes" tabIndex={0} aria-label={`${product.short} Patch Notes`}>
      {releases !== null && sorted.length === 0 && <p className="notes-empty">No Patch Notes yet.</p>}
      {sorted.map((r) => {
        const parts = r.notes === null ? null : parseNotes(r.notes);
        return (
          <article key={r.release_id} aria-labelledby={`rel-${r.release_id}`}>
            <header>
              <h2 id={`rel-${r.release_id}`} className="notes-title">
                {product.short} {releaseLabel(r.release_id, r.display_version, sorted)}
              </h2>
              {parts?.summary && (
                <p className="notes-summary">
                  <Text inlines={parts.summary} />
                </p>
              )}
              <p className="notes-meta">
                <span className="k">Patch notes</span>
                <time dateTime={r.release_date}>{longDate(r.release_date)}</time>
              </p>
            </header>
            {parts ? (
              parts.sections.length > 0 && (
                <div className="notes-body">
                  <NotesSections id={`rel-${r.release_id}`} sections={parts.sections} />
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
