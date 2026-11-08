// @vitest-environment jsdom
// Issue 123: vitest doesn't load CSS, and the release build ships every stylesheet as one global
// bundle, so a page's rule can restyle the shell. These tests load all of src/**/*.css into the
// document, then check the sidebar's cascaded styles. jsdom computes the cascade but not layout,
// so the spec's "Settings at the bottom" and "products clickable" are checked as the rules that
// produce them.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { ApiContext } from "../api/context";
import { createFixtureApi, type FixtureApi } from "../api/fixtures";
import { ToastProvider } from "../ui/Toast";
import { Shell } from "./Shell";

function cssFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return cssFiles(path);
    return path.endsWith(".css") ? [path] : [];
  });
}

const sheets = cssFiles("src").map((f) => f.replaceAll("\\", "/"));
let styles: HTMLStyleElement[] = [];
let api: FixtureApi | null = null;

beforeAll(() => {
  styles = sheets.map((file) => {
    const style = document.createElement("style");
    style.dataset.file = file;
    style.textContent = readFileSync(file, "utf8");
    document.head.append(style);
    return style;
  });
});
afterAll(() => styles.forEach((s) => s.remove()));
beforeEach(() => localStorage.clear());
afterEach(() => {
  cleanup();
  api?.dispose();
  api = null;
});

function mount() {
  api = createFixtureApi("ready", { tick: 60_000 });
  render(
    <ApiContext.Provider value={api}>
      <ToastProvider>
        <Shell product="pd2r" />
      </ToastProvider>
    </ApiContext.Provider>,
  );
  return userEvent.setup();
}

const sidebar = () => screen.getByRole("navigation", { name: "Launcher" });
const sidebarItems = () => [...sidebar().querySelectorAll<HTMLElement>("button")];
const settingsItem = () => screen.getByRole("button", { name: "Settings" });

/** Every style rule in `file`, including those inside media queries. */
function rules(file: string): CSSStyleRule[] {
  const sheet = styles.find((s) => s.dataset.file === file)!.sheet!;
  const out: CSSStyleRule[] = [];
  const walk = (list: CSSRuleList) => {
    for (const rule of list) {
      if (rule instanceof CSSStyleRule) out.push(rule);
      else if ("cssRules" in rule) walk((rule as CSSGroupingRule).cssRules);
    }
  };
  walk(sheet.cssRules);
  return out;
}

describe("the sidebar with every stylesheet loaded (issue 123)", () => {
  it("loads the stylesheets the sidebar and Settings use", () => {
    expect(sheets).toContain("src/shell/shell.css");
    expect(sheets).toContain("src/settings/settings.css");
  });

  for (const view of ["a product page", "Settings"]) {
    it(`keeps every sidebar item in the column's flow on ${view}`, async () => {
      const user = mount();
      if (view === "Settings") await user.click(settingsItem());
      await screen.findByRole("heading", { name: view === "Settings" ? "Settings" : /./ });
      for (const item of sidebarItems()) {
        const style = getComputedStyle(item);
        // An absolute item covers the sidebar (it is position: relative) and takes every click.
        expect([item.textContent, style.position]).toEqual([item.textContent, "static"]);
      }
      // Settings sits at the bottom of the flex column, pushed down by an auto top margin
      // (jsdom resolves a computed auto margin to 0px, so the matching rule is checked).
      expect(getComputedStyle(sidebar()).flexDirection).toBe("column");
      const pushed = rules("src/shell/shell.css").filter((r) => settingsItem().matches(r.selectorText) && r.style.marginTop === "auto");
      expect(pushed.map((r) => r.selectorText)).not.toEqual([]);
    });
  }

  it("no page stylesheet styles anything in the sidebar", async () => {
    const user = mount();
    await user.click(settingsItem());
    await screen.findByRole("heading", { name: "Settings" });
    const inSidebar = [sidebar(), ...sidebar().querySelectorAll("*")];
    // The sidebar's own rules live in shell.css; the Settings dot styles itself (selfupdate.css).
    const pages = sheets.filter((f) => !/^src\/(shell|selfupdate|ui)\/|^src\/(tokens|styles)\.css$/.test(f));
    const leaks = pages.flatMap((file) =>
      rules(file)
        .filter((r) => inSidebar.some((el) => el.matches(r.selectorText)))
        .map((r) => `${file}: ${r.selectorText}`),
    );
    expect(leaks).toEqual([]);
  });
});
