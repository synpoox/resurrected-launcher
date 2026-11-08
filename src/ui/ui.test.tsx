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

  it("dismissLabel replaces Dismiss; onDismiss says who closed it, and actions don't call it", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const onDismiss = vi.fn();
    const onGo = vi.fn();
    const toast: ToastInput = {
      tone: "info",
      text: "Pending",
      actions: [{ label: "Go", onSelect: onGo }],
      dismissLabel: "Later",
      onDismiss,
    };
    render(
      <ToastProvider>
        <Shower toast={toast} />
      </ToastProvider>,
    );
    await user.click(screen.getByText("show"));
    expect(screen.queryByRole("button", { name: "Dismiss" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Later" }));
    expect(onDismiss).toHaveBeenCalledWith("player");

    await user.click(screen.getByText("show"));
    act(() => void vi.advanceTimersByTime(TOAST_MS + 10));
    expect(onDismiss).toHaveBeenLastCalledWith("timeout");
    expect(screen.queryByText("Pending")).toBeNull();

    onDismiss.mockClear();
    await user.click(screen.getByText("show"));
    await user.click(screen.getByRole("button", { name: "Go" }));
    expect(onGo).toHaveBeenCalled();
    expect(onDismiss).not.toHaveBeenCalled();
    expect(screen.queryByText("Pending")).toBeNull();
  });

  it("a disabled action stays focusable, does nothing, and is described by the toast's text", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <ToastProvider>
        <Shower toast={{ tone: "info", text: "Held for now", autoDismiss: false, actions: [{ label: "Restart", onSelect, disabled: true }] }} />
      </ToastProvider>,
    );
    await user.click(screen.getByText("show"));
    const restart = screen.getByRole("button", { name: "Restart" }) as HTMLButtonElement;
    expect(restart.getAttribute("aria-disabled")).toBe("true");
    expect(document.getElementById(restart.getAttribute("aria-describedby")!)?.textContent).toBe("Held for now");
    await user.click(restart);
    expect(onSelect).not.toHaveBeenCalled();
    expect(screen.getByText("Held for now")).toBeTruthy();
    restart.focus();
    await user.keyboard("{Enter}");
    expect(onSelect).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(restart);
  });

  it("update() that disables or relabels a focused action keeps focus on it", async () => {
    const user = userEvent.setup();
    function Updater() {
      const { show, update } = useToast();
      const [id, setId] = useState<number | null>(null);
      const input = (label: string, disabled: boolean): ToastInput => ({
        tone: "info",
        text: "Ready",
        autoDismiss: false,
        actions: [{ label, onSelect: () => {}, disabled }],
      });
      return (
        <>
          <button type="button" onClick={() => setId(show(input("Restart", false)))}>
            show
          </button>
          <button type="button" onClick={() => id !== null && update(id, input("Restart later", true))}>
            hold
          </button>
        </>
      );
    }
    render(
      <ToastProvider>
        <Updater />
      </ToastProvider>,
    );
    await user.click(screen.getByText("show"));
    const action = screen.getByRole("button", { name: "Restart" });
    action.focus();
    act(() => screen.getByText("hold").click());
    expect(document.activeElement).toBe(action);
    expect(action.textContent).toBe("Restart later");
    expect(action.getAttribute("aria-disabled")).toBe("true");
  });

  it("update() replaces an open toast in place and ignores a closed one", async () => {
    const user = userEvent.setup();
    function Updater() {
      const { show, update } = useToast();
      const [id, setId] = useState<number | null>(null);
      return (
        <>
          <button type="button" onClick={() => setId(show({ tone: "info", text: "One", autoDismiss: false }))}>
            show
          </button>
          <button type="button" onClick={() => id !== null && update(id, { tone: "info", text: "Two", autoDismiss: false })}>
            update
          </button>
        </>
      );
    }
    render(
      <ToastProvider>
        <Updater />
      </ToastProvider>,
    );
    await user.click(screen.getByText("show"));
    await user.click(screen.getByText("update"));
    expect(screen.getAllByRole("status").map((s) => s.querySelector("p")?.textContent)).toEqual(["Two"]);
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    await user.click(screen.getByText("update"));
    expect(screen.queryByRole("status")).toBeNull();
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
