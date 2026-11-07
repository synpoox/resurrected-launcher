import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const names = (text: string, pattern: RegExp) => [...text.matchAll(pattern)].map((m) => m[1]);

describe("command bindings", () => {
  it("invoke exactly the commands Rust allowlists in build.rs", () => {
    const build = readFileSync("src-tauri/build.rs", "utf8");
    const allowlist = names(build.slice(build.indexOf(".commands(")).split("]")[0], /"(\w+)"/g);
    const invoked = names(readFileSync("src/ipc.ts", "utf8"), /invoke(?:<[^(]*>)?\("(\w+)"/g);
    expect(allowlist.length).toBeGreaterThan(1);
    expect([...invoked].sort()).toEqual([...allowlist].sort());
  });
});
