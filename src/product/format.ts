import { isLauncherError, type LauncherError, type ReleaseNotes } from "../api/types";

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

const GENERIC_FAILURE =
  "Something went wrong. Try again. If it keeps happening, use Export diagnostics in Settings → Troubleshooting.";

/** The catalog line for a launcher that can't use the latest release (#272). */
const updateNeeded = (short: string) =>
  `A launcher update is needed for the latest ${short} · Settings → Launcher → Check for updates`;

/** "2147483648 bytes needed, 1073741824 free on …" (preflight.rs) → "2.0 GB needed, 1.0 GB free. ". */
function spaceText(message: string): string {
  const m = /^(\d+) bytes needed, (\d+) free/.exec(message);
  return m ? `${formatBytes(Number(m[1]))} needed, ${formatBytes(Number(m[2]))} free. ` : "";
}

/** Player wording for any launcher error, by code. Rust's `message` is a diagnostic and is never
 *  shown (it carries paths and byte counts); insufficient_space only has its sizes read out of it.
 *  Every toast an error reaches uses this: operation outcomes, failed actions, tray requests, Settings. */
export function operationFailureText(error: unknown, short: string): string {
  const e = isLauncherError(error) ? error : null;
  switch (e?.code) {
    case "access_denied":
      return `Windows blocked a change to the ${short} folder; close Explorer windows in it and try again.`;
    case "inventory_mismatch":
      return `The files in the ${short} folder aren't the ones the launcher installed. Choose Repair or Clean install… in the ⋯ menu.`;
    case "mandatory_setting_conflict":
      return `Settings you changed stop this ${short} release from installing over them. Choose Clean install… in the ⋯ menu.`;
    case "insufficient_space":
      return `Not enough free disk space: ${spaceText(e.message)}Free up space, then try again.`;
    case "operation_in_progress":
      return `Another change to ${short} is still running. Wait for it to finish.`;
    case "recovery_required":
      return `An earlier change to ${short} didn't finish. Choose Check again to restore it.`;
    case "download_failed":
      return "The download failed. Check your connection, then try again.";
    case "verification_failed":
    case "unsafe_archive_entry":
      return "The download didn't pass its safety check, so nothing was changed. Try again.";
    case "release_changed":
      return `${short} has a newer release than the one checked. Try again.`;
    case "game_running":
      return "Quit Diablo II: Resurrected, then try again.";
    case "launcher_update_required":
      return updateNeeded(short);
    case "metadata_invalid":
    case "unsupported_contract_version":
      return `The latest ${short} release information couldn't be read. Try again later.`;
    case "product_not_installed":
      return `${short} isn't installed at this D2R location.`;
    case "d2r_not_found":
      return "Diablo II: Resurrected wasn't found. Choose its folder in Settings → Game.";
    case "game_build_unsupported":
    case "loader_not_ready":
      return `${short} can't be played yet. Its page says what to do.`;
    case "launch_failed":
      return "The game couldn't be started. Try again.";
    case "cancelled":
      return "Cancelled.";
    case "save_breaking_not_acknowledged":
    case "clean_install_consent_required":
      return "That change needs your confirmation first. Try again.";
    default:
      return GENERIC_FAILURE;
  }
}

/** The status line for a catalog failure that isn't plain offline; null when it is. */
export function catalogProblemText(error: LauncherError, short: string): string | null {
  if (error.code === "launcher_update_required") return updateNeeded(short);
  if (error.code === "metadata_invalid" || error.code === "unsupported_contract_version") {
    return `The latest ${short} release information couldn't be read · Try again later`;
  }
  return null;
}

