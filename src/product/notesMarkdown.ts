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
}

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
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === "\\" && i + 1 < src.length && /[!-/:-@[-`{-~]/.test(src[i + 1])) {
      text += src[i + 1];
      i += 2;
      continue;
    }
    if (c === "`") {
      const run = /^`+/.exec(src.slice(i))![0];
      const end = src.indexOf(run, i + run.length);
      if (end > 0) {
        push("code", src.slice(i + run.length, end).trim());
        i = end + run.length;
        continue;
      }
      text += run;
      i += run.length;
      continue;
    }
    // [text](url) and ![alt](url): the text only, never a link or an image.
    const link = /^!?\[([^\]]*)\]\([^)\s]*(?:\s+"[^"]*")?\)/.exec(src.slice(i));
    if (link) {
      text += unescape(link[1]);
      i += link[0].length;
      continue;
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
  const blocks = parseBlocks(notes.trim());
  let summary: Inline[] | null = null;
  if (blocks[0]?.kind === "paragraph") summary = (blocks.shift() as { inlines: Inline[] }).inlines;
  const levels = blocks.flatMap((b) => (b.kind === "heading" ? [b.level] : []));
  const top = levels.length ? Math.min(...levels) : 0;
  const sections: NotesSection[] = [];
  for (const b of blocks) {
    if (b.kind === "heading" && b.level === top) sections.push({ heading: b.inlines, blocks: [] });
    else {
      if (!sections.length) sections.push({ heading: null, blocks: [] });
      sections[sections.length - 1].blocks.push(b);
    }
  }
  return { summary, sections };
}
