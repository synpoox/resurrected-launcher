// @vitest-environment jsdom
// Issue 138: action buttons are busy from the click until the result or dialog arrives, never
// submit twice, and consent dialogs open at once with a loading state while inspection runs.
import { act, cleanup, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiContext } from "../api/context";
import { createFixtureApi, sceneByKey, type FixtureApi } from "../api/fixtures";
import type { Inspection } from "../api/types";
import { Shell } from "../shell/Shell";
import { BUSY_SAFETY_MS, usePending } from "../ui/pending";
import { ToastProvider } from "../ui/Toast";
import { SelfUpdateProvider } from "../selfupdate/SelfUpdate";
import { createSelfUpdateFixture } from "../selfupdate/api";

let api: FixtureApi | null = null;

function mount(key: string) {
  api = createFixtureApi(key, { tick: 60_000 });
  const scene = sceneByKey(key)!;
  const user = userEvent.setup();
  render(
    <ApiContext.Provider value={api}>
      <ToastProvider>
        <Shell product={scene.product} open={scene.open} />
      </ToastProvider>
    </ApiContext.Provider>,
  );
  return { api, user };
}

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

/** Replaces one API method with a held answer, counting calls; `release` lets the real one answer. */
type Held = "inspectOperation" | "startOperation" | "playProduct" | "cancelOperation" | "openFolder" | "selectGameLocation" | "chooseGameLocation";
function hold<K extends Held>(a: FixtureApi, name: K) {
  const real = a[name].bind(a) as (...args: unknown[]) => Promise<unknown>;
  const gate = deferred<void>();
  const calls: unknown[][] = [];
  (a as unknown as Record<string, unknown>)[name] = (...args: unknown[]) => {
    calls.push(args);
    return gate.promise.then(() => real(...args));
  };
  return { calls, release: () => gate.resolve() };
}

const busy = (b: HTMLElement) => b.getAttribute("aria-busy") === "true";
const blocked = (b: HTMLElement) => b.getAttribute("aria-disabled") === "true";
const loaded = () => screen.findByRole("tablist");

beforeEach(() => localStorage.clear());
afterEach(() => {
  cleanup();
  api?.dispose();
  api = null;
});

describe("pending action buttons (issue 138)", () => {
  it("Install is busy until the start answers, and a second click doesn't start twice", async () => {
    const { api, user } = mount("install");
    await screen.findByText(/free space needed/);
    const start = hold(api, "startOperation");
    const install = screen.getByRole("button", { name: "Install" });
    await user.click(install);
    await waitFor(() => expect(start.calls).toHaveLength(1));
    expect(busy(install)).toBe(true);
    expect(blocked(install)).toBe(true);
    expect(install.querySelector(".spinner")).not.toBeNull();
    // Focus stays on the button: it is aria-disabled, not disabled.
    expect(document.activeElement).toBe(install);
    await user.click(install);
    expect(start.calls).toHaveLength(1);
    start.release();
    await screen.findByRole("progressbar");
  });

  it("Play is busy until the game starts, and the other buttons in the row wait too", async () => {
    const { api, user } = mount("update");
    await loaded();
    const play = hold(api, "playProduct");
    const update = screen.getByRole("button", { name: "Update" });
    const playOld = screen.getByRole("button", { name: /^Play / });
    await user.click(playOld);
    expect(busy(playOld)).toBe(true);
    expect(blocked(update)).toBe(true);
    expect(busy(update)).toBe(false);
    await user.click(update);
    await user.click(playOld);
    expect(play.calls).toHaveLength(1);
    expect(api.calls.filter((c) => c.name === "inspectOperation")).toHaveLength(0);
    play.release();
    expect(await screen.findByRole("button", { name: "Playing" })).toBeTruthy();
  });

  it("Cancel is busy until the cancel request answers, and sends one request", async () => {
    const { api, user } = mount("downloading");
    const cancel = await screen.findByRole("button", { name: "Cancel" });
    const held = hold(api, "cancelOperation");
    await user.click(cancel);
    await user.click(cancel);
    expect(held.calls).toHaveLength(1);
    expect(busy(cancel)).toBe(true);
    held.release();
    await waitFor(() => expect(busy(screen.getByRole("button", { name: "Cancel" }))).toBe(false));
  });

  it("Update waits busy for its inspection before a save-breaking dialog can open", async () => {
    const { api, user } = mount("update");
    await loaded();
    const inspect = hold(api, "inspectOperation");
    const update = screen.getByRole("button", { name: "Update" });
    await user.click(update);
    await user.click(update);
    expect(inspect.calls).toHaveLength(1);
    expect(busy(update)).toBe(true);
    inspect.release();
    await screen.findByRole("progressbar");
  });
});

describe("consent dialogs open on the click (issue 138)", () => {
  it("Clean install… opens its dialog at once, loading until the inspection answers", async () => {
    const { api, user } = mount("clean-install");
    // The scene opens the dialog itself; close it to take the button's path.
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Not now" }));
    const inspect = hold(api, "inspectOperation");
    await user.click(screen.getByRole("button", { name: "Clean install…" }));
    const dialog = screen.getByRole("dialog", { name: /Replace the PD2R files/ });
    expect(within(dialog).getByRole("status").textContent).toMatch(/Checking the PD2R files/);
    const confirm = within(dialog).getByRole("button", { name: "Clean install" });
    expect(blocked(confirm)).toBe(true);
    await user.click(confirm);
    expect(api.calls.filter((c) => c.name === "startOperation")).toHaveLength(0);
    inspect.release();
    expect(await within(dialog).findByText(/doesn't match alpha 0\.4\.2/)).toBeTruthy();
    expect(blocked(confirm)).toBe(false);
    await user.click(confirm);
    await waitFor(() => expect(api.calls.some((c) => c.name === "startOperation")).toBe(true));
    const start = api.calls.find((c) => c.name === "startOperation")!;
    expect(start.args[0]).toMatchObject({ kind: "clean_install", consent_clean_install: true });
    expect((start.args[1] as Inspection).token).toBe("clean_install-token");
  });

  it("Not now while the inspection runs keeps the dialog closed when it answers", async () => {
    const { api, user } = mount("clean-install");
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Not now" }));
    const inspect = hold(api, "inspectOperation");
    await user.click(screen.getByRole("button", { name: "Clean install…" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Not now" }));
    // The page's actions are free at once, before the inspection answers.
    const again = screen.getByRole("button", { name: "Clean install…" });
    expect(busy(again)).toBe(false);
    expect(blocked(again)).toBe(false);
    inspect.release();
    await waitFor(() => expect(inspect.calls).toHaveLength(1));
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("Uninstall… opens its dialog at once, and Uninstall is busy until it answers", async () => {
    const { api, user } = mount("ready");
    await loaded();
    const inspect = hold(api, "inspectOperation");
    await user.click(screen.getByRole("button", { name: "Manage PD2R" }));
    await user.click(screen.getByRole("menuitem", { name: "Uninstall…" }));
    const dialog = screen.getByRole("dialog", { name: /Uninstall Project Diablo 2/ });
    expect(within(dialog).getByRole("status").textContent).toMatch(/Checking PD2R's files/);
    const confirm = within(dialog).getByRole("button", { name: "Uninstall" });
    expect(blocked(confirm)).toBe(true);
    // Waiting for the inspection isn't the confirm's own action: no spinner before it is chosen.
    expect(busy(confirm)).toBe(false);
    expect(confirm.querySelector(".spinner")).toBeNull();
    inspect.release();
    await within(dialog).findByText("lootfilter/default.filter");
    const start = hold(api, "startOperation");
    await user.click(confirm);
    await user.click(confirm);
    expect(start.calls).toHaveLength(1);
    expect(busy(confirm)).toBe(true);
    start.release();
    expect(await screen.findByRole("dialog", { name: "PD2R was uninstalled" })).toBeTruthy();
  });
});

describe("Settings folder buttons (issue 138)", () => {
  it("an open-folder button is busy until File Explorer answers, and opens once", async () => {
    const { api, user } = mount("ready");
    await loaded();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("tab", { name: "Folders and plugins" }));
    const open = hold(api, "openFolder");
    const region = within(screen.getByRole("tabpanel")).getByRole("region", { name: "PD2R" });
    const [saves, other] = within(region).getAllByRole("button");
    await user.click(saves);
    await user.click(saves);
    await user.click(other);
    expect(open.calls).toEqual([["pd2r", "saves"]]);
    expect(busy(saves)).toBe(true);
    expect(blocked(other)).toBe(true);
    open.release();
    expect(await screen.findByText(/Opened the .* in File Explorer/)).toBeTruthy();
    expect(busy(saves)).toBe(false);
  });
});

describe("Uninstall confirm (issue 138)", () => {
  /** Starts like the native side: answers at once, and the outcome arrives later. */
  function deferOutcome(a: FixtureApi) {
    const starts: unknown[] = [];
    a.startOperation = (request) => {
      starts.push(request);
      return Promise.resolve();
    };
    return starts;
  }

  async function openUninstall(user: ReturnType<typeof userEvent.setup>) {
    await loaded();
    await user.click(screen.getByRole("button", { name: "Manage PD2R" }));
    await user.click(screen.getByRole("menuitem", { name: "Uninstall…" }));
    const dialog = screen.getByRole("dialog", { name: /Uninstall Project Diablo 2/ });
    await within(dialog).findByText("lootfilter/default.filter");
    return dialog;
  }

  it("stays busy after the start answers, until the outcome report arrives", async () => {
    const { api, user } = mount("ready");
    const starts = deferOutcome(api);
    const dialog = await openUninstall(user);
    const confirm = within(dialog).getByRole("button", { name: "Uninstall" });
    await user.click(confirm);
    await waitFor(() => expect(starts).toHaveLength(1));
    await new Promise((r) => setTimeout(r, 0));
    expect(busy(confirm)).toBe(true);
    await user.click(confirm);
    expect(starts).toHaveLength(1);
    act(() =>
      api.emitOutcome({
        status: "succeeded",
        product_id: "pd2r",
        kind: "uninstall",
        report: { preserved: [], remaining: [], clean_install_backups: [] },
      }),
    );
    expect(await screen.findByRole("dialog", { name: "PD2R was uninstalled" })).toBeTruthy();
  });

  it("an outcome that arrives before the start answers isn't overwritten by it", async () => {
    const { api, user } = mount("ready");
    const dialog = await openUninstall(user);
    // The native side spawns the worker before it replies, so a fast failure can arrive first.
    const reply = deferred<void>();
    let starts = 0;
    api.startOperation = () => {
      starts++;
      return reply.promise;
    };
    const confirm = within(dialog).getByRole("button", { name: "Uninstall" });
    await user.click(confirm);
    expect(busy(confirm)).toBe(true);
    act(() =>
      api.emitOutcome({ status: "failed", product_id: "pd2r", kind: "uninstall", error: { code: "internal", message: "File locked" } }),
    );
    await act(async () => reply.resolve());
    await waitFor(() => expect(busy(confirm)).toBe(false));
    expect(blocked(confirm)).toBe(false);
    expect(screen.getByText("File locked")).toBeTruthy();
    await new Promise((r) => setTimeout(r, 0));
    expect(busy(confirm)).toBe(false);
    expect(starts).toBe(1);
  });

  it("a failed uninstall frees the confirm again", async () => {
    const { api, user } = mount("ready");
    const starts = deferOutcome(api);
    const dialog = await openUninstall(user);
    const confirm = within(dialog).getByRole("button", { name: "Uninstall" });
    await user.click(confirm);
    await waitFor(() => expect(starts).toHaveLength(1));
    act(() =>
      api.emitOutcome({ status: "failed", product_id: "pd2r", kind: "uninstall", error: { code: "internal", message: "Disk full" } }),
    );
    await waitFor(() => expect(busy(confirm)).toBe(false));
    expect(blocked(confirm)).toBe(false);
    expect(await screen.findByText("Disk full")).toBeTruthy();
  });
});

describe("location dialog (issue 138)", () => {
  it("Use this installation is busy, with a spinner, until the choice answers", async () => {
    const { api, user } = mount("d2r-choose");
    const dialog = await screen.findByRole("dialog");
    // Detection has answered once the installations are listed.
    await within(dialog).findAllByRole("radio");
    const use = within(dialog).getByRole("button", { name: "Use this installation" });
    const select = hold(api, "selectGameLocation");
    await user.click(use);
    await user.click(use);
    expect(select.calls).toHaveLength(1);
    expect(busy(use)).toBe(true);
    expect(blocked(use)).toBe(true);
    expect(use.querySelector(".spinner")).not.toBeNull();
    expect(blocked(within(dialog).getByRole("button", { name: /Browse for another folder/ }))).toBe(true);
    select.release();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("Browse is busy on its own: Use waits, but doesn't spin", async () => {
    const { api, user } = mount("d2r-choose");
    const dialog = await screen.findByRole("dialog");
    // Detection has answered once the installations are listed.
    await within(dialog).findAllByRole("radio");
    const use = within(dialog).getByRole("button", { name: "Use this installation" });
    const choose = hold(api, "chooseGameLocation");
    const browse = within(dialog).getByRole("button", { name: /Browse for another folder/ });
    await user.click(browse);
    await user.click(browse);
    expect(choose.calls).toHaveLength(1);
    expect(busy(browse)).toBe(true);
    expect(busy(use)).toBe(false);
    expect(blocked(use)).toBe(true);
    await user.click(use);
    expect(api.calls.some((c) => c.name === "selectGameLocation")).toBe(false);
    choose.release();
    await waitFor(() => expect(busy(browse)).toBe(false));
    expect(blocked(use)).toBe(false);
  });
});

function mountSelfUpdate(phase: string) {
  api = createFixtureApi("ready", { tick: 60_000 });
  const su = createSelfUpdateFixture(phase, { tick: 60_000 });
  const user = userEvent.setup();
  render(
    <ApiContext.Provider value={api}>
      <ToastProvider>
        <SelfUpdateProvider api={su}>
          <Shell product={sceneByKey("ready")!.product} />
        </SelfUpdateProvider>
      </ToastProvider>
    </ApiContext.Provider>,
  );
  return { su, user };
}

describe("self-update requests (issue 138)", () => {
  it("Download stays busy after the request answers, until the launcher's next phase arrives", async () => {
    const { su, user } = mountSelfUpdate("available");
    // Answers without a phase event, like the native updater before its first progress.
    let sent = 0;
    su.download = () => {
      sent++;
      return Promise.resolve();
    };
    await user.click(screen.getByRole("button", { name: /^Settings/ }));
    await user.click(screen.getByRole("tab", { name: "Launcher" }));
    const download = await within(screen.getByRole("tabpanel")).findByRole("button", { name: "Download" });
    await user.click(download);
    await new Promise((r) => setTimeout(r, 0));
    await user.click(download);
    expect(sent).toBe(1);
    expect(busy(download)).toBe(true);
    expect(download.querySelector(".spinner")).not.toBeNull();
    act(() => su.emit({ phase: "downloading", current: "1.2.0", next: "1.3.0", percent: 1 }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Download" })).toBeNull());
    su.dispose();
  });

  it("a failed Download frees the button and says why", async () => {
    const { su, user } = mountSelfUpdate("available");
    su.download = () => Promise.reject(new Error("offline"));
    await user.click(screen.getByRole("button", { name: /^Settings/ }));
    await user.click(screen.getByRole("tab", { name: "Launcher" }));
    const download = await within(screen.getByRole("tabpanel")).findByRole("button", { name: "Download" });
    await user.click(download);
    expect(await screen.findByText(/Couldn't start the launcher update/)).toBeTruthy();
    expect(busy(download)).toBe(false);
    su.dispose();
  });

  it("Restart to update sends one request and stays busy until the restart begins", async () => {
    const { su, user } = mountSelfUpdate("ready");
    let sent = 0;
    su.restartToUpdate = () => {
      sent++;
      return Promise.resolve();
    };
    await user.click(screen.getByRole("button", { name: /^Settings/ }));
    await user.click(screen.getByRole("tab", { name: "Launcher" }));
    const restart = await within(screen.getByRole("tabpanel")).findByRole("button", { name: "Restart to update" });
    await user.click(restart);
    await new Promise((r) => setTimeout(r, 0));
    await user.click(restart);
    expect(sent).toBe(1);
    expect(busy(restart)).toBe(true);
    su.dispose();
  });
});

describe("pending across pages and the Manage menu (issue 138)", () => {
  it("leaving the page mid-request and coming back keeps the action busy", async () => {
    const { api, user } = mount("install");
    await screen.findByText(/free space needed/);
    const start = hold(api, "startOperation");
    await user.click(screen.getByRole("button", { name: "Install" }));
    await user.click(screen.getByRole("button", { name: /^Settings/ }));
    await user.click(screen.getByRole("button", { name: "Project Diablo 2: Resurrected" }));
    const install = await screen.findByRole("button", { name: "Install" });
    expect(busy(install)).toBe(true);
    await user.click(install);
    expect(start.calls).toHaveLength(1);
    start.release();
    await screen.findByRole("progressbar");
  });

  it("the Manage menu waits while an action runs, and closes if one starts", async () => {
    const { api, user } = mount("ready");
    await loaded();
    const manage = screen.getByRole("button", { name: "Manage PD2R" });
    const play = hold(api, "playProduct");
    await user.click(screen.getByRole("button", { name: "Play" }));
    expect(blocked(manage)).toBe(true);
    await user.click(manage);
    expect(screen.queryByRole("menu")).toBeNull();
    play.release();
    await screen.findByRole("button", { name: "Playing" });
  });
});

describe("busy safety timeout (issue 138)", () => {
  it("a request that never answers frees its slot after BUSY_SAFETY_MS", async () => {
    vi.useFakeTimers();
    try {
      const { result } = renderHook(() => usePending());
      act(() => void result.current.run("stuck", () => new Promise(() => {})));
      expect(result.current.key).toBe("stuck");
      await act(() => vi.advanceTimersByTimeAsync(BUSY_SAFETY_MS - 1));
      expect(result.current.key).toBe("stuck");
      await act(() => vi.advanceTimersByTimeAsync(1));
      expect(result.current.key).toBeNull();
      // A later run isn't freed by the stuck one's timer.
      act(() => void result.current.run("next", () => new Promise(() => {})));
      expect(result.current.key).toBe("next");
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("toast text (issue 138)", () => {
  it("toast text is large: 17px body, 15px actions", () => {
    const css = readFileSync("src/ui/ui.css", "utf8");
    const rule = (sel: string) => css.match(new RegExp(`\\n${sel.replace(/[.]/g, "\\.")} \\{([^}]*)\\}`))![1];
    expect(rule(".toast")).toMatch(/font-size: 17px;/);
    expect(rule(".toast-actions")).toMatch(/font-size: 15px;/);
  });

  it("a busy button keeps the disabled look: the aria-busy rule sets no opacity", () => {
    const css = readFileSync("src/ui/ui.css", "utf8");
    for (const [, sel, body] of css.matchAll(/([^{}]*)\{([^}]*)\}/g)) {
      if (sel.includes("aria-busy")) expect(body, sel).not.toMatch(/opacity/);
    }
    expect(css).toMatch(/\.btn\[aria-disabled="true"\][^{]*\{[^}]*opacity: 0\.45/);
  });
});
