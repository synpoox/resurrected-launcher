import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const read = (path: string) => readFileSync(path, "utf8");
const sources = readdirSync("src")
  .filter((f) => !f.endsWith(".test.ts"))
  .map((f) => ({ name: f, text: read(join("src", f)) }));

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
      if (name === "tokens.css") continue;
      expect(text, name).not.toMatch(/#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(/i);
    }
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
