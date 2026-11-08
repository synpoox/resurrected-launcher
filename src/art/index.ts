// Placeholder hero art and the mark. Licensed art replaces files here only (spec: Artwork, release gate).
// `?no-inline` keeps small SVGs as bundled files: the CSP (`img-src 'self'`) blocks data: URLs.
import pd2r from "./pd2r.svg?no-inline";
import forever from "./forever.svg?no-inline";
import nine from "./09.svg?no-inline";
import mark from "./mark.svg?no-inline";

export type HeroProductId = "pd2r" | "forever" | "09";

export interface HeroArt {
  src: string; // a bundled module import, never a remote URL
  position: string; // CSS background-position, from the spec's framing table
  size?: string; // CSS background-size; defaults to "cover"
  name: string; // "Placeholder hero" until licensed art lands
  source: string; // shown in Settings → About and credits
  accessibleName: string; // the hero's accessible name
  placeholder: boolean;
}

const placeholder = (src: string, position: string, size?: string): HeroArt => ({
  src,
  position,
  size,
  name: "Placeholder hero",
  source: "Original placeholder made for this launcher",
  accessibleName: "Placeholder hero art (abstract shapes, not game art)",
  placeholder: true,
});

export const HERO_ART: Record<HeroProductId, HeroArt> = {
  pd2r: placeholder(pd2r, "55% center"),
  forever: placeholder(forever, "20% 30%"),
  "09": placeholder(nine, "74% 22%", "auto 135%"),
};

export const MARK_SRC: string = mark;
