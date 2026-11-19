// @vitest-environment jsdom
// Issue 262: each product's hero shows the logo lockup with its extra line, decoratively beside the one h1.
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { HeroCopy } from "./Hero";
import { PRODUCTS } from "../products";

afterEach(cleanup);

it.each(PRODUCTS.map((p) => [p.id, p] as const))("%s renders the logo and its extra line", (_id, product) => {
  render(<HeroCopy product={product} />);
  const mark = screen.getByTestId("wordmark");
  const kids = [...mark.children];
  expect(kids.filter((e) => e.tagName === "IMG")).toHaveLength(1);
  expect(kids.filter((e) => e.tagName === "SPAN").map((e) => e.textContent)).toEqual(
    [product.wordmark.above, product.wordmark.below].filter(Boolean),
  );
  // PROJECT sits above the logo, 09 below it.
  expect(kids[0].tagName === "SPAN").toBe(product.wordmark.above !== undefined);
  expect(mark.querySelector("img")!.getAttribute("alt")).toBe("");
  expect(mark.getAttribute("aria-hidden")).toBe("true");
  expect(screen.getAllByRole("heading")).toHaveLength(1);
});
