// @vitest-environment jsdom
// Slice 4 gate (#81), clause "Progress, errors and retry", in the window: an injected download failure
// shows an error and trying again succeeds; cancel during `downloading` and during `applying` ends in
// a safe state; byte progress reaches 100%; `startup_failed` shows its exit code. The native side of
// the same clause (real download, real cancel, real exit code) is src-tauri/tests/slice4_gate.
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { ProgressEvent } from "../api/types";
import { disposeAll, mount } from "./gate";

beforeEach(() => localStorage.clear());
afterEach(() => {
  cleanup();
  disposeAll();
});

const GB = 1024 ** 3;
/** How long the fixture's safe stop takes: long enough to see `cancelling` on a busy runner. */
const SAFE_STOP_MS = 1500;
const downloading = (done: number, total: number): ProgressEvent => ({
  product_id: "pd2r",
  kind: "install",
  phase: "downloading",
  boundary: null,
  bytes_done: done,
  bytes_total: total,
  files_done: null,
  files_total: null,
});

describe("progress, errors and retry", () => {
  it("an injected download failure shows an error, and Install again succeeds", async () => {
    let failNext = true;
    const { api, user } = mount("install", {
      tick: 5,
      prepare: (a) => {
        const real = a.startOperation.bind(a);
        a.startOperation = (request, inspection) => {
          if (!failNext) return real(request, inspection);
          failNext = false;
          a.calls.push({ name: "startOperation", args: [request, inspection] });
          // The download fails at once: the operation ends with `download_failed`, nothing installed.
          queueMicrotask(() =>
            a.emitOutcome({
              status: "failed",
              product_id: "pd2r",
              kind: "install",
              error: { code: "download_failed", message: "The download failed: connection reset. Try again." },
            }),
          );
          return Promise.resolve();
        };
      },
    });
    await user.click(await screen.findByRole("button", { name: /^Install$/ }));
    const toast = await screen.findByText("The download failed: connection reset. Try again.");
    expect(toast.closest("[role=alert], .toast")).not.toBeNull();
    // Still offering Install: the failure left nothing half-installed.
    const again = await screen.findByRole("button", { name: /^Install$/ });
    await user.click(again);
    await screen.findByRole("progressbar");
    expect(api.calls.filter((c) => c.name === "startOperation")).toHaveLength(2);
    // The retried download runs to its outcome.
    act(() =>
      api.emitOutcome({
        status: "succeeded",
        product_id: "pd2r",
        kind: "install",
        report: { preserved: [], remaining: [], clean_install_backups: [] },
      }),
    );
  });

  it("byte progress reaches 100%", async () => {
    const { api } = mount("downloading");
    await screen.findByRole("progressbar");
    act(() => api.emitProgress(downloading(0.6 * GB, 1.2 * GB)));
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("50");
    act(() => api.emitProgress(downloading(1.2 * GB, 1.2 * GB)));
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("100");
    // The action row and the sidebar badge both read 100%.
    expect(screen.getAllByText("100%").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByLabelText("100% done")).toBeTruthy();
  });

  it("cancel during downloading stops safely and returns to Install", async () => {
    const { api, user } = mount("downloading", { tick: SAFE_STOP_MS });
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(api.calls.some((c) => c.name === "cancelOperation")).toBe(true);
    // Safe stop: Cancel disabled with the reason, then the previous (no) installation.
    await screen.findByText("Stopping safely…");
    expect((screen.getByRole("button", { name: "Cancel" }) as HTMLButtonElement).disabled).toBe(true);
    expect(await screen.findByRole("button", { name: /^Install$/ }, { timeout: 5 * SAFE_STOP_MS })).toBeTruthy();
  });

  it("cancel during applying waits for the safe stop", async () => {
    const { api, user } = mount("applying", { tick: SAFE_STOP_MS });
    await screen.findByText(/Cancelling now waits until it's safe to stop, or rolls back/);
    await user.click(screen.getByRole("button", { name: "Cancel safely" }));
    expect(api.calls.filter((c) => c.name === "cancelOperation")).toHaveLength(1);
    await screen.findByText("Stopping safely…");
    expect(screen.getByText(/Restoring the previous files\. Don't close the launcher\./)).toBeTruthy();
    expect(await screen.findByRole("button", { name: /^Install$/ }, { timeout: 5 * SAFE_STOP_MS })).toBeTruthy();
  });

  it("startup_failed shows its exit code, and Try again plays", async () => {
    const { api, user } = mount("startup-failed");
    const notice = await screen.findByText(/exit code 1/);
    expect(notice).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(api.calls.some((c) => c.name === "playProduct")).toBe(true));
  });

  it("startup_failed without an exit code says nothing about one", async () => {
    mount("startup-failed", { prepare: (a) => void (a.scene.state.startup_failed = { exit_code: null }) });
    const tablist = await screen.findByRole("tablist");
    expect(within(tablist).getByRole("tab", { name: /Overview/ })).toBeTruthy();
    await screen.findByRole("button", { name: "Try again" });
    expect(screen.queryByText(/exit code/)).toBeNull();
  });
});
