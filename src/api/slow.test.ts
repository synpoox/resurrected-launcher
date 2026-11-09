import { afterEach, describe, expect, it, vi } from "vitest";
import { createFixtureApi } from "./fixtures";
import { selectApi } from "./index";
import { withDelay } from "./slow";

afterEach(() => vi.useRealTimers());

describe("?slow= dev fixture delay (issue 138)", () => {
  it("delays click-driven calls and leaves the rest immediate", async () => {
    vi.useFakeTimers();
    const base = createFixtureApi("ready", { tick: 60_000 });
    const api = withDelay(base, 500);
    let played = false;
    void api.playProduct("pd2r").then(() => (played = true));
    await vi.advanceTimersByTimeAsync(499);
    expect(played).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(played).toBe(true);
    expect((await api.getProductState("pd2r")).product_id).toBe("pd2r");
    expect(api.calls.map((c) => c.name)).toEqual(["playProduct", "getProductState"]);
    base.dispose();
  });

  it("applies only in dev, from &slow=", async () => {
    vi.useFakeTimers();
    const answered = async (search: string, dev: boolean) => {
      const sel = selectApi(search, dev);
      if (!sel.ok) throw new Error("fixture not selected");
      let done = false;
      void sel.api.playProduct("pd2r").then(() => (done = true));
      await vi.advanceTimersByTimeAsync(10);
      sel.api.dispose();
      return done;
    };
    expect(await answered("?fixture=ready&slow=800", true)).toBe(false);
    expect(await answered("?fixture=ready", true)).toBe(true);
    expect(await answered("?slow=800", false)).toBe(true);
  });
});
