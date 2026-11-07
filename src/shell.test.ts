import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const read = (path: string) => readFileSync(path, "utf8");

// Every source file under src/, recursively, excluding tests.
const sources = readdirSync("src", { recursive: true, withFileTypes: true })
  .filter((e) => e.isFile() && !e.name.endsWith(".test.ts"))
  .map((e) => {
    const path = join(e.parentPath, e.name);
    return { name: path, text: read(path) };
  });

// Properties that take a colour; their values must come from tokens.
const COLOUR_PROPS =
  /(?:^|[;{\s])((?:background|color|border[\w-]*|outline[\w-]*|fill|stroke|box-shadow|text-shadow|caret-color|accent-color|text-decoration[\w-]*)\s*:\s*([^;}]+))/gi;
const COLOURLESS = /^(?:none|inherit|initial|unset|transparent|currentcolor|0|[\d.]+(?:px|em|rem)?(?:\s+(?:solid|dashed|dotted))?)$/i;

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
      if (name.endsWith("tokens.css")) continue;
      expect(text, name).not.toMatch(/#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(|hwb\(|lab\(|lch\(|oklab\(|oklch\(|color\(/i);
    }
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
  it("the umbrella display name comes from Rust, not the frontend", () => {
    for (const { name, text } of sources) {
      expect(text, name).not.toContain("Project Resurrected");
      expect(text, name).not.toMatch(/PD2Resurrect\b/);
    }
  });
});
