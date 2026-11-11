// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { StrictMode, useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Dialog } from "./Dialog";
import { Menu } from "./Menu";
import { GHOST_MAX_MS, MOTION } from "./motion";

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
