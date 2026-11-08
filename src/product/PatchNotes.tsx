import type { ReleaseNotes } from "../api/types";
import type { Product } from "../products";
import { releaseLabel } from "./format";

/** "2026-11-04" → "4 November 2026". */
function longDate(day: string): string {
  const t = Date.parse(`${day}T00:00:00Z`);
  if (Number.isNaN(t)) return day;
  return new Date(t).toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
}

/** Splits plain-text notes into the first line (the summary) and the rest. No markup is parsed. */
export function splitNotes(notes: string): { summary: string; body: string } {
  const text = notes.replace(/\r\n?/g, "\n").trim();
  const nl = text.indexOf("\n");
  if (nl < 0) return { summary: text, body: "" };
  return { summary: text.slice(0, nl).trim(), body: text.slice(nl + 1).replace(/^\n+/, "") };
}

export interface PatchNotesProps {
  product: Product;
  releases: ReleaseNotes[] | null;
  onRetry: () => void;
}

/** Releases newest first in a centred column. Notes are plain text with line breaks kept. */
export function PatchNotes({ product, releases, onRetry }: PatchNotesProps) {
  const sorted = [...(releases ?? [])].sort((a, b) => b.release_date.localeCompare(a.release_date));
  return (
    <div className="notes" tabIndex={0} aria-label={`${product.short} Patch Notes`}>
      {releases !== null && sorted.length === 0 && <p className="notes-empty">No Patch Notes yet.</p>}
      {sorted.map((r) => {
        const parts = r.notes === null ? null : splitNotes(r.notes);
        return (
          <article key={r.release_id} aria-labelledby={`rel-${r.release_id}`}>
            <header>
              <h2 id={`rel-${r.release_id}`} className="notes-title">
                {product.short} {releaseLabel(r.release_id, r.display_version, sorted)}
              </h2>
              {parts && <p className="notes-summary">{parts.summary}</p>}
              <p className="notes-meta">
                <span className="k">Patch notes</span>
                <time dateTime={r.release_date}>{longDate(r.release_date)}</time>
              </p>
            </header>
            {parts ? (
              parts.body && <div className="notes-body">{parts.body}</div>
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
