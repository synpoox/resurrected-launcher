import type { HeroProductId } from "./art";
// The one table of "About <product>" pages, also bundled by Rust (`app::news`).
import PRODUCT_PAGES from "./product-pages.json";

/** A product as the sidebar and product page show it. Display copy is provisional (GLOSSARY.md). */
export interface Product {
  /** The catalog's product id. */
  id: string;
  hero: HeroProductId;
  /** Display name, in the sidebar and dialogs. */
  name: string;
  /** Short name for buttons and Patch Notes titles ("PD2R"). */
  short: string;
  /** Ribbon tag above the title. */
  ribbon: string;
  /** The hero title: always exactly these two lines. */
  title: [string, string];
  description: string;
  /** False for products without a release (Forever and 09 today): no tabs, no install. */
  hasRelease: boolean;
  /** Shows "About <product>" (products without a release): the product has an entry in
   *  `product-pages.json`. Forever and 09 have no public repository yet, so they have none. */
  aboutPage: boolean;
}

/** In the spec's sidebar order. */
export const PRODUCTS: readonly Product[] = [
  {
    id: "pd2r",
    hero: "pd2r",
    name: "Project Diablo 2: Resurrected",
    short: "PD2R",
    ribbon: "Playable alpha",
    title: ["The game you know.", "A world reborn."],
    description: "The Project Diablo 2 experience, rebuilt on Diablo II: Resurrected.",
    hasRelease: true,
    aboutPage: "pd2r" in PRODUCT_PAGES,
  },
  {
    id: "project-forever",
    hero: "forever",
    name: "Diablo II: Resurrected Forever",
    short: "Forever",
    ribbon: "Coming soon",
    title: ["A different future.", "A resurrected world."],
    description: "Class changes, legacy items, league mechanics and a reworked endgame.",
    hasRelease: false,
    aboutPage: "project-forever" in PRODUCT_PAGES,
  },
  {
    id: "project-09",
    hero: "09",
    name: "Diablo II: Resurrected 09",
    short: "09",
    ribbon: "Coming soon",
    title: ["An earlier era.", "A world revisited."],
    description: "Lord of Destruction 1.09d, ported to Diablo II: Resurrected 3.3.",
    hasRelease: false,
    aboutPage: "project-09" in PRODUCT_PAGES,
  },
];

export const DEFAULT_PRODUCT_ID = "pd2r";

export function productById(id: string): Product | undefined {
  return PRODUCTS.find((p) => p.id === id);
}
