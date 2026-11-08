// Slice 4 gate (#81), clause "Keyboard and accessibility": Patch Notes contrast against WCAG 2.2 AA.
// Patch Notes fills the main area on --bg with no hero art (docs/specs/launcher-ui.md), so every text
// colour there sits on a solid, known background and its ratio can be computed from src/tokens.css.
// The rules are read from the shipped CSS, so a colour change in it is measured here.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const tokens = Object.fromEntries(
  [...readFileSync("src/tokens.css", "utf8").matchAll(/(--[\w-]+):\s*(#[0-9a-f]+);/gi)].map((m) => [m[1], m[2]]),
);

function rgba(hex: string): [number, number, number, number] {
  const h = hex.slice(1);
  const full = h.length <= 4 ? [...h].map((c) => c + c).join("") : h;
  const n = (i: number) => parseInt(full.slice(i, i + 2), 16) / 255;
  return [n(0), n(2), n(4), full.length === 8 ? n(6) : 1];
}

/** `fg` composited over the opaque `bg`. */
function over(fg: string, bg: string): [number, number, number] {
  const [r, g, b, a] = rgba(fg);
  const [R, G, B] = rgba(bg);
  return [r * a + R * (1 - a), g * a + G * (1 - a), b * a + B * (1 - a)];
}

const lum = ([r, g, b]: [number, number, number]) => {
  const c = (v: number) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
  return 0.2126 * c(r) + 0.7152 * c(g) + 0.0722 * c(b);
};

/** WCAG contrast ratio of `fg` (possibly translucent) over the opaque `bg`. */
function ratio(fg: string, bg: string): number {
  const [a, b] = [lum(over(fg, bg)), lum(over(bg, bg))];
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/** The declarations of every rule in `css` whose selector list is exactly `selector`. */
function rule(css: string, selector: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{([^}]*)\}/g)) {
    const selectors = m[1].split(",").map((s) => s.trim());
    if (!selectors.includes(selector)) continue;
    for (const d of m[2].split(";")) {
      const [k, ...v] = d.split(":");
      if (k?.trim() && v.length) out[k.trim()] = v.join(":").trim();
    }
  }
  return out;
}

const product = readFileSync("src/product/product.css", "utf8");
const ui = readFileSync("src/ui/ui.css", "utf8");
const token = (value: string | undefined) => {
  const name = value?.match(/var\((--[\w-]+)\)/)?.[1];
  expect(name, `a token colour, got ${value}`).toBeDefined();
  return tokens[name!];
};
const isLarge = (decl: Record<string, string>) => {
  const px = parseFloat(decl["font-size"] ?? "0");
  const bold = Number(decl["font-weight"] ?? 400) >= 700;
  return px >= 24 || (bold && px >= 18.66);
};

const BG = tokens["--bg"];

describe("Patch Notes contrast (WCAG 2.2 AA, on --bg)", () => {
  it("--bg is the page background and the top fade", () => {
    expect(BG).toBe("#101011");
    expect(rule(product, ".fade").background).toContain("var(--bg)");
  });

  // Text on the Patch Notes page, by the rule that colours it.
  const text: [string, Record<string, string>][] = [
    [".notes-summary", rule(product, ".notes-summary")],
    [".notes-meta", rule(product, ".notes-meta")],
    [".notes-meta .k", { ...rule(product, ".notes-meta"), ...rule(product, ".notes-meta .k") }],
    [".notes-body", rule(product, ".notes-body")],
    [".notes-offline", rule(product, ".notes-offline")],
    [".notes-empty", rule(product, ".notes-empty")],
    [".notes-truncated", rule(product, ".notes-truncated")],
    [".tabs [role=\"tab\"] (unselected)", rule(product, '.tabs [role="tab"]')],
    [".link (Retry)", rule(ui, ".link")],
  ];
  it.each(text)("%s", (_, decl) => {
    const fg = token(decl.color);
    const need = isLarge(decl) ? 3 : 4.5;
    expect(ratio(fg, BG)).toBeGreaterThanOrEqual(need);
  });

  // Sections are boxes on their own background (#128): body text, headings and markers on it.
  it("section boxes: text, headings and list markers on the section background", () => {
    const section = rule(product, ".notes-section");
    const bg = token(section.background);
    expect(ratio(token(rule(product, ".notes-body").color), bg)).toBeGreaterThanOrEqual(4.5);
    expect(ratio(token(rule(product, ".notes-heading").color), bg)).toBeGreaterThanOrEqual(4.5);
    expect(ratio(token(rule(product, ".notes-sub").color), bg)).toBeGreaterThanOrEqual(4.5);
    expect(ratio(token(rule(product, ".notes-section strong").color), bg)).toBeGreaterThanOrEqual(4.5);
    expect(ratio(token(rule(product, ".notes-section li::marker").color), bg)).toBeGreaterThanOrEqual(3);
  });

  it("release titles inherit --text and pass as large text", () => {
    const title = rule(product, ".notes-title");
    expect(title.color).toBeUndefined();
    expect(isLarge(title)).toBe(true);
    // Large text needs 3:1; it also clears 4.5:1 by a wide margin (about 16.9:1).
    expect(ratio(tokens["--text"], BG)).toBeGreaterThanOrEqual(4.5);
  });

  it("selected tab: --text on --panel, frame translucent over --bg", () => {
    const selected = rule(product, '.tabs [role="tab"][aria-selected="true"]');
    expect(ratio(token(selected.color), token(selected.background))).toBeGreaterThanOrEqual(4.5);
    // Unselected tabs sit on the frame (--bg at 70%) over --bg, which is --bg itself.
    expect(token(rule(product, ".tabs").background)).toBe(tokens["--frame"]);
  });

  it("the main action at top-left: white on --button", () => {
    const primary = rule(ui, ".btn.primary");
    expect(ratio(token(primary.color), token(primary.background))).toBeGreaterThanOrEqual(4.5);
    // Matches the spec's measurements (Accessibility: white on --button 5.44, on --ribbon 6.25).
    expect(ratio(tokens["--white"], tokens["--button"])).toBeCloseTo(5.44, 1);
    expect(ratio(tokens["--white"], tokens["--ribbon"])).toBeCloseTo(6.25, 1);
    expect(ratio(tokens["--white"], tokens["--button-hover"])).toBeGreaterThanOrEqual(4.5);
  });

  it("the scrollbar thumb is a 3:1 UI component", () => {
    const thumb = rule(product, ".notes")["scrollbar-color"].split(/\s+/)[0];
    expect(ratio(token(thumb), BG)).toBeGreaterThanOrEqual(3);
  });

  it("the focus ring (--focus) is a 3:1 indicator on --bg", () => {
    expect(ratio(tokens["--focus"], BG)).toBeGreaterThanOrEqual(3);
  });
});
