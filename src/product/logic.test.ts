import { describe, expect, it } from "vitest";
import type { Inspection, ProgressEvent } from "../api/types";
import { formatBytes, formatTimeLeft, operationFailureText, patchVersion, relativeDate, releaseLabel, startedAgo, statusTag } from "./format";
import { badgeOf, existingOf, hasNotice } from "./journey";
import { heading, MAX_LIST_DEPTH, MAX_NOTES_CHARS, parseBlocks, parseInline, parseNotes } from "./notesMarkdown";
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

describe("operationFailureText", () => {
  it("says Windows blocked the change, with no path, on access denied", () => {
    const message = String.raw`moving \\?\C:\Games\D2R\mods\PD2Resurrected: Access is denied. (os error 5)`;
    const text = operationFailureText({ code: "access_denied", message }, "PD2R");
    expect(text).toBe("Windows blocked a change to the PD2R folder; close Explorer windows in it and try again.");
  });
  it("never shows the extended-length path prefix", () => {
    const text = operationFailureText({ code: "internal", message: String.raw`creating \\?\C:\x: boom` }, "PD2R");
    expect(text).toBe(String.raw`creating C:\x: boom`);
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
const li = (text: string) => ({ inlines: [t(text)], sublist: null });

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
      truncated: false,
      sections: [
        {
          heading: [t("Items")],
          blocks: [{ kind: "list", ordered: false, items: [li("Crushing blow rolls on normal hits."), li("The Horadric Cube has a 4x4 grid.")] }],
        },
        { heading: [t("Skills")], blocks: [{ kind: "list", ordered: false, items: [li("Dark Pact consumes a curse.")] }] },
      ],
    });
  });

  it("takes an opening paragraph as the summary; plain text without headings is one untitled section", () => {
    expect(parseNotes("PD2 Resurrected 0.1.0-alpha.")).toEqual({ summary: [t("PD2 Resurrected 0.1.0-alpha.")], sections: [], truncated: false });
    expect(parseNotes(["Summary", "", "A line", "- b"].join("\n"))).toEqual({
      summary: [t("Summary")],
      truncated: false,
      sections: [{ heading: null, blocks: [{ kind: "paragraph", inlines: [t("A line")] }, { kind: "list", ordered: false, items: [li("b")] }] }],
    });
  });

  it("sections on the shallowest heading level; deeper headings stay inside as sub-headings", () => {
    const { sections } = parseNotes(
      ["## Items", "### Weapons", "1. Crossbows", "2) Claws", "## Fixes", "* one", "  continued", "  - nested"].join("\n"),
    );
    expect(sections.map((s) => s.heading)).toEqual([[t("Items")], [t("Fixes")]]);
    expect(sections[0].blocks).toEqual([
      { kind: "heading", level: 3, inlines: [t("Weapons")] },
      { kind: "list", ordered: true, items: [li("Crossbows"), li("Claws")] },
    ]);
    expect(sections[1].blocks).toEqual([
      { kind: "list", ordered: false, items: [{ inlines: [t("one continued")], sublist: { kind: "list", ordered: false, items: [li("nested")] } }] },
    ]);
  });

  it("drops rules and keeps fenced code as text", () => {
    expect(parseNotes(["# A", "---", "```", "<b>x</b>", "```"].join("\n")).sections).toEqual([
      { heading: [t("A")], blocks: [{ kind: "code", text: "<b>x</b>" }] },
    ]);
  });
});

// Review of #132: a manifest may be 8 MB, and Patch Notes parse on the UI thread.
describe("parseNotes on huge or malformed notes", () => {
  const timed = <T,>(f: () => T): [T, number] => {
    const start = performance.now();
    const out = f();
    return [out, performance.now() - start];
  };

  it("200k headings neither overflow the stack nor take a second", () => {
    const [parsed, ms] = timed(() => parseNotes("# h\n".repeat(200_000)));
    expect(parsed.truncated).toBe(true);
    expect(parsed.sections.length).toBeGreaterThan(20_000);
    expect(ms).toBeLessThan(1000);
  });

  it("the deepest-first heading level is found without spreading every level into Math.min", () => {
    const notes = "### a\n".repeat(15_000) + "## top\n";
    expect(notes.length).toBeLessThan(MAX_NOTES_CHARS);
    const { sections } = parseNotes(notes);
    expect(sections.map((s) => s.heading)).toEqual([null, [t("top")]]);
  });

  it.each([
    ["unclosed [", "[".repeat(200_000)],
    ["unclosed ![a](", "![a](".repeat(40_000)],
    ["[a](url without a )", "[a](x ".repeat(35_000)],
    ['[a](u "unclosed title', '[a](u "t '.repeat(25_000)],
    ["unclosed backtick runs", "` `` ".repeat(40_000)],
  ])("200 KB of %s parses in linear time", (_, src) => {
    const [inline, ms] = timed(() => parseInline(src));
    expect(inline.map((n) => n.text).join("").length).toBeGreaterThan(0);
    expect(ms).toBeLessThan(1000);
    expect(timed(() => parseNotes(src))[1]).toBeLessThan(1000);
  });

  // Re-review of #132: a backtracking heading regex took 10–13 s on 99k inner spaces.
  it.each([
    ["a heading with 99k inner spaces then text (12.7 s with the old regex)", "# a" + " ".repeat(99_000) + "b"],
    ["a heading with 99k leading spaces then text", "#" + " ".repeat(99_000) + "x"],
    ["a heading with 99k spaces between words and a closing #", "# a" + " ".repeat(99_000) + "b #"],
    ["a heading of 99k spaces and #", "#" + " #".repeat(49_000)],
    ["a list item with 99k spaces", "-" + " ".repeat(99_000) + "x"],
    ["a numbered item with 99k spaces", "1." + " ".repeat(99_000) + "x"],
    ["a rule with 99k spaces", "-" + " ".repeat(99_000) + "-- x"],
    ["a nested item after 99k spaces", "- a\n" + " ".repeat(99_000) + "- b"],
    ["a fence after 99k spaces", " ".repeat(99_000) + "```"],
    ["99k escapes", "\\".repeat(99_000)],
    ["a line of 99k mixed markers", "*_`[!(#-".repeat(12_000)],
  ])("%s parses in under a second", (_, notes) => {
    expect(notes.length).toBeLessThanOrEqual(MAX_NOTES_CHARS);
    const [, ms] = timed(() => parseNotes(notes));
    expect(ms).toBeLessThan(1000);
  });

  it("headings: level, text and an optional closing run of #", () => {
    expect(heading("#" + " ".repeat(99_000) + "x")).toEqual({ level: 1, text: "x" });
    expect(heading("  ### Items ##  ")).toEqual({ level: 3, text: "Items" });
    expect(heading("## C# notes#")).toEqual({ level: 2, text: "C# notes#" });
    expect(heading("# #")).toEqual({ level: 1, text: "" });
    expect(heading("#")).toEqual({ level: 1, text: "" });
    expect(heading("#5 items")).toBeNull();
    expect(heading("####### seven")).toBeNull();
    expect(heading("    # indented code")).toBeNull();
  });

  it("cuts notes over the limit at a line break and says so", () => {
    const line = "- " + "x".repeat(98) + "\n";
    const { sections, truncated } = parseNotes("#### Items\n" + line.repeat(2_000));
    expect(truncated).toBe(true);
    const items = (sections[0].blocks[0] as { items: unknown[] }).items;
    expect(items.length).toBe(Math.floor((MAX_NOTES_CHARS - 10) / 101));
    expect(items.every((i) => JSON.stringify(i) === JSON.stringify(li("x".repeat(98))))).toBe(true);
    expect(parseNotes("#### Items\n- a").truncated).toBe(false);
  });
});

describe("parseBlocks: nested lists and quotes (owner design pass)", () => {
  const sub = (...items: ReturnType<typeof li>[]) => ({ kind: "list", ordered: false, items });

  it("nests bullets by indent and returns to the outer list", () => {
    const blocks = parseBlocks(["- Blossom", "  - AD/AP", "    - deeper", "  - Mana", "- Elderwood", "\t- tabbed"].join("\n"));
    expect(blocks).toEqual([
      {
        kind: "list",
        ordered: false,
        items: [
          { inlines: [t("Blossom")], sublist: { kind: "list", ordered: false, items: [{ inlines: [t("AD/AP")], sublist: sub(li("deeper")) }, li("Mana")] } },
          { inlines: [t("Elderwood")], sublist: sub(li("tabbed")) },
        ],
      },
    ]);
  });

  it(`caps nesting at ${MAX_LIST_DEPTH} levels; deeper items join the deepest list`, () => {
    const notes = ["- 1", "  - 2", "    - 3", "      - 4", "        - 5"].join("\n");
    const [list] = parseBlocks(notes) as { items: { sublist: { items: { sublist: { items: unknown[] } }[] } }[] }[];
    expect(list.items[0].sublist.items[0].sublist.items).toEqual([li("3"), li("4"), li("5")]);
  });

  it("an indented line continues the deepest item; a numbered list under a bullet stays nested", () => {
    expect(parseBlocks(["- a", "  1. one", "     more", "- b"].join("\n"))).toEqual([
      {
        kind: "list",
        ordered: false,
        items: [{ inlines: [t("a")], sublist: { kind: "list", ordered: true, items: [li("one more")] } }, li("b")],
      },
    ]);
  });

  it("a list starts only at an indent of up to three", () => {
    expect(parseBlocks("    - code-like")).toEqual([{ kind: "paragraph", inlines: [t("- code-like")] }]);
  });

  it("> lines are one quote, ended by a blank line or other content", () => {
    expect(parseBlocks(["> Large, like this", ">patch.", "", "> Second", "Text"].join("\n"))).toEqual([
      { kind: "quote", inlines: [t("Large, like this patch.")] },
      { kind: "quote", inlines: [t("Second")] },
      { kind: "paragraph", inlines: [t("Text")] },
    ]);
  });
});

describe("Patch Notes title and tag", () => {
  it("the patch version drops a leading or trailing status, which the tag shows", () => {
    expect(patchVersion("0.2.0-alpha", "alpha")).toBe("0.2.0");
    expect(patchVersion("0.2.0 ALPHA", "alpha")).toBe("0.2.0");
    expect(patchVersion("alpha 0.4.2", "alpha")).toBe("0.4.2");
    expect(patchVersion("0.4.2", "alpha")).toBe("0.4.2");
    expect(patchVersion("0.2.0-alpha.1", "alpha")).toBe("0.2.0-alpha.1");
    expect(patchVersion("0.2.0alpha", "alpha")).toBe("0.2.0alpha");
    expect(patchVersion("alpha", "alpha")).toBe("alpha");
    expect(patchVersion("1.0.0", "release")).toBe("1.0.0");
    expect([statusTag("alpha"), statusTag("beta"), statusTag("release")]).toEqual(["Alpha", "Beta", "Release"]);
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
    // Not links: a space in the URL without a title, a ] not followed by (, an unclosed (.
    expect(parseInline("[a](b c) [d] e [f](g")).toEqual([t("[a](b c) [d] e [f](g")]);
    expect(parseInline("[[x](y)] ![](z)")).toEqual([t("[x] ")]);
  });

  it("leaves snake_case, lone markers and escapes alone", () => {
    expect(parseInline("file_name_here")).toEqual([t("file_name_here")]);
    expect(parseInline("+2/10/18 * 3")).toEqual([t("+2/10/18 * 3")]);
    expect(parseInline(String.raw`\*not em\*`)).toEqual([t("*not em*")]);
    expect(parseInline("a ** b")).toEqual([t("a ** b")]);
  });
});

describe("existingOf (issue 126)", () => {
  const insp = (o: Partial<Inspection>): Inspection => ({
    product_id: "pd2r",
    kind: "install",
    release: { release_id: "pd2r-0.2.0-alpha", display_version: "0.2.0-alpha", save_breaking: false },
    files_replaced_or_removed: 0,
    files_added: 0,
    files_kept: 0,
    preserved: [],
    backup_only: [],
    settings_reset: [],
    conflicts: [],
    save_breaking_notice: null,
    breaks_saves: false,
    space_needed: 0,
    token: "t",
    ...o,
  });
  it("reads the install inspection of a product folder the launcher has no record of", () => {
    expect(existingOf(null)).toBe("none");
    expect(existingOf(insp({ files_added: 1543 }))).toBe("none");
    expect(existingOf(insp({ files_kept: 1543 }))).toBe("adoptable");
    expect(existingOf(insp({ files_kept: 1542, files_added: 1 }))).toBe("incomplete");
    // The owner's clone: 0.1.0 files while 0.2.0 is offered.
    const charstats = { type: "unknown_file" as const, path: "PD2Resurrected.mpq/data/global/excel/charstats.txt" };
    expect(existingOf(insp({ files_kept: 476, files_added: 967, conflicts: [charstats] }))).toBe("different");
  });
});
