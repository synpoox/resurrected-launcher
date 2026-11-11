// @vitest-environment jsdom
import { cleanup, render } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";
import { Progress } from "./Progress";

afterEach(cleanup);

const fill = (percent: number | null) => {
  const { container } = render(
    <Progress label="x" percent={percent} detail="" cancelLabel="Cancel" onCancel={() => {}} striped />,
  );
  return container.querySelector<HTMLElement>(".fill")!;
};

describe("Progress fill", () => {
  it.each([
    [0, "0"],
    [50, "0.5"],
    [100, "1"],
    [null, "0"],
  ])("at %s the clip fraction is %s, with no transform to distort the stripes", (percent, p) => {
    const el = fill(percent);
    expect(el.style.getPropertyValue("--p")).toBe(p);
    expect(el.style.transform).toBe("");
  });

  it("the stylesheet clips the full-width fill by --p and never scales it", () => {
    const css = readFileSync("src/product/product.css", "utf8");
    const rule = /\n\.fill \{[^}]*\}/.exec(css)![0];
    expect(rule).toContain("clip-path: inset(0 calc((1 - var(--p)) * 100%) 0 0)");
    expect(rule).not.toContain("scale");
  });
});

describe("table of contents clipping", () => {
  it("the list is clipped only while the panel animates", () => {
    const css = readFileSync("src/product/product.css", "utf8");
    expect(/\.notes-toc ol \{[^}]*overflow/.test(css)).toBe(false);
    expect(/\.notes-toc-panel\.animating ol \{[^}]*overflow: hidden/.test(css)).toBe(true);
  });
});
