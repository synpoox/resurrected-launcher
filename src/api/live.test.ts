import { beforeEach, describe, expect, expectTypeOf, it, vi } from "vitest";

const invoke = vi.fn();
const listen = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => false }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

const { createLiveApi } = await import("./live");
const { selectApi } = await import("./index");
const { CONTRACT_VERSION } = await import("../ipc");
type LauncherApi = import("./types").LauncherApi;

const reply = (data: unknown) => ({ contract_version: CONTRACT_VERSION, data });

beforeEach(() => {
  invoke.mockReset();
  listen.mockReset();
});

describe("live adapter", () => {
  it("satisfies LauncherApi with no casts", () => {
    const live = createLiveApi();
    expectTypeOf(live).toExtend<LauncherApi>();
    const api: LauncherApi = live;
    expect(api).toBe(live);
  });

  it("passes window commands straight through to ipc", async () => {
    const live = createLiveApi();
    invoke.mockResolvedValue(undefined);
    await live.minimizeWindow();
    await live.closeWindow();
    await live.startWindowDrag();
    await live.quitLauncher();
    expect(invoke.mock.calls.map((c) => c[0])).toEqual([
      "minimize_window",
      "close_window",
      "start_window_drag",
      "quit_launcher",
    ]);
  });

  it("unwraps Versioned replies and subscribes to the native events", async () => {
    const live = createLiveApi();
    invoke.mockResolvedValue(reply("rolled_back"));
    expect(await live.recoverProduct("pd2r")).toBe("rolled_back");
    expect(invoke).toHaveBeenCalledWith("recover_product", { product_id: "pd2r" });
    listen.mockResolvedValue(() => {});
    await live.onStateChanged(() => {});
    await live.onProgress(() => {});
    await live.onOutcome(() => {});
    await live.onTrayRequest(() => {});
    expect(listen.mock.calls.map((c) => c[0])).toEqual([
      "product-state-changed",
      "operation-progress",
      "operation-outcome",
      "tray-request",
    ]);
  });

  it("passes news commands through with product ids and indexes only", async () => {
    const live = createLiveApi();
    invoke.mockResolvedValueOnce(reply({ status: "offline" }));
    expect(await live.getNews("pd2r")).toEqual({ status: "offline" });
    invoke.mockResolvedValue(undefined);
    await live.openNewsLink("pd2r", 2);
    await live.openProductPage("pd2r");
    await live.openCommunityPage();
    expect(invoke.mock.calls).toEqual([
      ["get_news", { product_id: "pd2r" }],
      ["open_news_link", { product_id: "pd2r", index: 2 }],
      ["open_product_page", { product_id: "pd2r" }],
      ["open_community_page"],
    ]);
  });
});

describe("selectApi with live", () => {
  it("selects the live adapter under Tauri", () => {
    const s = selectApi("", false, true);
    expect(s.ok && s.mode).toBe("live");
  });

  it("?fixture= still selects a fixture in dev builds under Tauri", () => {
    const s = selectApi("?fixture=install", true, true);
    expect(s.ok && s.mode === "fixture" && s.key).toBe("install");
  });

  it("ignores ?fixture= outside dev builds under Tauri", () => {
    const s = selectApi("?fixture=install", false, true);
    expect(s.ok && s.mode).toBe("live");
  });
});
