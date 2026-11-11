import { useState } from "react";
import { createPortal } from "react-dom";
import type { NewsItem, NewsState } from "../api/types";
import { Dialog } from "../ui/Dialog";
import { relativeDate } from "./format";

export const NEWS_LIMIT = 3;

export interface NewsFeedProps {
  news: NewsState | null;
  onOpen: (index: number) => void;
  onRetry: () => void;
  /** A Retry is in flight: the button is disabled and the line says so. */
  retrying?: boolean;
  /** When the last Retry failed again; shown as "Still offline". */
  failedAt?: Date | null;
}

/** Newest first, keeping each item's index in the product's feed (links open by index). */
function newestFirst(items: NewsItem[]) {
  return items.map((item, index) => ({ item, index })).sort((a, b) => b.item.published.localeCompare(a.item.published));
}

function NewsList({ items, onOpen }: { items: { item: NewsItem; index: number }[]; onOpen: (index: number) => void }) {
  return (
    <ul className="feed-list">
      {items.map(({ item, index }) => (
        <li key={index}>
          <button type="button" className="feed-item" onClick={() => onOpen(index)}>
            <span className="feed-meta">
              <span className="feed-tag">{item.tag}</span>
              <span aria-hidden="true">·</span> {relativeDate(item.published)}
            </span>
            <span className="feed-title">
              {item.title} <span aria-hidden="true">↗</span>
              <span className="sr-only"> (opens in your browser)</span>
            </span>
            {item.summary && <span className="feed-summary">{item.summary}</span>}
          </button>
        </li>
      ))}
    </ul>
  );
}

/** Bottom-right, text only: announcements and guides as cards stacked vertically (issue 139).
 *  Links open by index, never by URL. More than NEWS_LIMIT items adds See all, which lists
 *  every item in a dialog, newest first (issue 192). */
export function NewsFeed({ news, onOpen, onRetry, retrying = false, failedAt = null }: NewsFeedProps) {
  const [allFor, setAllFor] = useState<NewsState | null>(null);
  const all = allFor !== null && allFor === news;
  const setAll = (open: boolean) => setAllFor(open ? news : null);
  // Open only for the news object it was opened on: a refresh closes it for good.
  // An empty list renders nothing: no heading, no placeholder (#183). After the hooks above.
  if (news?.status === "ok" && news.items.length === 0) return null;
  let body;
  if (news === null) body = null;
  else if (news.status === "offline") {
    body = (
      <p className="feed-empty" role="status">
        {retrying
          ? "Retrying…"
          : failedAt
            ? `Still offline (tried at ${failedAt.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}).`
            : "News needs a connection."}{" "}
        <button
          type="button"
          className="link"
          disabled={retrying}
          aria-busy={retrying || undefined}
          onClick={onRetry}
        >
          Retry
        </button>
      </p>
    );
  } else {
    const items = newestFirst(news.items);
    body = (
      <>
        <NewsList items={items.slice(0, NEWS_LIMIT)} onOpen={onOpen} />
        {items.length > NEWS_LIMIT && (
          <button type="button" className="link feed-all" onClick={() => setAll(true)}>
            See all ({items.length})
          </button>
        )}
        {all &&
          // Portaled out of .bottom, whose stacking context would leave .tabs and .topact above the backdrop.
          createPortal(
            <Dialog
              title="All news"
              onClose={() => setAll(false)}
              actions={
                <button type="button" className="btn ghost" onClick={() => setAll(false)}>
                  Close
                </button>
              }
            >
              <NewsList items={items} onOpen={onOpen} />
            </Dialog>,
            document.body,
          )}
      </>
    );
  }
  return (
    <section className="feed" aria-label="News">
      <h2 className="label">News</h2>
      {body}
    </section>
  );
}
