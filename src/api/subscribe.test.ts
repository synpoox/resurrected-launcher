import { describe, expect, it, vi } from "vitest";
import { subscribe } from "./subscribe";

describe("subscribe", () => {
  it("stops every subscription, including one that resolves after stop", async () => {
    const early = vi.fn();
    const late = vi.fn();
    let resolveLate!: (un: () => void) => void;
    const sub = subscribe(Promise.resolve(early), new Promise((ok) => (resolveLate = ok)));
    await Promise.resolve();
    await Promise.resolve();
    sub.stop();
    expect(early).toHaveBeenCalledTimes(1);
    resolveLate(late);
    await sub.ready;
    expect(late).toHaveBeenCalledTimes(1);
  });

  it("ready resolves once every subscription is in place", async () => {
    const sub = subscribe(Promise.resolve(() => {}), Promise.resolve(() => {}));
    await expect(sub.ready).resolves.toBeUndefined();
    sub.stop();
  });
});
