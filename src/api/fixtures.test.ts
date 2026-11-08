import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import { journeyOf } from "../product/journey";
import { createFixtureApi, SCENES, sceneByKey } from "./fixtures";
import { FIXTURE_KEYS, selectApi } from "./index";
import type { LauncherError } from "./types";

const spec = readFileSync("docs/specs/launcher-ui.md", "utf8");
const table = spec.slice(spec.indexOf("## Journey states"), spec.indexOf("## Motion"));
const journeyKeys = [...table.matchAll(/^\| [^|]+ \| `([a-z0-9-]+)` \|/gm)].map((m) => m[1]);

afterEach(() => vi.useRealTimers());

describe("fixtures", () => {
  it("has one named fixture per journey key in the spec, plus offline", () => {
    expect(journeyKeys.length).toBeGreaterThanOrEqual(19);
    for (const key of [...journeyKeys, "offline"]) expect(sceneByKey(key), key).toBeDefined();
    expect(new Set(FIXTURE_KEYS).size).toBe(FIXTURE_KEYS.length);
  });

  it("each state fixture derives the journey it is named after", () => {
    const overlays: Record<string, string> = {
      "d2r-choose": "d2r-missing",
      "save-breaking": "update",
      repair: "ready",
      manage: "ready",
      uninstall: "ready",
      "coming-soon": "ready",
      "quit-busy": "downloading",
    };
    for (const s of SCENES) {
      expect(journeyOf(s.state, (s.progressAtEnd ? s.progress?.at(-1) : s.progress?.[0]) ?? null), s.key).toBe(overlays[s.key] ?? s.key);
    }
  });

  it("an unknown fixture is an error", () => {
    expect(() => createFixtureApi("nope")).toThrow(/Unknown fixture/);
  });
});

describe("selectApi", () => {
  it("selects ?fixture=<key> in dev builds", () => {
    const s = selectApi("?fixture=install", true);
    expect(s.ok && s.key).toBe("install");
  });

  it("shows an error for an unknown key", () => {
    const s = selectApi("?fixture=nope", true);
    expect(s.ok).toBe(false);
    expect(!s.ok && s.error).toMatch(/Unknown fixture "nope"/);
  });

  it("ignores ?fixture= outside dev builds", () => {
    const s = selectApi("?fixture=install", false);
    expect(s.ok && s.key).toBe("ready");
  });
});

describe("fixture API (#74 bindings)", () => {
  it("selectGameLocation indexes the latest detectGameLocations reply", async () => {
    const api = createFixtureApi("d2r-missing");
    await expect(api.selectGameLocation(0)).rejects.toMatchObject({ code: "invalid_request" });
    const found = await api.detectGameLocations();
    const chosen = await api.selectGameLocation(1);
    expect(chosen).toEqual(found[1]);
    await expect(api.selectGameLocation(found.length)).rejects.toMatchObject({ code: "invalid_request" });
    expect((await api.getProductState("pd2r")).location).toEqual(found[1]);
  });

  it("selectGameLocation is refused while the game runs", async () => {
    const api = createFixtureApi("running");
    await api.detectGameLocations();
    await expect(api.selectGameLocation(0)).rejects.toMatchObject({ code: "game_running" });
  });

  it("a save-breaking update needs the acknowledgement", async () => {
    const api = createFixtureApi("save-breaking");
    const request = { product_id: "pd2r", kind: "update" as const };
    const inspection = await api.inspectOperation(request);
    const refusal = await api.startOperation(request, inspection).catch((e: LauncherError) => e);
    expect(refusal).toMatchObject({ code: "save_breaking_not_acknowledged" });
    await api.startOperation({ ...request, acknowledge_save_breaking: true }, inspection);
    expect((await api.getProductState("pd2r")).running).toBe("update");
    api.dispose();
  });

  it("replays progress and advances one event per tick", async () => {
    vi.useFakeTimers();
    const api = createFixtureApi("downloading", { tick: 100 });
    const seen: number[] = [];
    await api.onProgress((e) => seen.push(e.bytes_done ?? -1));
    expect(seen).toHaveLength(1);
    vi.advanceTimersByTime(250);
    expect(seen).toHaveLength(3);
    expect(seen[1]).toBeGreaterThan(seen[0]);
    api.dispose();
  });

  it("cancelling ends with a cancelled outcome", async () => {
    vi.useFakeTimers();
    const api = createFixtureApi("downloading", { tick: 10 });
    const outcomes: string[] = [];
    await api.onOutcome((o) => outcomes.push(o.status));
    await api.cancelOperation("pd2r");
    expect((await api.getProductState("pd2r")).cancelling).toBe(true);
    vi.advanceTimersByTime(20);
    expect(outcomes).toEqual(["cancelled"]);
    expect((await api.getProductState("pd2r")).running).toBeNull();
  });

  it("records calls without sending paths", async () => {
    const api = createFixtureApi("ready");
    await api.openFolder("pd2r", "saves");
    await api.openNewsLink("pd2r", 1);
    expect(api.calls.map((c) => c.name)).toEqual(["openFolder", "openNewsLink"]);
    expect(JSON.stringify(api.calls)).not.toMatch(/[A-Z]:\\/);
  });
});
