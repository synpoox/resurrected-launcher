import { describe, expect, it } from "vitest";
import type { ProgressEvent } from "../api/types";
import { formatBytes, formatTimeLeft, relativeDate, releaseLabel, startedAgo } from "./format";
import { badgeOf, hasNotice } from "./journey";
import { parseInline, parseNotes } from "./notesMarkdown";
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
    // #128: a display version that already names the status is not prefixed again.
    expect(releaseLabel("r", "0.2.0-alpha", notes)).toBe("0.2.0-alpha");
    expect(releaseLabel("r", "Alpha 3", notes)).toBe("Alpha 3");
    expect(releaseLabel("r", "alphabet 1", notes)).toBe("alpha alphabet 1");
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

const t = (text: string) => ({ kind: "text", text });

describe("parseNotes", () => {
  // The shape of the published pd2r 0.2.0-alpha notes (#128): no summary, `####` sections, bullets.
  it("makes each heading a section of its bullets, with no summary when the notes open on a heading", () => {
    const notes = [
      "#### Items\r",
      "\r",
      "- Crushing blow rolls on normal hits.",
      "- The Horadric Cube has a 4x4 grid.",
      "",
      "#### Skills",
      "",
      "- Dark Pact consumes a curse.",
      "",
    ].join("\n");
    expect(parseNotes(notes)).toEqual({
      summary: null,
      sections: [
        {
          heading: [t("Items")],
          blocks: [{ kind: "list", ordered: false, items: [[t("Crushing blow rolls on normal hits.")], [t("The Horadric Cube has a 4x4 grid.")]] }],
        },
        { heading: [t("Skills")], blocks: [{ kind: "list", ordered: false, items: [[t("Dark Pact consumes a curse.")]] }] },
      ],
    });
  });

  it("takes an opening paragraph as the summary; plain text without headings is one untitled section", () => {
    expect(parseNotes("PD2 Resurrected 0.1.0-alpha.")).toEqual({ summary: [t("PD2 Resurrected 0.1.0-alpha.")], sections: [] });
    expect(parseNotes(["Summary", "", "A line", "- b"].join("\n"))).toEqual({
      summary: [t("Summary")],
      sections: [{ heading: null, blocks: [{ kind: "paragraph", inlines: [t("A line")] }, { kind: "list", ordered: false, items: [[t("b")]] }] }],
    });
  });

  it("sections on the shallowest heading level; deeper headings stay inside as sub-headings", () => {
    const { sections } = parseNotes(
      ["## Items", "### Weapons", "1. Crossbows", "2) Claws", "## Fixes", "* one", "  continued", "  - nested"].join("\n"),
    );
    expect(sections.map((s) => s.heading)).toEqual([[t("Items")], [t("Fixes")]]);
    expect(sections[0].blocks).toEqual([
      { kind: "heading", level: 3, inlines: [t("Weapons")] },
      { kind: "list", ordered: true, items: [[t("Crossbows")], [t("Claws")]] },
    ]);
    expect(sections[1].blocks).toEqual([{ kind: "list", ordered: false, items: [[t("one continued")], [t("nested")]] }]);
  });

  it("drops rules and keeps fenced code as text", () => {
    expect(parseNotes(["# A", "---", "```", "<b>x</b>", "```"].join("\n")).sections).toEqual([
      { heading: [t("A")], blocks: [{ kind: "code", text: "<b>x</b>" }] },
    ]);
  });
});

describe("parseInline", () => {
  it("reads strong, em and code", () => {
    expect(parseInline("**Bold** and *em* and _em_ and `x_y`")).toEqual([
      { kind: "strong", text: "Bold" },
      t(" and "),
      { kind: "em", text: "em" },
      t(" and "),
      { kind: "em", text: "em" },
      t(" and "),
      { kind: "code", text: "x_y" },
    ]);
  });

  it("keeps HTML, links and images as text: no tag, URL or remote image survives", () => {
    expect(parseInline('<img src=x> [site](https://e.com) ![art](https://e.com/a.png "t")')).toEqual([t("<img src=x> site art")]);
  });

  it("leaves snake_case, lone markers and escapes alone", () => {
    expect(parseInline("file_name_here")).toEqual([t("file_name_here")]);
    expect(parseInline("+2/10/18 * 3")).toEqual([t("+2/10/18 * 3")]);
    expect(parseInline(String.raw`\*not em\*`)).toEqual([t("*not em*")]);
    expect(parseInline("a ** b")).toEqual([t("a ** b")]);
  });
});
