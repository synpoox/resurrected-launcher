// @vitest-environment jsdom
// Issue 138: action buttons are busy from the click until the result or dialog arrives, never
// submit twice, and consent dialogs open at once with a loading state while inspection runs.
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ApiContext } from "../api/context";
import { createFixtureApi, sceneByKey, type FixtureApi } from "../api/fixtures";
import type { Inspection } from "../api/types";
import { Shell } from "../shell/Shell";
import { ToastProvider } from "../ui/Toast";

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
function hold<K extends "inspectOperation" | "startOperation" | "playProduct" | "cancelOperation" | "openFolder">(a: FixtureApi, name: K) {
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
    expect(await within(dialog).findByText(/doesn't match 0\.4\.2/)).toBeTruthy();
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
    inspect.release();
    await waitFor(() => expect(busy(screen.getByRole("button", { name: "Clean install…" }))).toBe(false));
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

describe("toast text (issue 138)", () => {
  it("toast text is large: 17px body, 15px actions", () => {
    const css = readFileSync("src/ui/ui.css", "utf8");
    const rule = (sel: string) => css.match(new RegExp(`\\n${sel.replace(/[.]/g, "\\.")} \\{([^}]*)\\}`))![1];
    expect(rule(".toast")).toMatch(/font-size: 17px;/);
    expect(rule(".toast-actions")).toMatch(/font-size: 15px;/);
  });
});
