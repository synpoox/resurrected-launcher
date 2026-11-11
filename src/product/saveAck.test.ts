// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

beforeEach(() => {
  localStorage.clear();
  vi.resetModules();
});

describe("save-breaking acknowledgement (issue 213)", () => {
  it("survives a restart for the same product and target release only", async () => {
    const first = await import("./saveAck");
    expect(first.saveBreakingAcked("pd2r", "r4", "r5")).toBe(false);
    first.rememberSaveBreakingAck("pd2r", "r4", "r5");
    vi.resetModules(); // a restart: the in-memory set is gone, localStorage stays
    const second = await import("./saveAck");
    expect(second.saveBreakingAcked("pd2r", "r4", "r5")).toBe(true);
    expect(second.saveBreakingAcked("pd2r", "r4", "r6")).toBe(false);
    expect(second.saveBreakingAcked("other", "r4", "r5")).toBe(false);
  });

  it("is per installed release too (issue 215): another starting point asks again", async () => {
    const mod = await import("./saveAck");
    mod.rememberSaveBreakingAck("pd2r", "r4", "r6");
    expect(mod.saveBreakingAcked("pd2r", "r4", "r6")).toBe(true);
    expect(mod.saveBreakingAcked("pd2r", "r5", "r6")).toBe(false);
    expect(mod.saveBreakingAcked("pd2r", undefined, "r6")).toBe(false);
  });

  it("asks again when storage is unavailable", async () => {
    const mod = await import("./saveAck");
    const get = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const set = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(mod.saveBreakingAcked("pd2r", "r4", "r5")).toBe(false);
    expect(() => mod.rememberSaveBreakingAck("pd2r", "r4", "r5")).not.toThrow();
    get.mockRestore();
    set.mockRestore();
  });
});
