import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { PRODUCTS } from "./products";

describe("products", () => {
  it("show About exactly for the products Rust bundles a page for (#80)", () => {
    const rust = readFileSync("src-tauri/src/app/news.rs", "utf8");
    const table = rust.slice(rust.indexOf("pub const PRODUCT_PAGES")).split(";")[0];
    const bundled = [...table.matchAll(/\("([\w-]+)",\s*"https:\/\/[^"]+"\)/g)].map((m) => m[1]);
    expect(bundled.length).toBeGreaterThan(0);
    expect(PRODUCTS.filter((p) => p.aboutPage).map((p) => p.id)).toEqual(bundled);
  });
});
