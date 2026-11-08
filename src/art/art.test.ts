import { readFileSync, readdirSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { HERO_ART, MARK_SRC } from "./index";

const dir = "src/art";
const index = readFileSync(`${dir}/index.ts`, "utf8");

describe("bundled art", () => {
  it("every file in src/art is referenced from index.ts", () => {
    for (const file of readdirSync(dir)) {
      if (file === "index.ts" || file.endsWith(".test.ts")) continue;
      expect(index, file).toContain(`"./${file}`);
    }
  });

  it("every colour in the art is a token value", () => {
    const tokens = readFileSync("src/tokens.css", "utf8").match(/#[0-9a-f]+(?=;)/gi) ?? [];
    for (const file of readdirSync(dir).filter((f) => f.endsWith(".svg"))) {
      const svg = readFileSync(`${dir}/${file}`, "utf8");
      expect(svg, file).not.toMatch(/rgba?\(|hsla?\(|hwb\(|lab\(|lch\(|color\(/i);
      for (const colour of svg.match(/#[0-9a-f]+\b/gi) ?? []) expect(tokens, `${file}: ${colour}`).toContain(colour);
    }
  });

  it("no art source is a remote URL", () => {
    for (const src of [MARK_SRC, ...Object.values(HERO_ART).map((a) => a.src)]) {
      expect(src).not.toMatch(/^(?:[a-z][\w+.-]*:|\/\/)/i);
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
