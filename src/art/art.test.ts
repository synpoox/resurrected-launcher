import { readFileSync, readdirSync, statSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { HERO_ART, MARK_SRC } from "./index";

const dir = "src/art";
const index = readFileSync(`${dir}/index.ts`, "utf8");
const tokens: string[] = readFileSync("src/tokens.css", "utf8").match(/#[0-9a-f]+(?=;)/gi) ?? [];

// Paint values, as attributes (fill="…") or style declarations (fill: …), that aren't a token, none, transparent or a url(#…) reference.
const PAINT = /(?:^|[\s;"'{])(?:fill|stroke|stop-color|flood-color|lighting-color|color)\s*(?:=\s*["']([^"']*)["']|:\s*([^;"'}]*))/gi;
const badPaints = (svg: string) =>
  [...svg.matchAll(PAINT)]
    .map(([, attr, decl]) => (attr ?? decl).trim())
    .filter((v) => !tokens.includes(v) && !/^(?:none|transparent|url\(#[\w-]+\))$/i.test(v));

describe("bundled art", () => {
  it("every file in src/art is referenced from index.ts", () => {
    for (const file of readdirSync(dir)) {
      if (file === "index.ts" || file.endsWith(".test.ts")) continue;
      expect(index, file).toContain(`"./${file}`);
    }
  });

  it("every colour in the art is a token value", () => {
    for (const file of readdirSync(dir).filter((f) => f.endsWith(".svg"))) {
      const svg = readFileSync(`${dir}/${file}`, "utf8");
      expect(badPaints(svg), file).toEqual([]);
      for (const colour of svg.match(/#[0-9a-f]+\b/gi) ?? []) expect(tokens, `${file}: ${colour}`).toContain(colour);
    }
  });

  it("the colour check would catch a named, current or off-token colour", () => {
    const bad = `<path fill="red"/><path stroke="currentColor"/><stop stop-color="white"/><path style="fill: #e3353e"/>`;
    expect(badPaints(bad)).toHaveLength(4);
    expect(badPaints(`<path fill="#e3353d" stroke="none"/><rect fill="url(#glow)"/>`)).toEqual([]);
  });

  it("no art source is a remote URL", () => {
    for (const src of [MARK_SRC, ...Object.values(HERO_ART).map((a) => a.src)]) {
      expect(src).not.toMatch(/^(?:[a-z][\w+.-]*:|\/\/)/i);
    }
  });

  it("every hero is real, bundled, optimised art credited to Blizzard, framed per the spec", () => {
    const framing = { pd2r: ["55% center", undefined], forever: ["74% 22%", "auto 135%"], "09": ["74% 22%", "auto 135%"] };
    for (const [id, art] of Object.entries(HERO_ART)) {
      expect(art.placeholder, id).toBe(false);
      expect(art.source, id).toBe("Official D2R promotional artwork © Blizzard Entertainment");
      expect([art.position, art.size], id).toEqual(framing[id as keyof typeof framing]);
      expect(art.accessibleName, id).not.toMatch(/placeholder/i);
    }
    for (const file of readdirSync(dir).filter((f) => f.endsWith(".webp"))) {
      expect(statSync(`${dir}/${file}`).size, file).toBeLessThan(250_000);
    }
  });

  it("every hero has an accessible name, name and source", () => {
    for (const [id, art] of Object.entries(HERO_ART)) {
      expect(art.accessibleName.trim(), id).not.toBe("");
      expect(art.name.trim(), id).not.toBe("");
      expect(art.source.trim(), id).not.toBe("");
    }
  });
});
