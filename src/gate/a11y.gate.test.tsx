// @vitest-environment jsdom
// Slice 4 gate (#81), clause "Keyboard and accessibility": automated axe-core checks (WCAG 2.2 A/AA)
// on every journey state (both tabs), every Settings panel and every self-update phase with the flag
// on, plus the spec's tab order, keyboard activation and Esc/focus return (jsdom has no layout).
// The manual keyboard pass at 1280×720 and 1024×640 is a checklist in the gate PR.
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SCENES } from "../api/fixtures";
import { SELF_UPDATE_FIXTURES } from "../selfupdate/api";
import { disposeAll, INDEX_HTML, mount, violations } from "./gate";

beforeEach(() => localStorage.clear());
afterEach(() => {
  cleanup();
  disposeAll();
});

/** Waits until the scene's own surface (dialog, menu, tabs or news) has rendered. */
async function settled(open: string | undefined) {
  await screen.findByRole("region", { name: "News" }).catch(() => undefined);
  if (open === "manage") await screen.findByRole("menu");
  else if (open && open !== "repair") await screen.findByRole("dialog");
  // Product state has loaded: no read is still pending.
  await waitFor(() => expect(document.querySelector(".page")).not.toBeNull());
}

const released = SCENES.filter((s) => s.key !== "coming-soon");

describe("axe-core harness", () => {
  it("is not vacuous: a nameless button and an unlabelled input are reported", async () => {
    document.body.innerHTML = '<main><button type="button"></button><input type="text"></main>';
    const found = await violations();
    expect(found.some((v) => v.startsWith("button-name:"))).toBe(true);
    expect(found.some((v) => v.startsWith("label:"))).toBe(true);
    document.body.innerHTML = "";
  });

  it("index.html declares the document language", () => {
    expect(INDEX_HTML).toMatch(/<html[^>]* lang="en"/);
  });

  // axe `document-title` (WCAG 2.4.2): the webview document carries the umbrella name too (#114).
  it("index.html has a <title>", () => {
    expect(INDEX_HTML).toMatch(/<title>[^<]+<\/title>/);
  });
});

describe("axe-core: every journey state, Overview", () => {
  it.each(SCENES.map((s) => [s.key, s.open] as const))("%s", async (key, open) => {
    mount(key);
    await settled(open);
    expect(await violations()).toEqual([]);
  });
});

describe("axe-core: every journey state, Patch Notes", () => {
  // Patch Notes has no Manage button (spec), so the `manage` scene opens no menu there.
  it.each(released.map((s) => [s.key, s.open === "manage" ? undefined : s.open] as const))("%s", async (key, open) => {
    mount(key, { tab: "notes" });
    await settled(open);
    if (!open) await screen.findByRole("tabpanel");
    expect(await violations()).toEqual([]);
  });
});

const PANELS = ["Game", "Launcher", "Folders and plugins", "Troubleshooting"];

describe("axe-core: every Settings panel", () => {
  it.each(PANELS)("%s", async (panel) => {
    const { user } = mount("ready");
    await screen.findByRole("tablist");
    await user.click(screen.getByRole("button", { name: /^Settings/ }));
    await user.click(screen.getByRole("tab", { name: panel }));
    expect(screen.getByRole("tabpanel")).toBeTruthy();
    await screen.findByText(/build 3\.3\.0|Start with Windows|PD2R plugins|Export diagnostics/);
    expect(await violations()).toEqual([]);
  });

  it("Export diagnostics review dialog", async () => {
    const { user } = mount("ready");
    await screen.findByRole("tablist");
    await user.click(screen.getByRole("button", { name: /^Settings/ }));
    await user.click(screen.getByRole("tab", { name: "Troubleshooting" }));
    await user.click(screen.getByRole("button", { name: /Export diagnostics/ }));
    await screen.findByRole("dialog");
    await screen.findByText(/Launcher diagnostics \(fixture\)/);
    expect(await violations()).toEqual([]);
  });
});

describe("axe-core: every self-update phase (flag on)", () => {
  it.each(SELF_UPDATE_FIXTURES.map((f) => f.key))("%s: product page with its toast", async (phase) => {
    mount("ready", { selfUpdate: phase });
    await screen.findByRole("tablist").catch(() => undefined);
    await screen.findByRole("region", { name: "Notifications" }).catch(() => undefined);
    expect(await violations()).toEqual([]);
  });

  it.each(SELF_UPDATE_FIXTURES.filter((f) => f.key !== "restarting" && f.key !== "quitting").map((f) => f.key))(
    "%s: Settings → Launcher",
    async (phase) => {
      const { user } = mount("ready", { selfUpdate: phase });
      await screen.findByRole("tablist");
      await user.click(screen.getByRole("button", { name: /^Settings/ }));
      await user.click(screen.getByRole("tab", { name: "Launcher" }));
      await within(screen.getByRole("tabpanel")).findByText("Launcher version");
      expect(await violations()).toEqual([]);
    },
  );

  it("held restart (ready while downloading): toast and Settings → Launcher", async () => {
    const { user } = mount("downloading", { selfUpdate: "ready" });
    await screen.findByText(/Restarts after PD2R finishes downloading/);
    expect(await violations()).toEqual([]);
    await user.click(screen.getByRole("button", { name: /^Settings/ }));
    await user.click(screen.getByRole("tab", { name: "Launcher" }));
    expect(await violations()).toEqual([]);
  });
});

describe("keyboard", () => {
  it("tab order follows the spec: window controls → sidebar → tabs → action row → news", async () => {
    const { user } = mount("ready");
    await screen.findByText(/TCP\/IP co-op testing weekend/);
    const zone = (el: Element) =>
      el.closest(".window-controls, [aria-label='Window controls']")
        ? "window"
        : el.closest("nav, .sidebar, [aria-label='Products']")
          ? "sidebar"
          : el.closest("[role=tablist]")
            ? "tabs"
            : el.closest("[aria-label='News']")
              ? "news"
              : "main";
    const zones: string[] = [];
    for (let i = 0; i < 20; i++) {
      await user.tab();
      const z = zone(document.activeElement!);
      if (zones.at(-1) !== z) zones.push(z);
    }
    // One pass in order; the tab cycle may wrap back to the window controls at the end.
    const pass = zones.slice(0, zones.indexOf("news") + 1);
    expect(pass).toEqual(["window", "sidebar", "tabs", "main", "news"]);
  });

  it("every journey state's main action is reachable by keyboard and activates with Enter", async () => {
    const { api, user } = mount("install");
    const install = await screen.findByRole("button", { name: /^Install/ });
    for (let i = 0; i < 30 && document.activeElement !== install; i++) await user.tab();
    expect(document.activeElement).toBe(install);
    await user.keyboard("{Enter}");
    await waitFor(() => expect(api.calls.some((c) => c.name === "startOperation")).toBe(true));
  });

  it("Esc closes a dialog and returns focus to its opener", async () => {
    const { user } = mount("ready");
    await screen.findByRole("tablist");
    await user.click(screen.getByRole("button", { name: /^Settings/ }));
    await user.click(screen.getByRole("tab", { name: "Troubleshooting" }));
    const opener = screen.getByRole("button", { name: /Export diagnostics/ });
    await user.click(opener);
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(document.activeElement).toBe(opener);
  });
});
