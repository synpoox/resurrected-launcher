// Release notes are publisher text in a small Markdown subset (docs/specs/release-manifest.md).
// This parses it into plain data that PatchNotes renders as React elements, so nothing in the
// notes is ever interpreted as HTML: tags, images and links come out as text.
//
// The parse runs on the UI thread over notes from an up-to-8 MB manifest, so it uses no regular
// expressions: every test is a manual forward scan and the whole parse is linear in the input.
// (A backtracking heading regex took 10 s on a heading with 99k inner spaces.)

export type Inline =
  | { kind: "text"; text: string }
  | { kind: "strong"; text: string }
  | { kind: "em"; text: string }
  | { kind: "code"; text: string };

export type Block =
  | { kind: "heading"; level: number; inlines: Inline[] }
  | { kind: "paragraph"; inlines: Inline[] }
  | { kind: "list"; ordered: boolean; items: Inline[][] }
  | { kind: "code"; text: string };

export interface NotesSection {
  /** Null for content before the first heading. */
  heading: Inline[] | null;
  blocks: Block[];
}

export interface ParsedNotes {
  /** The opening paragraph, when the notes start with one. */
  summary: Inline[] | null;
  sections: NotesSection[];
  /** The notes were longer than MAX_NOTES_CHARS and only their start is shown. */
  truncated: boolean;
}

/** Longest notes parsed; manifests may be 8 MB and the parse runs on the UI thread. */
export const MAX_NOTES_CHARS = 100_000;

// Character classes.

/** Space or tab. */
const isBlank = (ch: string | undefined) => ch === " " || ch === "\t";

/** JavaScript's `\s` (String.prototype.trim strips exactly that set). */
const isSpace = (ch: string | undefined) => ch !== undefined && ch !== "" && ch.trim() === "";

/** `[A-Za-z0-9_]` */
function isWord(ch: string | undefined): boolean {
  if (!ch) return false;
  const c = ch.charCodeAt(0);
  return (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c === 95;
}

/** ASCII punctuation: what a backslash may escape. */
function isPunct(ch: string | undefined): boolean {
  if (!ch) return false;
  const c = ch.charCodeAt(0);
  return (c >= 33 && c <= 47) || (c >= 58 && c <= 64) || (c >= 91 && c <= 96) || (c >= 123 && c <= 126);
}

const isDigit = (ch: string | undefined) => ch !== undefined && ch.length === 1 && ch >= "0" && ch <= "9";

// Line tests.

/** The index after up to three leading spaces, or -1 when there are four or more. */
function indent3(line: string): number {
  let i = 0;
  while (i < 3 && line[i] === " ") i += 1;
  return line[i] === " " ? -1 : i;
}

function skipBlanks(line: string, i: number): number {
  while (isBlank(line[i])) i += 1;
  return i;
}

/** A `#`–`######` heading: its level and text, without an optional closing run of `#`. */
export function heading(line: string): { level: number; text: string } | null {
  const start = indent3(line);
  if (start < 0) return null;
  let i = start;
  while (line[i] === "#") i += 1;
  const level = i - start;
  if (level < 1 || level > 6 || (i < line.length && !isBlank(line[i]))) return null;
  let end = line.length;
  while (end > i && isBlank(line[end - 1])) end -= 1;
  // A closing run of `#` counts only after a blank, or as the whole text.
  let hashes = end;
  while (hashes > i && line[hashes - 1] === "#") hashes -= 1;
  if (hashes < end && (hashes === i || isBlank(line[hashes - 1]))) {
    end = hashes;
    while (end > i && isBlank(line[end - 1])) end -= 1;
  }
  const from = skipBlanks(line, i);
  return { level, text: from < end ? line.slice(from, end) : "" };
}

/**
 * A list item: `-`, `*` or `+`, or 1–9 digits then `.` or `)`, then a blank. Top-level items may
 * be indented up to three spaces; `nested` items must be indented.
 */
function listItem(line: string, nested: boolean): { ordered: boolean; text: string } | null {
  let i = nested ? skipBlanks(line, 0) : indent3(line);
  if (i < 0 || (nested && i === 0)) return null;
  let ordered = false;
  if (line[i] === "-" || line[i] === "*" || line[i] === "+") i += 1;
  else {
    const digits = i;
    while (i - digits < 10 && isDigit(line[i])) i += 1;
    const count = i - digits;
    if (count < 1 || count > 9 || (line[i] !== "." && line[i] !== ")")) return null;
    i += 1;
    ordered = true;
  }
  if (!isBlank(line[i])) return null;
  return { ordered, text: line.slice(skipBlanks(line, i)) };
}

/** Three or more of the same `-`, `*` or `_`, with blanks allowed between: a rule. */
function isRule(line: string): boolean {
  const start = indent3(line);
  if (start < 0) return false;
  const mark = line[start];
  if (mark !== "-" && mark !== "*" && mark !== "_") return false;
  let count = 0;
  for (let i = start; i < line.length; i++) {
    if (line[i] === mark) count += 1;
    else if (!isBlank(line[i])) return false;
  }
  return count >= 3;
}

/** The opening run of a ``` or ~~~ fence (three or more), or null. */
function fenceRun(line: string): string | null {
  const start = indent3(line);
  if (start < 0) return null;
  const mark = line[start];
  if (mark !== "`" && mark !== "~") return null;
  let i = start;
  while (line[i] === mark) i += 1;
  return i - start >= 3 ? line.slice(start, i) : null;
}

// Inline Markdown: `code`, **strong**, *em* or _em_, links and images reduced to their text, \ escapes.
export function parseInline(src: string): Inline[] {
  const out: Inline[] = [];
  let text = "";
  const push = (kind: Inline["kind"], t: string) => {
    if (text) out.push({ kind: "text", text });
    text = "";
    if (t) out.push({ kind, text: kind === "code" ? t : unescape(t) } as Inline);
  };
  // Forward-only cursors: every scan for a closing character resumes where the last one stopped,
  // so unclosed `[`, `(` or backticks cost linear time, not a rescan to the end per opener.
  const nextClose = cursor(src, (ch) => ch === "]");
  const nextParen = cursor(src, (ch) => ch === ")");
  const nextSpace = cursor(src, isSpace);
  const nextNonSpace = cursor(src, (ch) => !isSpace(ch));
  const nextQuote = cursor(src, (ch) => ch === '"' || ch === "\n");
  const fences = new Map<number, number>();
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === "\\" && isPunct(src[i + 1])) {
      text += src[i + 1];
      i += 2;
      continue;
    }
    if (c === "`") {
      let len = 1;
      while (src[i + len] === "`") len += 1;
      const run = "`".repeat(len);
      // The next run of this length, remembered: a later opener never searches the same stretch again.
      let end = fences.get(len);
      if (end === undefined || (end >= 0 && end < i + len)) {
        end = src.indexOf(run, i + len);
        fences.set(len, end);
      }
      if (end > 0) {
        push("code", src.slice(i + len, end).trim());
        i = end + len;
        continue;
      }
      text += run;
      i += len;
      continue;
    }
    // [text](url) and ![alt](url) / ![alt](url "title"): the text only, never a link or an image.
    const open = c === "[" ? i : c === "!" && src[i + 1] === "[" ? i + 1 : -1;
    if (open >= 0) {
      const end = linkEnd(src, open, nextClose, nextParen, nextSpace, nextNonSpace, nextQuote);
      if (end > 0) {
        text += unescape(src.slice(open + 1, nextClose(open + 1)));
        i = end + 1;
        continue;
      }
    }
    if (c === "*" || c === "_") {
      const double = src[i + 1] === c;
      const marker = double ? c + c : c;
      const start = i + marker.length;
      // `_` only at a word boundary, so snake_case and file_names stay as written.
      const leftOk = c === "*" || !isWord(src[i - 1]);
      // findClose stops at the next run of this marker, and no opener of the same marker lies
      // before it, so the searches for one marker never overlap: linear in total.
      const end = findClose(src, marker, start);
      if (leftOk && end > start && !isSpace(src[start]) && !isSpace(src[end - 1])) {
        if (c === "*" || !isWord(src[end + marker.length])) {
          push(double ? "strong" : "em", src.slice(start, end));
          i = end + marker.length;
          continue;
        }
      }
      text += marker;
      i += marker.length;
      continue;
    }
    text += c;
    i += 1;
  }
  if (text) out.push({ kind: "text", text });
  return out;
}

type Cursor = (from: number) => number;

/**
 * The index of the first character at or after `from` matching `pred` (or `src.length`).
 * Queries must not go backwards; the scan position only moves forward, so all queries cost O(n).
 */
function cursor(src: string, pred: (ch: string) => boolean): Cursor {
  let at = -1;
  return (from) => {
    if (at < from) {
      at = from;
      while (at < src.length && !pred(src[at])) at += 1;
    }
    return at;
  };
}

/** For `[` at `open`, the index of the `)` ending `[text](url)` or `[text](url "title")`, else -1. */
function linkEnd(
  src: string,
  open: number,
  nextClose: Cursor,
  nextParen: Cursor,
  nextSpace: Cursor,
  nextNonSpace: Cursor,
  nextQuote: Cursor,
): number {
  const close = nextClose(open + 1);
  if (src[close + 1] !== "(") return -1;
  const paren = nextParen(close + 2);
  if (paren >= src.length) return -1;
  const space = nextSpace(close + 2);
  if (space > paren) return paren; // (url): no space before the `)`
  const quote = nextNonSpace(space);
  if (src[quote] !== '"') return -1;
  const endQuote = nextQuote(quote + 1);
  return src[endQuote] === '"' && endQuote + 1 === paren ? paren : -1;
}

function findClose(src: string, marker: string, from: number): number {
  for (let j = from; j < src.length; j++) {
    if (src[j] === "\\") {
      j += 1;
      continue;
    }
    if (src.startsWith(marker, j) && (marker.length === 2 || src[j + 1] !== marker)) return j;
  }
  return -1;
}

/** Drops the backslash of each `\` + punctuation escape. */
function unescape(s: string): string {
  if (!s.includes("\\")) return s;
  let out = "";
  for (let i = 0; i < s.length; i++) {
    if (s[i] === "\\" && isPunct(s[i + 1])) i += 1;
    out += s[i];
  }
  return out;
}

/** Block Markdown: ATX headings, bullet and numbered lists, paragraphs, fenced code; rules are dropped. */
export function parseBlocks(notes: string): Block[] {
  const lines = notes.replaceAll("\r\n", "\n").replaceAll("\r", "\n").split("\n");
  const blocks: Block[] = [];
  let para: string[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;
  const flush = () => {
    if (para.length) blocks.push({ kind: "paragraph", inlines: parseInline(para.join(" ")) });
    para = [];
    if (list) blocks.push({ kind: "list", ordered: list.ordered, items: list.items.map(parseInline) });
    list = null;
  };
  for (let n = 0; n < lines.length; n++) {
    const line = lines[n];
    const fence = fenceRun(line);
    if (fence) {
      flush();
      const body: string[] = [];
      n += 1;
      while (n < lines.length && !lines[n].trimStart().startsWith(fence)) body.push(lines[n++]);
      blocks.push({ kind: "code", text: body.join("\n") });
      continue;
    }
    if (!line.trim()) {
      // A blank line ends a paragraph; a list carries on if the next item follows.
      if (para.length) flush();
      continue;
    }
    if (isRule(line)) {
      flush();
      continue;
    }
    const head = heading(line);
    if (head) {
      flush();
      blocks.push({ kind: "heading", level: head.level, inlines: parseInline(head.text) });
      continue;
    }
    const item = listItem(line, false);
    if (item) {
      if (para.length || (list && list.ordered !== item.ordered)) flush();
      list ??= { ordered: item.ordered, items: [] };
      list.items.push(item.text);
      continue;
    }
    if (list) {
      // Nested items are kept as items of the same list; other indented lines continue the last item.
      const nested = listItem(line, true);
      if (nested) list.items.push(nested.text);
      else if (isBlank(line[0])) list.items[list.items.length - 1] += ` ${line.trim()}`;
      else {
        flush();
        para.push(line.trim());
      }
      continue;
    }
    para.push(line.trim());
  }
  flush();
  return blocks;
}

/**
 * Splits notes into the opening summary and sections. The shallowest heading level in the notes
 * starts a section; deeper headings stay inside it as sub-headings.
 */
export function parseNotes(notes: string): ParsedNotes {
  let text = notes.trim();
  const truncated = text.length > MAX_NOTES_CHARS;
  if (truncated) {
    // Cut at the last line break inside the limit, so no line is split.
    const cut = text.lastIndexOf("\n", MAX_NOTES_CHARS);
    text = text.slice(0, cut > 0 ? cut : MAX_NOTES_CHARS);
  }
  const blocks = parseBlocks(text);
  let summary: Inline[] | null = null;
  const first = blocks[0];
  if (first?.kind === "paragraph") {
    summary = first.inlines;
    blocks.shift();
  }
  // A loop, not Math.min(...levels): spreading 200k arguments overflows the stack.
  let top = 0;
  for (const b of blocks) if (b.kind === "heading" && (top === 0 || b.level < top)) top = b.level;
  const sections: NotesSection[] = [];
  for (const b of blocks) {
    if (b.kind === "heading" && b.level === top) sections.push({ heading: b.inlines, blocks: [] });
    else {
      if (!sections.length) sections.push({ heading: null, blocks: [] });
      sections[sections.length - 1].blocks.push(b);
    }
  }
  return { summary, sections, truncated };
}
