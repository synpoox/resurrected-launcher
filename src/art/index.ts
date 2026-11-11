// Hero art and the mark (spec: Artwork). Hero images are official D2R promotional art, bundled locally
// (the CSP is `img-src 'self'`) and credited in Settings → About and credits, matching resurrected-web's credit.
// `?no-inline` keeps files bundled: the CSP blocks data: URLs.
import pd2r from "./pd2r.webp?no-inline";
import forever from "./forever.webp?no-inline";
import nine from "./09.webp?no-inline";
import mark from "./mark.svg?no-inline";

export type HeroProductId = "pd2r" | "forever" | "09";

export interface HeroArt {
  src: string; // a bundled module import, never a remote URL
  position: string; // CSS background-position, from the spec's framing table
  size?: string; // CSS background-size; defaults to "cover"
  name: string; // what the image is
  source: string; // shown in Settings → About and credits
  accessibleName: string; // the hero's accessible name
  placeholder: boolean;
}

export const ART_SOURCE = "Official D2R promotional artwork © Blizzard Entertainment";

const official = (src: string, name: string, accessibleName: string, position: string, size?: string): HeroArt => ({
  src,
  position,
  size,
  name,
  source: ART_SOURCE,
  accessibleName,
  placeholder: false,
});

export const HERO_ART: Record<HeroProductId, HeroArt> = {
  pd2r: official(
    pd2r,
    "Reign of the Warlock key art",
    "The Warlock seated on a throne in official Diablo II: Resurrected artwork",
    "55% center",
  ),
  forever: official(
    forever,
    "30th Anniversary key art",
    "Diablo before a glowing arcane circle in the official Diablo II: Resurrected 30th Anniversary key art",
    "20% 30%",
  ),
  "09": official(
    nine,
    "Paladin promotional art",
    "Official Diablo II: Resurrected Paladin promotional artwork",
    "74% 22%",
    "auto 135%",
  ),
};

export const MARK_SRC: string = mark;
