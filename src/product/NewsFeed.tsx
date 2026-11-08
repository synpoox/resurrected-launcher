import type { NewsState } from "../api/types";
import { relativeDate } from "./format";

export const NEWS_LIMIT = 3;

export interface NewsFeedProps {
  news: NewsState | null;
  onOpen: (index: number) => void;
  onRetry: () => void;
}

/** Bottom-right, text only: announcements and guides. Links open by index, never by URL. */
export function NewsFeed({ news, onOpen, onRetry }: NewsFeedProps) {
  let body;
  if (news === null) body = null;
  else if (news.status === "offline") {
    body = (
      <p className="feed-empty">
        News needs a connection.{" "}
        <button type="button" className="link" onClick={onRetry}>
          Retry
        </button>
      </p>
    );
  } else if (news.items.length === 0) body = <p className="feed-empty">Nothing yet.</p>;
  else {
    const items = [...news.items]
      .map((item, index) => ({ item, index }))
      .sort((a, b) => b.item.published.localeCompare(a.item.published))
      .slice(0, NEWS_LIMIT);
    body = (
      <ul>
        {items.map(({ item, index }) => (
          <li key={index}>
            <button type="button" className="feed-item" onClick={() => onOpen(index)}>
              <span className="feed-meta">
                {item.tag} · {relativeDate(item.published)}
              </span>
              <span className="feed-title">
                {item.title} <span aria-hidden="true">↗</span>
                <span className="sr-only"> (opens in your browser)</span>
              </span>
            </button>
          </li>
        ))}
      </ul>
    );
  }
  return (
    <section className="feed" aria-label="News">
      <h2 className="label">News</h2>
      {body}
    </section>
  );
}
