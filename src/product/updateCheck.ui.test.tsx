// @vitest-environment jsdom
// #282: the background catalog check (focus, timer) and the check Play makes first, against fixtures.
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiContext } from "../api/context";
import { createFixtureApi, type FixtureApi } from "../api/fixtures";
import { Shell } from "../shell/Shell";
import { ToastProvider } from "../ui/Toast";
import { CHECK_DEBOUNCE_MS, refreshCatalogWithin } from "./store";

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

  it("launches when the feed is up to date", async () => {
    const { api, user } = mount("ready");
    await loaded();
    await user.click(screen.getByRole("button", { name: "Play" }));
    await waitFor(() => expect(count(api, "playProduct")).toBe(1));
  });
});

describe("refreshCatalogWithin", () => {
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
