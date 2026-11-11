import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const read = (path: string) => readFileSync(path, "utf8");

// Every text source file under src/, recursively, excluding tests and binary images.
const sources = readdirSync("src", { recursive: true, withFileTypes: true })
  .filter((e) => e.isFile() && !/\.test\.tsx?$/.test(e.name) && !/\.(?:webp|png|jpe?g)$/i.test(e.name))
  .map((e) => {
    const path = join(e.parentPath, e.name);
    return { name: path, text: read(path) };
  });

// Properties that take a colour; their values must come from tokens.
const COLOUR_PROPS =
  /(?:^|[;{\s])((?:background|color|border[\w-]*|outline[\w-]*|fill|stroke|box-shadow|text-shadow|caret-color|accent-color|text-decoration[\w-]*)\s*:\s*([^;}]+))/gi;
// Drops /* */ and // comments so issue references like "#155" don't read as hex colours.
// A // right after ":" is a URL scheme ("https://"), not a comment.
const stripComments = (text: string) =>
  text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(?<!:)\/\/[^\n]*/g, "");
const COLOUR_LITERAL = /#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(|hwb\(|lab\(|lch\(|oklab\(|oklch\(|color\(/i;
const COLOURLESS =
  /^(?:none|inherit|initial|unset|transparent|currentcolor|underline|0|-?[\d.]+(?:px|em|rem|%)?(?:\s+(?:solid|dashed|dotted|double))?)$/i;

describe("design tokens", () => {
  it("tokens.css defines every token from the launcher UI spec with its value", () => {
    const spec = read("docs/specs/launcher-ui.md");
    // Table rows like: | `--bg` | `#101011` | ... (value may be bold).
    const rows = [...spec.matchAll(/^\| `(--[\w-]+)` \| \**`(#[0-9a-f]+)`\** \|/gim)];
    expect(rows.length).toBeGreaterThanOrEqual(14);
    const tokens = read("src/tokens.css");
    for (const [, name, value] of rows) {
      expect(tokens).toContain(`${name}: ${value};`);
    }
  });

  it("no colour literal appears outside tokens.css", () => {
    for (const { name, text } of sources) {
      // src/art SVGs can't read CSS vars as images; src/art/art.test.ts holds their colours to token values.
      if (name.endsWith("tokens.css") || /^src[\\/]art[\\/][^\\/]+\.svg$/.test(name)) continue;
      expect(stripComments(text), name).not.toMatch(COLOUR_LITERAL);
    }
  });

  it("the colour literal check ignores comments but not code", () => {
    expect(stripComments("// see #155\n/* #abc */ a {}")).not.toMatch(COLOUR_LITERAL);
    expect(stripComments('const u = "https://x"; a { color: #fff; }')).toMatch(COLOUR_LITERAL);
  });

  it("colour properties in stylesheets use tokens, not named colours", () => {
    for (const { name, text } of sources) {
      if (!name.endsWith(".css") || name.endsWith("tokens.css")) continue;
      for (const [, decl, value] of text.matchAll(COLOUR_PROPS)) {
        const v = value.trim();
        expect(v.includes("var(--") || COLOURLESS.test(v), `${name}: ${decl}`).toBe(true);
      }
    }
  });

  it("components carry no inline styles", () => {
    for (const { name, text } of sources) {
      if (/\.(tsx|ts)$/.test(name)) expect(text, name).not.toMatch(/style=\{/);
    }
  });

  it("the colour checks would catch a named colour", () => {
    const bad = "a { color: red; }";
    const hits = [...bad.matchAll(COLOUR_PROPS)].filter(([, , v]) => !v.includes("var(--") && !COLOURLESS.test(v.trim()));
    expect(hits).toHaveLength(1);
  });
});

describe("display names", () => {
  const BRANDING = join("src", "shell", "branding.ts");

  it("the umbrella display name lives only in the swappable branding constant", () => {
    for (const { name, text } of sources) {
      if (name !== BRANDING) expect(text, name).not.toContain("Project Resurrected");
      expect(text, name).not.toMatch(/PD2Resurrect\b/);
    }
  });

  it("the branding constant matches the name Rust reports", () => {
    const rust = read("src-tauri/src/contract/mod.rs").match(/UMBRELLA_DISPLAY_NAME: &str = "([^"]+)"/);
    expect(rust).not.toBeNull();
    expect(read(BRANDING)).toContain(`name: "${rust![1]}"`);
  });

  it("the index.html document title matches the branding constant", () => {
    const title = read("index.html").match(/<title>([^<]+)<\/title>/);
    expect(title).not.toBeNull();
    expect(read(BRANDING)).toContain(`name: "${title![1]}"`);
  });
});

describe("fixture boundaries", () => {
  it("only src/ipc.ts imports @tauri-apps/api or calls invoke", () => {
    for (const { name, text } of sources) {
      if (name === join("src", "ipc.ts") || !/\.(tsx?|css)$/.test(name)) continue;
      expect(text, name).not.toMatch(/(?:from\s+|import\(\s*)["']@tauri-apps\/api|\binvoke\s*[(<]/);
    }
  });

  it("src/api imports ipc.ts for types only, except the live adapter", () => {
    for (const { name, text } of sources) {
      if (!name.startsWith(join("src", "api")) || name === join("src", "api", "live.ts")) continue;
      for (const [line] of text.matchAll(/^import[^;]*from "\.\.\/ipc";/gm)) {
        expect(line, name).toMatch(/^import type /);
      }
    }
  });

  it("no remote URL appears in components or stylesheets", () => {
    for (const { name, text } of sources) {
      if (/\.(tsx|css)$/.test(name)) expect(text, name).not.toMatch(/https?:\/\//);
    }
  });
});

describe("motion", () => {
  it("prefers-reduced-motion turns every animation and transition off", () => {
    const css = read("src/styles.css");
    const block = css.slice(css.indexOf("@media (prefers-reduced-motion: reduce)"));
    expect(block).toMatch(/animation: none !important;/);
    expect(block).toMatch(/transition: none !important;/);
  });

  it("takes every duration and easing from the motion tokens", () => {
    const css = sources.filter((s) => s.name.endsWith(".css") && !s.name.endsWith("tokens.css")).map((s) => stripComments(s.text)).join("\n");
    const tokens = stripComments(read("src/tokens.css"));
    for (const t of ["--dur-fast: 150ms", "--dur-base: 200ms", "--dur-slow: 300ms", "--ease-standard:", "--ease-out:", "--ease-in:"]) {
      expect(tokens).toContain(t);
    }
    // No literal millisecond durations or cubic-beziers outside tokens.css (loops use seconds).
    expect(css).not.toMatch(/\b\d+ms\b/);
    expect(css).not.toMatch(/cubic-bezier|\bease-out;|\bease-in;/);
    expect(css).toContain("pulse 1.6s");
    expect(css).not.toMatch(/scroll-behavior:\s*smooth/);
  });

  it("transitions only compositor properties and colour, never layout", () => {
    const css = sources.filter((s) => s.name.endsWith(".css")).map((s) => stripComments(s.text)).join("\n");
    expect(css).not.toMatch(/transition:[^;]*\b(?:width|height|top|left|right|bottom|margin|padding)\b/);
  });
});

describe("focus", () => {
  it("every focusable element shows the 3px --focus ring at a 2px offset", () => {
    expect(read("src/styles.css")).toMatch(/:focus-visible \{\s*outline: 3px solid var\(--focus\);\s*outline-offset: 2px;/);
  });

  it("window controls use an inset ring", () => {
    expect(read("src/shell/shell.css")).toMatch(/\.win:focus-visible \{\s*outline-offset: -3px;/);
  });
});
