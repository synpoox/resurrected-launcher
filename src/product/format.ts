import type { ReleaseNotes } from "../api/types";

const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** 1288490188 → "1.2 GB"; 566231040 → "540 MB". */
export function formatBytes(bytes: number): string {
  let v = bytes;
  let u = 0;
  while (v >= 1024 && u < UNITS.length - 1) {
    v /= 1024;
    u += 1;
  }
  const digits = u >= 3 && v < 10 ? 1 : 0;
  return `${v.toFixed(digits)} ${UNITS[u]}`;
}

/** Seconds → "about 1 min left" / "about 40 s left" / "about 2 h left". */
export function formatTimeLeft(seconds: number): string {
  if (seconds < 60) return `about ${Math.max(1, Math.round(seconds))} s left`;
  if (seconds < 3600) return `about ${Math.round(seconds / 60)} min left`;
  return `about ${Math.round(seconds / 3600)} h left`;
}

const plural = (n: number, unit: string) => `${n} ${unit}${n === 1 ? "" : "s"} ago`;

/** An ISO date relative to `now`: "today", "3 days ago", "2 weeks ago". */
export function relativeDate(iso: string, now = Date.now()): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const days = Math.floor((now - t) / 86_400_000);
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 7) return plural(days, "day");
  if (days < 30) return plural(Math.floor(days / 7), "week");
  if (days < 365) return plural(Math.floor(days / 30), "month");
  return plural(Math.floor(days / 365), "year");
}

/** Unix seconds → "12 min ago". */
export function startedAgo(sinceSecs: number, now = Date.now()): string {
  const mins = Math.max(0, Math.floor((now / 1000 - sinceSecs) / 60));
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins} min ago`;
  const h = Math.floor(mins / 60);
  return `${h} h ${mins % 60} min ago`;
}

/** "alpha 0.4.2", using the status from the release notes when known. */
export function releaseLabel(releaseId: string, version: string, notes: ReleaseNotes[]): string {
  const status = notes.find((n) => n.release_id === releaseId)?.status;
  return status && status !== "release" ? `${status} ${version}` : version;
}

export const formatCount = (n: number) => n.toLocaleString("en-US");
