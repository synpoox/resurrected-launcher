import type { ProgressEvent } from "../api/types";

export interface Sample {
  /** Milliseconds, from the receiver's clock. */
  at: number;
  event: ProgressEvent;
}

export interface ProgressStats {
  percent: number | null;
  bytesDone: number | null;
  bytesTotal: number | null;
  /** Bytes per second over the recent samples; null until two byte samples exist. */
  speed: number | null;
  /** Seconds; null without a speed. */
  secondsLeft: number | null;
  filesDone: number | null;
  filesTotal: number | null;
}

/** How far back speed is averaged. */
export const SPEED_WINDOW_MS = 5000;

/** Computes %, speed and time left from successive progress events of one operation. */
export function progressStats(samples: Sample[]): ProgressStats {
  const last = samples.at(-1)?.event ?? null;
  const withBytes = samples.filter((s) => s.event.bytes_done !== null && s.event.bytes_total);
  const lastBytes = withBytes.at(-1);
  const bytesDone = lastBytes?.event.bytes_done ?? null;
  const bytesTotal = lastBytes?.event.bytes_total ?? null;

  let speed: number | null = null;
  if (lastBytes && last?.phase === "downloading") {
    const window = withBytes.filter((s) => lastBytes.at - s.at <= SPEED_WINDOW_MS);
    const first = window[0];
    const dt = (lastBytes.at - first.at) / 1000;
    const db = (lastBytes.event.bytes_done ?? 0) - (first.event.bytes_done ?? 0);
    if (dt > 0 && db > 0) speed = db / dt;
  }
  const secondsLeft = speed && bytesTotal !== null && bytesDone !== null ? (bytesTotal - bytesDone) / speed : null;

  const withFiles = samples.filter((s) => s.event.files_total);
  const lastFiles = withFiles.at(-1)?.event;
  const filesDone = lastFiles?.files_done ?? null;
  const filesTotal = lastFiles?.files_total ?? null;

  let percent: number | null = null;
  if (filesTotal && filesDone !== null && last?.files_total) percent = Math.floor((filesDone / filesTotal) * 100);
  else if (bytesTotal && bytesDone !== null) percent = Math.floor((bytesDone / bytesTotal) * 100);

  return { percent, bytesDone, bytesTotal, speed, secondsLeft, filesDone, filesTotal };
}
