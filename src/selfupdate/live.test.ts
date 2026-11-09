import { existsSync, readdirSync, readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, expectTypeOf, it, vi } from "vitest";

const invoke = vi.fn();
const listen = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => false }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

const { createLiveSelfUpdate, CHECK_EVERY_MS, FIRST_CHECK_MS } = await import("./live");
const { selectSelfUpdate } = await import("./flag");
const { CONTRACT_VERSION } = await import("../ipc");
type SelfUpdateApi = import("./api").SelfUpdateApi;
type SelfUpdateStatus = import("./api").SelfUpdateStatus;

const reply = (data: unknown) => ({ contract_version: CONTRACT_VERSION, data });
const commands = () => invoke.mock.calls.map((c) => c[0] as string);

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
  listen.mockReset();
  listen.mockResolvedValue(() => {});
});

afterEach(() => {
  vi.useRealTimers();
});

describe("live self-update", () => {
  it("satisfies SelfUpdateApi", () => {
    const live = createLiveSelfUpdate();
    expectTypeOf(live).toExtend<SelfUpdateApi>();
    live.dispose();
  });

  it("passes every call to its narrow native command", async () => {
    const live = createLiveSelfUpdate();
    invoke.mockResolvedValueOnce(reply({ phase: "ready", current: "0.1.0", next: "0.2.0" }));
    expect(await live.getStatus()).toEqual({ phase: "ready", current: "0.1.0", next: "0.2.0" });
    await live.checkForUpdates();
    await live.download();
    await live.restartToUpdate();
    await live.openWhatsNew();
    invoke.mockResolvedValueOnce(reply(false));
    expect(await live.getAutoDownload()).toBe(false);
    await live.setAutoDownload(true);
    expect(invoke.mock.calls).toEqual([
      ["get_launcher_update_status"],
      ["check_for_launcher_update"],
      ["download_launcher_update"],
      ["restart_to_update_launcher"],
      ["open_launcher_whats_new"],
      ["get_launcher_auto_download"],
      ["set_launcher_auto_download", { enabled: true }],
    ]);
    live.dispose();
  });

  it("sends nothing the native side would trust: no key, endpoint, URL or path", async () => {
    const live = createLiveSelfUpdate();
    await live.checkForUpdates();
    await live.download();
    await live.restartToUpdate();
    await live.openWhatsNew();
    for (const call of invoke.mock.calls) expect(call.slice(1), call[0]).toEqual([]);
    live.dispose();
  });

  it("unwraps versioned phase events", async () => {
    const live = createLiveSelfUpdate();
    const seen: SelfUpdateStatus[] = [];
    await live.onPhase((s) => seen.push(s));
    expect(listen.mock.calls[0][0]).toBe("self-update-phase");
    const handler = listen.mock.calls[0][1] as (e: { payload: unknown }) => void;
    handler({ payload: reply({ phase: "signature_rejected", current: "0.1.0", next: "0.2.0" }) });
    expect(seen).toEqual([{ phase: "signature_rejected", current: "0.1.0", next: "0.2.0" }]);
    expect(() => handler({ payload: { contract_version: 999, data: null } })).toThrow(/Unsupported contract version/);
    live.dispose();
  });

  it("checks in the background only while something listens", async () => {
    vi.useFakeTimers();
    const live = createLiveSelfUpdate();
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS);
    expect(commands()).toEqual([]);

    const unlisten = await live.onPhase(() => {});
    await vi.advanceTimersByTimeAsync(FIRST_CHECK_MS);
    expect(commands()).toEqual(["check_for_launcher_update"]);
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS);
    expect(commands()).toEqual(["check_for_launcher_update", "check_for_launcher_update"]);

    unlisten();
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS * 2);
    expect(commands()).toHaveLength(2);
  });

  it("a failed background check is silent", async () => {
    vi.useFakeTimers();
    invoke.mockRejectedValue({ code: "download_failed", message: "offline" });
    const live = createLiveSelfUpdate();
    await live.onPhase(() => {});
    await vi.advanceTimersByTimeAsync(FIRST_CHECK_MS);
    expect(commands()).toEqual(["check_for_launcher_update"]);
    live.dispose();
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS);
    expect(commands()).toHaveLength(1);
  });
});

describe("live selection", () => {
  it("flag off: no updater, even under Tauri, so the launcher never checks", () => {
    expect(selectSelfUpdate(null, false, true)).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
    expect(listen).not.toHaveBeenCalled();
  });

  it("flag on under Tauri: the real updater; a dev ?selfupdate= still picks a fixture", () => {
    const live = selectSelfUpdate(null, true, true);
    expect(live?.ok && live.key).toBe("live");
    if (live?.ok) live.api.dispose();
    const fixture = selectSelfUpdate("ready", true, true);
    expect(fixture?.ok && fixture.key).toBe("ready");
    if (fixture?.ok) fixture.api.dispose();
  });

  it("only release.yml turns VITE_SELF_UPDATE on, for every signed build", () => {
    const envFiles = readdirSync(".").filter((f) => f.startsWith(".env"));
    for (const f of envFiles) expect(readFileSync(f, "utf8"), f).not.toMatch(/VITE_SELF_UPDATE/);
    for (const f of [".github/workflows/ci.yml", "vite.config.ts", "package.json"]) {
      if (existsSync(f)) expect(readFileSync(f, "utf8"), f).not.toMatch(/VITE_SELF_UPDATE/);
    }
    // release.yml sets it for release and gate builds alike (#157); dev and CI builds leave it off.
    const release = readFileSync(".github/workflows/release.yml", "utf8");
    const set = release.split("\n").map((l) => l.trimEnd()).filter((l) => /^\s*VITE_SELF_UPDATE\s*:/.test(l));
    expect(set).toEqual(["          VITE_SELF_UPDATE: '1'"]);
  });
});
