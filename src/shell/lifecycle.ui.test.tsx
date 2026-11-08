// @vitest-environment jsdom
// #79: startup, state re-reads, tray requests and the clean-install report, against fixtures.
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ApiContext } from "../api/context";
import { createFixtureApi, type FixtureApi } from "../api/fixtures";
import type { LauncherError } from "../api/types";
import { ToastProvider } from "../ui/Toast";
import { Shell } from "./Shell";

let api: FixtureApi | null = null;

/** Mounts the shell on a scene's state, without the scene's own dialog. */
function mount(key: string, tick = 60_000) {
  api = createFixtureApi(key, { tick });
  const user = userEvent.setup();
  render(
    <ApiContext.Provider value={api}>
      <ToastProvider>
        <Shell />
      </ToastProvider>
    </ApiContext.Provider>,
  );
  return { api, user };
}

const names = (a: FixtureApi) => a.calls.map((c) => c.name);
const count = (a: FixtureApi, name: string) => names(a).filter((n) => n === name).length;
const loaded = () => screen.findByRole("tablist");

beforeEach(() => localStorage.clear());
afterEach(() => {
  cleanup();
  api?.dispose();
  api = null;
});

describe("startup", () => {
  it("recovers the product before reading its state, then refreshes the catalog", async () => {
    const { api } = mount("ready");
    await loaded();
    const calls = names(api);
    expect(calls.indexOf("recoverProduct")).toBeGreaterThanOrEqual(0);
    expect(calls.indexOf("recoverProduct")).toBeLessThan(calls.indexOf("getProductState"));
    expect(calls).toContain("refreshCatalog");
    expect(api.calls.find((c) => c.name === "recoverProduct")!.args).toEqual(["pd2r"]);
  });

  it("Play stays unavailable until recovery answers", async () => {
    let finish!: () => void;
    api = createFixtureApi("ready");
    const recover = api.recoverProduct;
    api.recoverProduct = (id) => new Promise((ok) => (finish = () => ok(recover(id))));
    render(
      <ApiContext.Provider value={api}>
        <ToastProvider>
          <Shell />
        </ToastProvider>
      </ApiContext.Provider>,
    );
    await waitFor(() => expect(finish).toBeDefined());
    expect(screen.queryByRole("button", { name: "Play" })).toBeNull();
    expect(names(api)).not.toContain("getProductState");
    await act(async () => finish());
    expect(await screen.findByRole("button", { name: "Play" })).toBeTruthy();
  });

  it("a tray request during recovery reads no state until recovery answers", async () => {
    let finish!: () => void;
    api = createFixtureApi("blocked");
    const recover = api.recoverProduct;
    api.recoverProduct = (id) => new Promise((ok) => (finish = () => ok(recover(id))));
    render(
      <ApiContext.Provider value={api}>
        <ToastProvider>
          <Shell />
        </ToastProvider>
      </ApiContext.Provider>,
    );
    await waitFor(() => expect(finish).toBeDefined());
    const a = api;
    await act(async () => a.emitTrayRequest({ action: "show_what_happened", product_id: "pd2r", error: null }));
    await act(async () =>
      a.emitProgress({
        product_id: "pd2r",
        kind: "install",
        phase: "downloading",
        boundary: null,
        bytes_done: null,
        bytes_total: null,
        files_done: null,
        files_total: null,
      }),
    );
    expect(names(a)).not.toContain("getProductState");
    await act(async () => finish());
    await loaded();
    expect(names(a).indexOf("recoverProduct")).toBeLessThan(names(a).indexOf("getProductState"));
  });

  it("a failed recovery still shows state", async () => {
    api = createFixtureApi("ready");
    api.recoverProduct = () => Promise.reject({ code: "internal", message: "x" } satisfies LauncherError);
    render(
      <ApiContext.Provider value={api}>
        <ToastProvider>
          <Shell />
        </ToastProvider>
      </ApiContext.Provider>,
    );
    expect(await screen.findByRole("button", { name: "Play" })).toBeTruthy();
  });
});

describe("state re-reads", () => {
  it("re-reads state on operation-progress and operation-outcome", async () => {
    const { api } = mount("downloading");
    await loaded();
    await waitFor(() => expect(count(api, "getProductState")).toBeGreaterThan(0));
    const before = count(api, "getProductState");
    await act(async () => api.emitProgress(api.scene.progress![0]));
    await waitFor(() => expect(count(api, "getProductState")).toBeGreaterThan(before));
    const mid = count(api, "getProductState");
    await act(async () => api.emitOutcome({ status: "cancelled", product_id: "pd2r", kind: "install" }));
    await waitFor(() => expect(count(api, "getProductState")).toBeGreaterThan(mid));
  });

  it("coalesces a burst of events into at most two reads", async () => {
    const { api } = mount("downloading");
    await loaded();
    await waitFor(() => expect(count(api, "getProductState")).toBeGreaterThan(0));
    const before = count(api, "getProductState");
    await act(async () => {
      for (let i = 0; i < 20; i++) api.emitProgress(api.scene.progress![0]);
    });
    await waitFor(() => expect(count(api, "getProductState")).toBeGreaterThan(before));
    expect(count(api, "getProductState") - before).toBeLessThanOrEqual(2);
  });
});

describe("tray requests", () => {
  it("save_breaking opens the save-breaking dialog on that product", async () => {
    const { api, user } = mount("save-breaking");
    await loaded();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await act(async () => api.emitTrayRequest({ action: "save_breaking", product_id: "pd2r", error: null }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("checkbox")).toBeTruthy();
    // The modal hides the sidebar from the accessibility tree, so read it from the DOM.
    expect(document.querySelector(".products [aria-current=page] .item-name")?.textContent).toBe("Project Diablo 2: Resurrected");
  });

  it("the same request twice opens the dialog twice", async () => {
    const { api, user } = mount("save-breaking");
    await loaded();
    await act(async () => api.emitTrayRequest({ action: "save_breaking", product_id: "pd2r", error: null }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Not now" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await act(async () => api.emitTrayRequest({ action: "save_breaking", product_id: "pd2r", error: null }));
    expect(await screen.findByRole("dialog")).toBeTruthy();
  });

  it("show_what_happened opens the product's Overview with its notice and the refusal", async () => {
    const { api, user } = mount("blocked");
    await loaded();
    await user.click(screen.getByRole("tab", { name: "Patch Notes" }));
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const error: LauncherError = { code: "game_build_unsupported", message: "D2R build 3.3.1 isn't supported." };
    await act(async () => api.emitTrayRequest({ action: "show_what_happened", product_id: "pd2r", error }));
    expect((await screen.findByRole("tab", { name: "Overview" })).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByText(/isn't supported by/)).toBeTruthy();
    expect(screen.getByText(error.message)).toBeTruthy();
  });

  it("quit_while_busy opens the Quit dialog; Cancel download and quit cancels, waits, then quits", async () => {
    const { api, user } = mount("downloading", 50);
    await loaded();
    await act(async () => api.emitTrayRequest({ action: "quit_while_busy", product_id: "pd2r", error: null }));
    const dialog = await screen.findByRole("dialog", { name: "Quit while PD2R is downloading?" });
    await user.click(within(dialog).getByRole("button", { name: "Cancel download and quit" }));
    await waitFor(() => expect(names(api)).toContain("quitLauncher"));
    expect(names(api).indexOf("cancelOperation")).toBeLessThan(names(api).indexOf("quitLauncher"));
  });

  it("quit_while_busy: Keep downloading closes the dialog", async () => {
    const { api, user } = mount("downloading");
    await loaded();
    await act(async () => api.emitTrayRequest({ action: "quit_while_busy", product_id: "pd2r", error: null }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Keep downloading" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(names(api)).not.toContain("cancelOperation");
  });
});

describe("quit while busy", () => {
  it("re-renders during a slow stop cancel once, then quit after the cancelled outcome", async () => {
    const { api, user } = mount("downloading");
    await loaded();
    let cancels = 0;
    let quits = 0;
    api.cancelOperation = () => {
      cancels += 1;
      return Promise.resolve();
    };
    api.quitLauncher = () => {
      quits += 1;
      return Promise.resolve();
    };
    await act(async () => api.emitTrayRequest({ action: "quit_while_busy", product_id: "pd2r", error: null }));
    const dialog = await screen.findByRole("dialog", { name: "Quit while PD2R is downloading?" });
    await user.click(within(dialog).getByRole("button", { name: "Cancel download and quit" }));
    await waitFor(() => expect(cancels).toBe(1));
    // Live progress keeps re-rendering the shell while the stop waits.
    for (let i = 0; i < 10; i++) {
      await act(async () => api.emitProgress(api.scene.progress![0]));
    }
    await waitFor(() => expect(count(api, "getProductState")).toBeGreaterThan(2));
    expect(cancels).toBe(1);
    expect(quits).toBe(0);
    expect(within(dialog).getByRole("button", { name: "Stopping safely…" })).toBeTruthy();
    await act(async () => api.emitOutcome({ status: "cancelled", product_id: "pd2r", kind: "install" }));
    await waitFor(() => expect(quits).toBe(1));
    expect(cancels).toBe(1);
  });
});

describe("quit when the operation already ended", () => {
  const refused = () => Promise.reject({ code: "invalid_request", message: "pd2r has no operation running" } satisfies LauncherError);

  it("a cancel refused because nothing is running goes on to quit", async () => {
    const { api, user } = mount("downloading");
    await loaded();
    await act(async () => api.emitTrayRequest({ action: "quit_while_busy", product_id: "pd2r", error: null }));
    const dialog = await screen.findByRole("dialog", { name: "Quit while PD2R is downloading?" });
    const read = api.getProductState;
    api.getProductState = async (id) => ({ ...(await read(id)), running: null });
    api.cancelOperation = refused;
    let quits = 0;
    api.quitLauncher = () => {
      quits += 1;
      return Promise.resolve();
    };
    await user.click(within(dialog).getByRole("button", { name: "Cancel download and quit" }));
    await waitFor(() => expect(quits).toBe(1));
    expect(screen.queryByText(/no operation running/)).toBeNull();
  });

  it("a cancel refused while still running shows the error and lets the player try again", async () => {
    const { api, user } = mount("downloading");
    await loaded();
    await act(async () => api.emitTrayRequest({ action: "quit_while_busy", product_id: "pd2r", error: null }));
    const dialog = await screen.findByRole("dialog", { name: "Quit while PD2R is downloading?" });
    api.cancelOperation = refused;
    await user.click(within(dialog).getByRole("button", { name: "Cancel download and quit" }));
    expect(await screen.findByText(/no operation running/)).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Cancel download and quit" })).toBeTruthy();
    expect(names(api)).not.toContain("quitLauncher");
  });
});

describe("quit refused", () => {
  it("a refused quit shows why and re-enables both buttons", async () => {
    const { api, user } = mount("downloading");
    await loaded();
    await act(async () => api.emitTrayRequest({ action: "quit_while_busy", product_id: "pd2r", error: null }));
    const dialog = await screen.findByRole("dialog", { name: "Quit while PD2R is downloading?" });
    api.cancelOperation = () => Promise.resolve();
    api.quitLauncher = () =>
      Promise.reject({ code: "recovery_required", message: "Recovery is still running." } satisfies LauncherError);
    await user.click(within(dialog).getByRole("button", { name: "Cancel download and quit" }));
    await act(async () => api.emitOutcome({ status: "cancelled", product_id: "pd2r", kind: "install" }));
    expect(await screen.findByText("Recovery is still running.")).toBeTruthy();
    const again = within(dialog).getByRole("button", { name: "Cancel download and quit" });
    expect(again.hasAttribute("disabled")).toBe(false);
    expect(within(dialog).getByRole("button", { name: "Keep downloading" }).hasAttribute("disabled")).toBe(false);
  });
});

describe("About in live mode", () => {
  const renderForever = (showAbout?: boolean) => {
    api = createFixtureApi("coming-soon");
    render(
      <ApiContext.Provider value={api}>
        <ToastProvider>
          <Shell product="project-forever" showAbout={showAbout} />
        </ToastProvider>
      </ApiContext.Provider>,
    );
  };

  it("is hidden when the shell is told to (live mode until #80)", async () => {
    renderForever(false);
    expect(await screen.findByText("Coming soon")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "About Forever" })).toBeNull();
  });

  it("is shown by default (fixtures)", async () => {
    renderForever();
    expect(await screen.findByRole("button", { name: "About Forever" })).toBeTruthy();
  });
});

describe("outcome report", () => {
  it("keeps clean-install backup locations that arrive while the product page isn't shown", async () => {
    const { api, user } = mount("ready");
    await loaded();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const backup = String.raw`C:\Games\D2R\mods\pd2r.backup-2`;
    await act(async () =>
      api.emitOutcome({
        status: "succeeded",
        product_id: "pd2r",
        kind: "clean_install",
        report: { preserved: [], remaining: [], clean_install_backups: [backup] },
      }),
    );
    await user.click(screen.getByRole("button", { name: "Project Diablo 2: Resurrected" }));
    expect(await screen.findByText(backup)).toBeTruthy();
  });

  it("shows clean-install backup locations", async () => {
    const { api, user } = mount("ready");
    await loaded();
    const backup = String.raw`C:\Games\D2R\mods\pd2r.backup-1`;
    await act(async () =>
      api.emitOutcome({
        status: "succeeded",
        product_id: "pd2r",
        kind: "clean_install",
        report: { preserved: [], remaining: [], clean_install_backups: [backup] },
      }),
    );
    expect(await screen.findByText(backup)).toBeTruthy();
    expect(screen.getByText(/Clean install finished/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText(backup)).toBeNull();
  });
});
