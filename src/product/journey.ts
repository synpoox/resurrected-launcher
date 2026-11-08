import type { Inspection, ProductState, ProgressEvent } from "../api/types";

/** The journey keys in docs/specs/launcher-ui.md that come from product state (dialogs and the menu
 *  — `d2r-choose`, `save-breaking`, `manage`, `uninstall` — sit on top of one of these). */
export type Journey =
  | "d2r-missing"
  | "loader-missing"
  | "install"
  | "downloading"
  | "applying"
  | "cancelling"
  | "ready"
  | "update"
  | "blocked"
  | "running"
  | "startup-failed"
  | "repair"
  | "offline"
  | "offline-empty";

const APPLYING: ProgressEvent["phase"][] = ["activating", "committing", "cleaning_up", "recovering"];

/** Derives the journey state from `ProductState` and the latest `ProgressEvent`. */
export function journeyOf(state: ProductState, progress: ProgressEvent | null): Journey {
  if (state.running) {
    if (state.cancelling) return "cancelling";
    if (state.running === "repair") return "repair";
    if (state.running === "uninstall") return "applying";
    if (progress && APPLYING.includes(progress.phase)) return "applying";
    return "downloading";
  }
  const offline = state.catalog.status === "failed";
  switch (state.readiness.status) {
    case "d2r_missing":
      return "d2r-missing";
    case "running":
      return "running";
    case "loader_missing":
    case "loader_unsupported":
      return "loader-missing";
    case "recovery_required":
    case "build_unsupported":
      return "blocked";
    case "not_installed":
      return state.recommended ? "install" : "offline-empty";
    case "update_available":
      return state.startup_failed ? "startup-failed" : "update";
    case "ready":
      if (state.startup_failed) return "startup-failed";
      return offline ? "offline" : "ready";
  }
}

/** Sidebar badge: progress %, ↑ update, ● running, ! error. */
export type Badge = { kind: "progress"; text: string } | { kind: "update" } | { kind: "running" } | { kind: "error" };

export function badgeOf(journey: Journey, percent: number | null): Badge | null {
  switch (journey) {
    case "downloading":
    case "applying":
    case "repair":
    case "cancelling":
      return { kind: "progress", text: percent === null ? "…" : `${percent}%` };
    case "update":
      return { kind: "update" };
    case "running":
      return { kind: "running" };
    case "blocked":
    case "startup-failed":
      return { kind: "error" };
    default:
      return null;
  }
}

/** True when the state shows a notice on Overview (Patch Notes points to it with a red dot). */
export function hasNotice(journey: Journey): boolean {
  return ["d2r-missing", "loader-missing", "blocked", "startup-failed"].includes(journey);
}

/** What the install inspection found in the product folder while no release is installed (issue 126):
 *  - `none`: nothing of the release yet (or not inspected yet): a plain Install.
 *  - `adoptable`: every release file is already there with its bytes: Adopt, no download.
 *  - `incomplete`: some release files are there, none differ, some are missing: Install adds them.
 *  - `different`: files differ from the release (an older version or edited managed files), so
 *    Install is refused; only Clean install, with consent, replaces them. */
export type Existing = "none" | "adoptable" | "incomplete" | "different";

export function existingOf(inspection: Inspection | null): Existing {
  if (!inspection) return "none";
  if (inspection.conflicts.length > 0) return "different";
  if (inspection.files_kept === 0) return "none";
  return inspection.files_added === 0 && inspection.files_replaced_or_removed === 0 ? "adoptable" : "incomplete";
}
