// Self-update copy (spec: Launcher self-update), kept pure so it can be tested on its own.
import { productById } from "../products";
import type { SelfUpdateStatus } from "./api";
import type { OperationKind } from "../api/types";
import type { RestartHold } from "./hold";

/** What the product is busy with, for the held-restart text. Downloads keep the spec's wording. */
const BUSY_WITH: Record<OperationKind, string> = {
  install: "downloading",
  update: "downloading",
  clean_install: "downloading",
  adopt: "setting up",
  repair: "repairing",
  uninstall: "uninstalling",
};

/** "Restarts after you quit PD2R" / "Restarts after PD2R finishes downloading" (or repairing, …). */
export function holdText(hold: RestartHold): string {
  const short = productById(hold.product_id)?.short ?? "the game";
  return hold.reason === "product_running"
    ? `Restarts after you quit ${short}`
    : `Restarts after ${short} finishes ${BUSY_WITH[hold.operation]}`;
}

/** The Launcher version row's status, after the version. */
export function versionStatus(s: SelfUpdateStatus): string {
  switch (s.phase) {
    case "up_to_date":
    case "updated":
      return "Up to date";
    case "available":
      return `${s.next} is available`;
    case "downloading":
      return `Downloading ${s.next} · ${s.percent}%`;
    case "ready":
      return `${s.next} ready · installs on restart or Quit`;
    case "restarting":
    case "quitting":
      return `Installing ${s.next}…`;
    case "download_failed":
      return `Couldn't download ${s.next}`;
    case "signature_rejected":
      return `${s.next} rejected: it failed its signature check, so nothing was installed`;
  }
}

export const AUTO_DOWNLOAD_ON_HINT = "Updates download in the background and install when you restart or Quit.";
export const AUTO_DOWNLOAD_OFF_HINT = "You'll be told when an update is available. Nothing downloads until you choose Download.";

export const availableText = (next: string) => `Launcher ${next} is available`;
/**
 * While a game runs, Quit closes the launcher without installing (the update stays ready). While an
 * operation runs, Quit itself waits or asks, so the update installs on Restart or Quit afterwards.
 */
export const readyText = (next: string, hold: RestartHold | null) => {
  if (!hold) return `Launcher ${next} is ready`;
  const short = productById(hold.product_id)?.short ?? "the game";
  return hold.reason === "product_running"
    ? `Launcher ${next} is ready. ${holdText(hold)}. Quitting the launcher while ${short} runs doesn't install it; it stays ready for later.`
    : `Launcher ${next} is ready. ${holdText(hold)}. After that, it also installs when you Quit.`;
};
export const updatedText = (version: string) => `Updated to launcher ${version}`;
export const downloadFailedText = (current: string) =>
  `Couldn't download the launcher update. Launcher ${current} keeps working.`;
export const rejectedText = (next: string, current: string) =>
  `Launcher update rejected. ${next} didn't pass its signature check, so it was deleted and nothing was installed. Launcher ${current} keeps working.`;
