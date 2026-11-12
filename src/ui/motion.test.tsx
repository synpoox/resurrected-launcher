// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { StrictMode, useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Dialog } from "./Dialog";
import { Menu } from "./Menu";
import { HeroArt } from "../product/Hero";
import { PRODUCTS } from "../products";
import { Ghosted, GHOST_MAX_MS, MOTION } from "./motion";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const motion = (reduce: boolean) =>
  vi.stubGlobal("matchMedia", (q: string) => ({ matches: reduce && q.includes("reduce"), media: q }));

const ghosts = () => document.querySelectorAll(".ghost-out");

function Opener() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        open
      </button>
      {open && (
        <Dialog title="Settings" onClose={() => setOpen(false)} actions={<button type="button">Done</button>}>
          <p>body</p>
        </Dialog>
      )}
    </>
  );
}

describe("motion tokens", () => {
  it("the JS durations equal the CSS tokens", () => {
    const css = readFileSync("src/tokens.css", "utf8");
    const ms = (name: string) => Number(new RegExp(`--dur-${name}:\\s*(\\d+)ms`).exec(css)?.[1]);
    expect(MOTION).toEqual({ fast: ms("fast"), base: ms("base"), slow: ms("slow") });
  });
});

function Swap() {
  const [a, setA] = useState(true);
  return (
    <div data-testid="stage">
      <button type="button" onClick={() => setA(false)}>
        swap
      </button>
      {a ? (
        <Ghosted key="a">
          <input type="radio" name="d2r-location" defaultChecked />
        </Ghosted>
      ) : (
        <Ghosted key="b">
          <input type="radio" name="d2r-location" defaultChecked />
        </Ghosted>
      )}
    </div>
  );
}


describe("ghost copies of a view", () => {
  it("a view's copy sits beneath the live view that replaced it, nameless so it can't touch a live radio group", async () => {
    motion(false);
    render(<Swap />);
    await userEvent.setup().click(screen.getByText("swap"));
    await act(async () => {});
    const stage = screen.getByTestId("stage");
    const copy = stage.querySelector(".ghost-out")!;
    const live = [...stage.children].find((c) => c !== copy && c.tagName === "DIV")!;
    expect(copy).toBeTruthy();
    // Earlier in the DOM paints beneath its absolutely positioned sibling.
    expect(copy.compareDocumentPosition(live) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(copy.querySelector("[name]")).toBeNull();
    // The live radio is still checked in its group.
    expect((live.querySelector("input") as HTMLInputElement).checked).toBe(true);
    expect((live.querySelector("input") as HTMLInputElement).name).toBe("d2r-location");
  });

  it("a dialog's copy stays last, above the page", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    motion(false);
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(<Opener />);
    await user.click(screen.getByText("open"));
    await user.keyboard("{Escape}");
    await act(async () => {});
    const copy = document.querySelector(".ghost-out")!;
    expect(copy.parentElement!.lastElementChild).toBe(copy);
  });
});

describe("view switches are never see-through", () => {
  const read = (f: string) => readFileSync(f, "utf8");
  // The body of the first rule/at-rule whose head is `head` (no nested braces beyond keyframe steps).
  const block = (css: string, head: string) => {
    const at = css.indexOf(`${head} {`);
    if (at < 0) return "";
    let depth = 0;
    for (let i = css.indexOf("{", at); i < css.length; i++) {
      if (css[i] === "{") depth++;
      if (css[i] === "}" && --depth === 0) return css.slice(css.indexOf("{", at), i);
    }
    return "";
  };

  it("the tab keyframes move by transform only", () => {
    const css = read("src/styles.css");
    for (const name of ["slide-from-right", "slide-from-left"]) {
      const body = block(css, `@keyframes ${name}`);
      expect(body, name).toContain("transform");
      expect(body, name).not.toContain("opacity");
    }
  });

  it("tab panels are opaque; a product view has no entrance animation; the hero art crossfades over opaque art", () => {
    const css = read("src/product/product.css");
    expect(block(css, ".view")).not.toContain("animation");
    expect(block(css, ".art")).not.toContain("animation");
    expect(block(css, ".art.art-in")).toMatch(/animation: fade var\(--dur-slow\)/);
    expect(block(read("src/styles.css"), "@keyframes fade")).toMatch(/opacity: 0/);
    expect(css).toMatch(/^\.tabpanel \{\s*background:\s*var\(--bg\)/m);
    expect(block(css, ".tabpanel.enter-notes")).toContain("slide-from-right");
  });

  it("a left view stays fully opaque (no animation) until the new one has arrived, then goes", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    motion(false);
    render(<Swap />);
    await userEvent.setup({ advanceTimers: vi.advanceTimersByTime }).click(screen.getByText("swap"));
    await act(async () => {});
    const copy = document.querySelector(".ghost-out")!;
    expect(copy.classList.contains("ghost-hold")).toBe(true);
    expect(block(read("src/styles.css"), ".ghost-out.ghost-hold")).toContain("animation: none");
    expect(block(read("src/styles.css"), ".ghost-out.ghost-hold")).toContain("isolation: isolate");
    act(() => void vi.advanceTimersByTime(MOTION.base - 100));
    expect(ghosts()).toHaveLength(1);
    act(() => void vi.advanceTimersByTime(120));
    expect(ghosts()).toHaveLength(0);
  });
});

describe("product switch: the hero art crossfades, nothing else animates", () => {
  function Pick() {
    const [i, setI] = useState(0);
    return (
      <div>
        <button type="button" onClick={() => setI(1)}>
          next
        </button>
        <HeroArt key={PRODUCTS[i].id} product={PRODUCTS[i]} />
      </div>
    );
  }

  it("the new art fades in over the old art, which stays beneath, opaque and unanimated", async () => {
    motion(false);
    const { container } = render(<Pick />);
    expect(container.querySelectorAll(".art")).toHaveLength(1);
    expect(container.querySelector(".art-in")).toBeNull();
    await userEvent.setup().click(screen.getByText("next"));
    const arts = [...container.querySelectorAll(".art")];
    expect(arts).toHaveLength(2);
    // Old art first (beneath, no animation class); the new one on top fades in.
    expect(arts[0].classList.contains("art-in")).toBe(false);
    expect(arts[1].classList.contains("art-in")).toBe(true);
    expect(arts[0].getAttribute("aria-hidden")).toBe("true");
    // The fade ends: the old layer goes.
    fireEvent.animationEnd(arts[1]);
    expect(container.querySelectorAll(".art")).toHaveLength(1);
  });

  it("under reduced motion the art swaps at once", async () => {
    motion(true);
    const { container } = render(<Pick />);
    await userEvent.setup().click(screen.getByText("next"));
    expect(container.querySelectorAll(".art")).toHaveLength(1);
    expect(container.querySelector(".art-in")).toBeNull();
  });
});

describe("exit animations", () => {
  it("a closed dialog is gone for the user at once and leaves an inert copy that fades, then goes", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    motion(false);
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(<Opener />);
    await user.click(screen.getByText("open"));
    expect(ghosts()).toHaveLength(0);
    await user.keyboard("{Escape}");
    // The real dialog is unmounted: it can't be reached, and the opener has its focus back.
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(screen.getByText("open"));
    await act(async () => {});
    const copy = ghosts();
    expect(copy).toHaveLength(1);
    expect(copy[0].hasAttribute("inert")).toBe(true);
    expect(copy[0].getAttribute("aria-hidden")).toBe("true");
    expect(copy[0].querySelector("[id]")).toBeNull();
    act(() => void vi.advanceTimersByTime(GHOST_MAX_MS + 10));
    expect(ghosts()).toHaveLength(0);
  });

  it("under reduced motion nothing is left behind", async () => {
    motion(true);
    const user = userEvent.setup();
    render(<Opener />);
    await user.click(screen.getByText("open"));
    await user.keyboard("{Escape}");
    await act(async () => {});
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(ghosts()).toHaveLength(0);
  });

  it("Strict Mode's mount, unmount, mount does not leave a copy of an open dialog", async () => {
    motion(false);
    render(
      <StrictMode>
        <Opener />
      </StrictMode>,
    );
    await userEvent.setup().click(screen.getByText("open"));
    await act(async () => {});
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(ghosts()).toHaveLength(0);
  });

  it("a closed menu leaves an inert copy, and choosing an item acts at once", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    motion(false);
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const onSelect = vi.fn();
    render(<Menu label="Manage" items={[{ label: "Repair", onSelect }]} />);
    await user.click(screen.getByRole("button", { name: "Manage" }));
    await user.click(screen.getByRole("menuitem", { name: "Repair" }));
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).toBeNull();
    await act(async () => {});
    expect(ghosts()).toHaveLength(1);
    expect(ghosts()[0].classList.contains("menu")).toBe(true);
    act(() => void vi.advanceTimersByTime(GHOST_MAX_MS + 10));
    expect(ghosts()).toHaveLength(0);
  });
});

describe("text selection (#230)", () => {
  const css = readFileSync("src/styles.css", "utf8");
  // Every top-level block for the selector, joined (body has more than one).
  const rule = (selector: string) =>
    [...css.matchAll(new RegExp(`(?:^|\n)${selector} {([^}]*)}`, "g"))].map((m) => m[1]).join("\n");

  it("is off app-wide on body, so portals are covered too", () => {
    expect(rule("body")).toContain("user-select: none");
    expect(readFileSync("src/shell/shell.css", "utf8")).not.toContain("user-select");
  });

  it("stays on for text entry", () => {
    const entry = css.match(/input,\s*textarea,\s*\[contenteditable\][^{]*\{([^}]*)\}/)?.[1] ?? "";
    expect(entry).toContain("user-select: text");
  });
});
