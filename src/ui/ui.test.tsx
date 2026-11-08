// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Notice } from "./Notice";
import { Switch } from "./Switch";
import { TOAST_MS, ToastProvider, useToast, type ToastInput } from "./Toast";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function Shower({ toast }: { toast: ToastInput }) {
  const { show } = useToast();
  return (
    <button type="button" onClick={() => show(toast)}>
      show
    </button>
  );
}

describe("Toast", () => {
  it("information auto-dismisses; errors stay until dismissed", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(
      <ToastProvider>
        <Shower toast={{ tone: "info", text: "Saved" }} />
      </ToastProvider>,
    );
    await user.click(screen.getByText("show"));
    expect(screen.getByRole("status").textContent).toMatch(/Saved/);
    act(() => void vi.advanceTimersByTime(TOAST_MS + 10));
    expect(screen.queryByText("Saved")).toBeNull();
    cleanup();

    const onRetry = vi.fn();
    render(
      <ToastProvider>
        <Shower toast={{ tone: "error", text: "Failed", actions: [{ label: "Try again", onSelect: onRetry }] }} />
      </ToastProvider>,
    );
    await user.click(screen.getByText("show"));
    act(() => void vi.advanceTimersByTime(TOAST_MS * 3));
    expect(screen.getByRole("alert").textContent).toMatch(/Failed/);
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(onRetry).toHaveBeenCalled();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("Switch", () => {
  function Harness({ reason }: { reason?: string }) {
    const [on, setOn] = useState(false);
    return <Switch label="Start with Windows" checked={on} onChange={setOn} disabledReason={reason} />;
  }

  it("is a switch that Space toggles", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const sw = screen.getByRole("switch", { name: "Start with Windows" });
    expect(sw.getAttribute("aria-checked")).toBe("false");
    sw.focus();
    await user.keyboard(" ");
    expect(sw.getAttribute("aria-checked")).toBe("true");
  });

  it("when disabled, says why next to it", () => {
    render(<Harness reason="Quit the game to change this" />);
    const sw = screen.getByRole("switch") as HTMLButtonElement;
    expect(sw.disabled).toBe(true);
    expect(document.getElementById(sw.getAttribute("aria-describedby")!)!.textContent).toBe("Quit the game to change this");
  });
});

describe("Notice", () => {
  it("errors are alerts; information is status", () => {
    render(<Notice tone="error">Broken</Notice>);
    expect(screen.getByRole("alert").className).toContain("error");
  });
});
