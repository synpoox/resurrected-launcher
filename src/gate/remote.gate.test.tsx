// @vitest-environment jsdom
// Slice 4 gate (#81), clause "No remote content native privilege", in the window: news text renders as
// text, links open only by index through `open_news_link`, the webview never builds a link, request or
// markup from remote text, and the shipped CSP allows no remote `connect-src` or `img-src`. The native
// side (capabilities, URL refusals) is src-tauri/tests/slice4_gate/remote_content.rs.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { cleanup, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { disposeAll, mount } from "./gate";

beforeEach(() => localStorage.clear());
afterEach(() => {
  cleanup();
  disposeAll();
});

const HOSTILE = `<img src="https://evil.example/x.png" onerror="alert(1)"><a href="https://evil.example">x</a>`;

describe("news renders as text and opens by index", () => {
  it("markup in a headline and tag is shown as text, never parsed", async () => {
    mount("ready", {
      prepare: (a) => {
        a.scene.news.pd2r = { status: "ok", items: [{ tag: "<b>Guide</b>", title: HOSTILE, published: "2026-11-01" }] };
      },
    });
    const feed = await screen.findByRole("region", { name: "News" });
    await within(feed).findByText((_, el) => el?.classList.contains("feed-title") === true);
    expect(feed.textContent).toContain(HOSTILE);
    expect(feed.textContent).toContain("<b>Guide</b>");
    expect(feed.querySelector("img, a, b, script, iframe")).toBeNull();
    expect(document.querySelector("img[src^='http'], a[href^='http'], iframe")).toBeNull();
  });

  it("a click sends the product id and the item's index, never a URL", async () => {
    const { api, user } = mount("ready");
    const feed = await screen.findByRole("region", { name: "News" });
    const items = await within(feed).findAllByRole("button");
    await user.click(items[2]);
    const call = api.calls.at(-1)!;
    expect(call.name).toBe("openNewsLink");
    expect(call.args).toEqual(["pd2r", 2]);
    expect(call.args.every((a) => typeof a !== "string" || !/[:/]/.test(a))).toBe(true);
  });

  it("the IPC binding for news links carries only product_id and index", () => {
    const ipc = readFileSync("src/ipc.ts", "utf8");
    expect(ipc).toMatch(/invoke\("open_news_link", \{ product_id: productId, index \}\)/);
    expect(ipc).toMatch(/invoke\("open_product_page", \{ product_id: productId \}\)/);
  });
});

/** Every non-test source file under src/. */
function sources(dir = "src"): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return sources(path);
    return /\.(ts|tsx)$/.test(name) && !/\.test\.tsx?$/.test(name) && !path.includes(join("src", "gate")) ? [path] : [];
  });
}

describe("the webview has no remote content path of its own", () => {
  it("no source renders HTML, opens windows, or makes network requests", () => {
    const banned = /dangerouslySetInnerHTML|\.innerHTML\s*=|window\.open\(|\bfetch\(|XMLHttpRequest|new WebSocket|<a\s|<iframe|\beval\(/;
    const hits = sources().filter((f) => banned.test(readFileSync(f, "utf8")));
    expect(hits).toEqual([]);
  });

  it("the CSP allows no remote connect-src or img-src", () => {
    const conf = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
    const csp: string = conf.app.security.csp;
    const directive = (name: string) =>
      csp
        .split(";")
        .map((d) => d.trim().split(/\s+/))
        .find(([n]) => n === name)
        ?.slice(1);
    expect(directive("connect-src")).toEqual(["ipc:", "http://ipc.localhost"]);
    expect(directive("img-src")).toEqual(["'self'"]);
    expect(directive("default-src")).toEqual(["'self'"]);
    for (const name of ["script-src", "frame-src", "object-src"]) {
      // Absent: falls back to default-src 'self'.
      expect(directive(name), name).toBeUndefined();
    }
    expect(csp).not.toMatch(/https?:\/\/(?!ipc\.localhost)|\*|'unsafe-inline'|'unsafe-eval'|data:/);
    expect(conf.app.security.dangerousDisableAssetCspModification).toBeUndefined();
    expect(conf.app.withGlobalTauri).toBeUndefined();
  });
});
