// Release notes are publisher text in a small Markdown subset (docs/specs/release-manifest.md).
// This parses it into plain data that PatchNotes renders as React elements, so nothing in the
// notes is ever interpreted as HTML: tags, images and links come out as text.

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

const HEADING = /^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$/;
const BULLET = /^ {0,3}[-*+][ \t]+(.*)$/;
const ORDERED = /^ {0,3}\d{1,9}[.)][ \t]+(.*)$/;
const NESTED_ITEM = /^[ \t]+(?:[-*+]|\d{1,9}[.)])[ \t]+(.*)$/;
const RULE = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const FENCE = /^ {0,3}(`{3,}|~{3,})/;

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
  const nextSpace = cursor(src, (ch) => /\s/.test(ch));
  const nextNonSpace = cursor(src, (ch) => !/\s/.test(ch));
  const nextQuote = cursor(src, (ch) => ch === '"' || ch === "\n");
  const fences = new Map<number, number>();
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === "\\" && i + 1 < src.length && /[!-/:-@[-`{-~]/.test(src[i + 1])) {
      text += src[i + 1];
      i += 2;
      continue;
    }
    if (c === "`") {
      let len = 1;
      while (src[i + len] === "`") len += 1;
      const run = "`".repeat(len);
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
      const leftOk = c === "*" || i === 0 || !/\w/.test(src[i - 1]);
      const end = findClose(src, marker, start);
      if (leftOk && end > start && !/\s/.test(src[start]) && !/\s/.test(src[end - 1])) {
        if (c === "*" || end + marker.length >= src.length || !/\w/.test(src[end + marker.length])) {
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

const unescape = (s: string) => s.replace(/\\([!-/:-@[-`{-~])/g, "$1");

/** Block Markdown: ATX headings, bullet and numbered lists, paragraphs, fenced code; rules are dropped. */
export function parseBlocks(notes: string): Block[] {
  const lines = notes.replace(/\r\n?/g, "\n").split("\n");
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
    const fence = FENCE.exec(line);
    if (fence) {
      flush();
      const body: string[] = [];
      n += 1;
      while (n < lines.length && !lines[n].trimStart().startsWith(fence[1])) body.push(lines[n++]);
      blocks.push({ kind: "code", text: body.join("\n") });
      continue;
    }
    if (!line.trim()) {
      // A blank line ends a paragraph; a list carries on if the next item follows.
      if (para.length) flush();
      continue;
    }
    if (RULE.test(line)) {
      flush();
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      flush();
      blocks.push({ kind: "heading", level: heading[1].length, inlines: parseInline(heading[2] ?? "") });
      continue;
    }
    const item = BULLET.exec(line) ?? ORDERED.exec(line);
    if (item) {
      const ordered = !BULLET.test(line);
      if (para.length || (list && list.ordered !== ordered)) flush();
      list ??= { ordered, items: [] };
      list.items.push(item[1]);
      continue;
    }
    if (list) {
      // Nested items are kept as items of the same list; other indented lines continue the last item.
      const nested = NESTED_ITEM.exec(line);
      if (nested) list.items.push(nested[1]);
      else if (/^[ \t]/.test(line)) list.items[list.items.length - 1] += ` ${line.trim()}`;
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
