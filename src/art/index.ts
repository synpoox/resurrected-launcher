// Hero art and the mark (spec: Artwork). Hero images are official D2R promotional art, bundled locally
// (the CSP is `img-src 'self'`).
// `?no-inline` keeps files bundled: the CSP blocks data: URLs.
import pd2r from "./pd2r.webp?no-inline";
import forever from "./forever.webp?no-inline";
import nine from "./09.webp?no-inline";
import mark from "./mark.svg?no-inline";
import logo from "./d2r-logo.webp?no-inline";

export type HeroProductId = "pd2r" | "forever" | "09";

export interface HeroArt {
  src: string; // a bundled module import, never a remote URL
  position: string; // CSS background-position, from the spec's framing table
  size?: string; // CSS background-size; defaults to "cover"
  accessibleName: string; // the hero's accessible name
}

const official = (src: string, accessibleName: string, position: string, size?: string): HeroArt => ({
  src,
  position,
  size,
  accessibleName,
});

export const HERO_ART: Record<HeroProductId, HeroArt> = {
  pd2r: official(
    pd2r,
    "The Warlock seated on a throne in official Diablo II: Resurrected artwork",
    "55% center",
  ),
  forever: official(
    forever,
    "Official Diablo II: Resurrected Paladin promotional artwork",
    "74% 22%",
    "auto 135%",
  ),
  "09": official(nine, "Official Diablo II: Resurrected Assassin and Druid artwork", "10% 0%"),
};

export const MARK_SRC: string = mark;

export const LOGO_SRC: string = logo;
