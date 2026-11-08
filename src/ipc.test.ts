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

  it("carry Rust's contract version", async () => {
    const rust = readFileSync("src-tauri/src/contract/mod.rs", "utf8");
    const { CONTRACT_VERSION } = await import("./ipc");
    expect(rust).toContain(`pub const CONTRACT_VERSION: u32 = ${CONTRACT_VERSION};`);
  });

  it("mirror every Rust ErrorCode", () => {
    const rust = readFileSync("src-tauri/src/contract/error.rs", "utf8");
    const body = rust.slice(rust.indexOf("pub enum ErrorCode {")).split("\n}")[0];
    const snake = names(body, /^ {4}(\w+),\r?$/gm).map((v) => v.replace(/(?<!^)([A-Z])/g, "_$1").toLowerCase());
    const ts = readFileSync("src/ipc.ts", "utf8");
    const union = ts.slice(ts.indexOf("export type ErrorCode =")).split(";")[0];
    expect(snake.length).toBeGreaterThan(20);
    expect(names(union, /"(\w+)"/g).sort()).toEqual(snake.sort());
  });
});
