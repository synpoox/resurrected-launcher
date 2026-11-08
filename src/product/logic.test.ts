import { describe, expect, it } from "vitest";
import type { ProgressEvent } from "../api/types";
import { formatBytes, formatTimeLeft, relativeDate, releaseLabel, startedAgo } from "./format";
import { badgeOf, hasNotice } from "./journey";
import { splitNotes } from "./PatchNotes";
import { progressStats, type Sample } from "./stats";

const MB = 1024 ** 2;
const ev = (over: Partial<ProgressEvent>): ProgressEvent => ({
  product_id: "pd2r",
  kind: "install",
  phase: "downloading",
  boundary: null,
  bytes_done: null,
  bytes_total: null,
  files_done: null,
  files_total: null,
  ...over,
});

describe("progressStats", () => {
  it("computes %, size, speed and time left from successive events", () => {
    const samples: Sample[] = [
      { at: 0, event: ev({ bytes_done: 100 * MB, bytes_total: 1000 * MB }) },
      { at: 1000, event: ev({ bytes_done: 110 * MB, bytes_total: 1000 * MB }) },
      { at: 2000, event: ev({ bytes_done: 120 * MB, bytes_total: 1000 * MB }) },
    ];
    const s = progressStats(samples);
    expect(s.percent).toBe(12);
    expect(s.bytesDone).toBe(120 * MB);
    expect(s.speed).toBeCloseTo(10 * MB);
    expect(s.secondsLeft).toBeCloseTo(88);
  });

  it("has no speed from a single event", () => {
    const s = progressStats([{ at: 0, event: ev({ bytes_done: 1, bytes_total: 4 }) }]);
    expect(s.percent).toBe(25);
    expect(s.speed).toBeNull();
    expect(s.secondsLeft).toBeNull();
  });

  it("averages speed over the recent window only", () => {
    const s = progressStats([
      { at: 0, event: ev({ bytes_done: 0, bytes_total: 100 * MB }) },
      { at: 10_000, event: ev({ bytes_done: 50 * MB, bytes_total: 100 * MB }) },
      { at: 11_000, event: ev({ bytes_done: 51 * MB, bytes_total: 100 * MB }) },
    ]);
    expect(s.speed).toBeCloseTo(1 * MB);
  });

  it("uses files checked for Repair", () => {
    const s = progressStats([{ at: 0, event: ev({ kind: "repair", phase: "verifying", files_done: 50, files_total: 200 }) }]);
    expect(s).toMatchObject({ percent: 25, filesDone: 50, filesTotal: 200, speed: null });
  });
});

describe("format", () => {
  it("formats sizes, time left and dates", () => {
    expect(formatBytes(540 * MB)).toBe("540 MB");
    expect(formatBytes(1.2 * 1024 ** 3)).toBe("1.2 GB");
    expect(formatTimeLeft(62)).toBe("about 1 min left");
    expect(formatTimeLeft(40)).toBe("about 40 s left");
    const now = Date.parse("2026-11-07T12:00:00Z");
    expect(relativeDate("2026-11-04", now)).toBe("3 days ago");
    expect(relativeDate("2026-10-24", now)).toBe("2 weeks ago");
    expect(startedAgo(now / 1000 - 12 * 60, now)).toBe("12 min ago");
  });

  it("labels releases with their status", () => {
    const notes = [{ release_id: "r", display_version: "0.4.2", release_date: "", status: "alpha" as const, notes: "" }];
    expect(releaseLabel("r", "0.4.2", notes)).toBe("alpha 0.4.2");
    expect(releaseLabel("x", "0.4.2", notes)).toBe("0.4.2");
  });
});

describe("journey", () => {
  it("badges: % while busy, ↑ update, ● running, ! error", () => {
    expect(badgeOf("downloading", 52)).toEqual({ kind: "progress", text: "52%" });
    expect(badgeOf("update", null)).toEqual({ kind: "update" });
    expect(badgeOf("running", null)).toEqual({ kind: "running" });
    expect(badgeOf("blocked", null)).toEqual({ kind: "error" });
    expect(badgeOf("ready", null)).toBeNull();
    expect(hasNotice("blocked")).toBe(true);
    expect(hasNotice("ready")).toBe(false);
  });
});

describe("splitNotes", () => {
  it("keeps plain text and line breaks as they are", () => {
    expect(splitNotes("Summary\r\n\r\nA\n- b <i>c</i>")).toEqual({ summary: "Summary", body: "A\n- b <i>c</i>" });
    expect(splitNotes("Only")).toEqual({ summary: "Only", body: "" });
  });
});
