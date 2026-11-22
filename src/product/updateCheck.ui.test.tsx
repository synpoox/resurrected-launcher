// @vitest-environment jsdom
// #282: the background catalog check (focus, timer) and the check Play makes first, against fixtures.
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiContext } from "../api/context";
import { createFixtureApi, type FixtureApi } from "../api/fixtures";
import { Shell } from "../shell/Shell";
import { ToastProvider } from "../ui/Toast";
import { CHECK_DEBOUNCE_MS, CHECK_INTERVAL_MS, refreshCatalogWithin, useProducts } from "./store";

let api: FixtureApi | null = null;
let clock = 1_000_000;

function mount(key: string, prepare?: (api: FixtureApi) => void) {
  api = createFixtureApi(key, { tick: 60_000 });
  prepare?.(api);
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

const count = (a: FixtureApi, name: string) => a.calls.filter((c) => c.name === name).length;
const loaded = () => screen.findByRole("tablist");
function Twice() {
  useProducts(api!);
  useProducts(api!);
  return null;
}
const focus = () => act(() => void window.dispatchEvent(new Event("focus")));

beforeEach(() => {
  localStorage.clear();
  vi.spyOn(Date, "now").mockImplementation(() => clock);
});
afterEach(() => {
  cleanup();
  api?.dispose();
  api = null;
  vi.restoreAllMocks();
});

describe("background check", () => {
  it("a newer feed found on focus turns Play into Update", async () => {
    mount("ready-then-update");
    await loaded();
    expect(screen.getByRole("button", { name: "Play" })).toBeTruthy();
    clock += CHECK_DEBOUNCE_MS + 1;
    focus();
    expect(await screen.findByRole("button", { name: "Update" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Play" })).toBeNull();
  });

  it("focus refreshes are debounced", async () => {
    const { api } = mount("ready");
    await loaded();
    expect(count(api, "refreshCatalog")).toBe(1); // startup's own
    focus(); // inside the debounce window of startup
    expect(count(api, "refreshCatalog")).toBe(1);
    clock += CHECK_DEBOUNCE_MS + 1;
    focus();
    focus();
    clock += 1000;
    focus();
    expect(count(api, "refreshCatalog")).toBe(2);
  });
});

describe("watchers", () => {
  it("two useProducts mounts share one watcher, which stops on unmount", async () => {
    const add = vi.spyOn(window, "addEventListener");
    api = createFixtureApi("ready");
    const { unmount } = render(
      <ApiContext.Provider value={api}>
        <Twice />
      </ApiContext.Provider>,
    );
    expect(add.mock.calls.filter((c) => c[0] === "focus")).toHaveLength(1);
    await waitFor(() => expect(count(api!, "refreshCatalog")).toBe(1));
    unmount();
    clock += CHECK_DEBOUNCE_MS + 1;
    focus();
    expect(count(api, "refreshCatalog")).toBe(1);
  });

  it("the native window focus, a visibilitychange and the interval each check, through the debounce", async () => {
    const timers = vi.spyOn(globalThis, "setInterval");
    const { api } = mount("ready");
    await loaded();
    const tick = timers.mock.calls.find((c) => c[1] === CHECK_INTERVAL_MS)![0] as () => void;
    clock += CHECK_DEBOUNCE_MS + 1;
    act(() => api.emitWindowFocus());
    expect(count(api, "refreshCatalog")).toBe(2);
    act(() => api.emitWindowFocus()); // debounced
    expect(count(api, "refreshCatalog")).toBe(2);
    clock += CHECK_DEBOUNCE_MS + 1;
    act(() => void document.dispatchEvent(new Event("visibilitychange")));
    expect(count(api, "refreshCatalog")).toBe(3);
    clock += CHECK_INTERVAL_MS;
    tick(); // the interval's callback
    expect(count(api, "refreshCatalog")).toBe(4);
  });
});

describe("Play checks the feed first", () => {
  it("offers the update instead of launching when a newer release is out", async () => {
    const { api, user } = mount("ready-then-update");
    await loaded();
    await user.click(screen.getByRole("button", { name: "Play" }));
    expect(await screen.findByRole("button", { name: "Update" })).toBeTruthy();
    expect(count(api, "playProduct")).toBe(0);
  });

  it("launches the installed release when the feed can't be reached", async () => {
    const { api, user } = mount("ready", (a) => {
      const first = a.refreshCatalog;
      let n = 0;
      a.refreshCatalog = () => (++n === 1 ? first() : Promise.reject(new Error("offline")));
    });
    await loaded();
    await user.click(screen.getByRole("button", { name: "Play" }));
    await waitFor(() => expect(count(api, "playProduct")).toBe(1));
  });

  it("launches when the refresh resolves but reports the catalog failed", async () => {
    const { api, user } = mount("offline");
    await loaded();
    await user.click(screen.getByRole("button", { name: "Play" }));
    await waitFor(() => expect(count(api, "playProduct")).toBe(1));
  });

  it("launches when the state read after the refresh fails", async () => {
    const { api, user } = mount("ready");
    await loaded();
    const read = api.getProductState;
    let fail = false;
    api.getProductState = (id) => (fail ? Promise.reject(new Error("x")) : read(id));
    api.refreshCatalog = () => ((fail = true), Promise.resolve());
    await user.click(screen.getByRole("button", { name: "Play" }));
    await waitFor(() => expect(count(api, "playProduct")).toBe(1));
  });

  it("launches when the feed is up to date", async () => {
    const { api, user } = mount("ready");
    await loaded();
    await user.click(screen.getByRole("button", { name: "Play" }));
    await waitFor(() => expect(count(api, "playProduct")).toBe(1));
  });
});

describe("refreshCatalogWithin", () => {
  it("waits through refreshing, ignores unrelated events, and ends when the refresh does", async () => {
    const a = createFixtureApi("ready");
    const read = a.getProductState;
    let refreshing = true;
    a.getProductState = async (id) => ({ ...(await read(id)), catalog: refreshing ? { status: "refreshing" } : { status: "refreshed" } });
    const unrelated = a.onStateChanged;
    let fire = () => undefined as void;
    a.onStateChanged = (h) => ((fire = h), unrelated(h));
    let done = false;
    void refreshCatalogWithin(a, "pd2r", 60_000).then(() => (done = true));
    await waitFor(() => expect(count(a, "refreshCatalog")).toBe(1));
    fire(); // an unrelated event while the backend still says refreshing
    await new Promise((r) => setTimeout(r, 20));
    expect(done).toBe(false);
    refreshing = false;
    fire();
    await waitFor(() => expect(done).toBe(true));
  });

  it("gives up after the timeout when the refresh never ends", async () => {
    vi.useFakeTimers();
    try {
      const a = createFixtureApi("ready");
      a.refreshCatalog = () => new Promise(() => undefined);
      let done = false;
      void refreshCatalogWithin(a, "pd2r", 4000).then(() => (done = true));
      await vi.advanceTimersByTimeAsync(3999);
      expect(done).toBe(false);
      await vi.advanceTimersByTimeAsync(2);
      expect(done).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });
});
