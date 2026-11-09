import type { LauncherError, ReleaseNotes } from "../api/types";

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

/**
 * "alpha 0.4.2", using the status from the release notes when known. A display version that
 * already names the status ("0.2.0-alpha") is shown as it is, so the status is never repeated.
 */
export function releaseLabel(releaseId: string, version: string, notes: ReleaseNotes[]): string {
  const status = notes.find((n) => n.release_id === releaseId)?.status;
  if (!status || status === "release") return version;
  const named = new RegExp(`(^|[^a-z])${status}([^a-z]|$)`, "i").test(version);
  return named ? version : `${status} ${version}`;
}

const isSeparator = (ch: string | undefined) => ch === " " || ch === "-" || ch === "_" || ch === ".";

/**
 * The version for "Patch <version> notes": the display version without a leading or trailing
 * status ("0.2.0-alpha" → "0.2.0", "alpha 0.4.2" → "0.4.2"), which the tag shows instead.
 * A status in the middle ("0.2.0-alpha.1") is part of the version and stays.
 */
export function patchVersion(version: string, status: ReleaseNotes["status"]): string {
  const v = version.trim();
  const lower = v.toLowerCase();
  if (lower.endsWith(status) && isSeparator(v[v.length - status.length - 1])) {
    return v.slice(0, v.length - status.length - 1).trim() || v;
  }
  if (lower.startsWith(status) && isSeparator(v[status.length])) {
    return v.slice(status.length + 1).trim() || v;
  }
  return v;
}

/** The tag in the Patch Notes meta row: "Alpha", "Beta" or "Release". */
export const statusTag = (status: ReleaseNotes["status"]) => status[0].toUpperCase() + status.slice(1);

export const formatCount = (n: number) => n.toLocaleString("en-US");

/** The red toast after an operation fails (#170): plain copy for a Windows refusal, and
 *  never a raw `\\?\` extended-length path prefix. */
export function operationFailureText(error: LauncherError, short: string): string {
  if (error.code === "access_denied") {
    return `Windows blocked a change to the ${short} folder; close Explorer windows in it and try again.`;
  }
  return error.message.replaceAll("\\\\?\\", "");
}
