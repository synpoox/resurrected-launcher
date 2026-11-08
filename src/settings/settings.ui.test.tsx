// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ApiContext } from "../api/context";
import { createFixtureApi, sceneByKey, type FixtureApi } from "../api/fixtures";
import type { LauncherError } from "../api/types";
import { Shell } from "../shell/Shell";
import { ToastProvider } from "../ui/Toast";

let api: FixtureApi | null = null;

async function openSettings(key: string, prepare?: (api: FixtureApi) => void, ready: RegExp = /build 3\.3\.0|wasn't found on this PC/) {
  api = createFixtureApi(key, { tick: 60_000 });
  prepare?.(api);
  const scene = sceneByKey(key)!;
  const user = userEvent.setup();
  render(
    <ApiContext.Provider value={api}>
      <ToastProvider>
        <Shell product={scene.product} />
      </ToastProvider>
    </ApiContext.Provider>,
  );
  await user.click(screen.getByRole("button", { name: "Settings" }));
  // Product state has loaded once the location row shows a store or the "not found" hint.
  await screen.findByText(ready);
  return { api, user };
}

const tab = (name: string) => screen.getByRole("tab", { name });
const panel = () => screen.getByRole("tabpanel");
const names = (a: FixtureApi) => a.calls.map((c) => c.name);
const reject = (code: LauncherError["code"]) => () => Promise.reject({ code, message: code } satisfies LauncherError);

beforeEach(() => localStorage.clear());
afterEach(() => {
  cleanup();
  api?.dispose();
  api = null;
});

describe("settings sub-nav", () => {
  it("lists the four sections and shows one panel at a time", async () => {
    const { user } = await openSettings("ready");
    expect(screen.getByRole("heading", { name: "Settings", level: 1 })).toBeTruthy();
    const tabs = within(screen.getByRole("tablist", { name: "Settings sections" })).getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["Game", "Launcher", "Folders and plugins", "Troubleshooting"]);
    expect(tab("Game").getAttribute("aria-selected")).toBe("true");
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
    expect(panel().getAttribute("aria-labelledby")).toBe(tab("Game").id);
    // Every aria-controls points at a rendered panel.
    for (const t of tabs) {
      const controls = t.getAttribute("aria-controls");
      if (controls) expect(document.getElementById(controls)).toBe(panel());
    }
    expect(tab("Game").getAttribute("aria-controls")).toBe(panel().id);
    expect(tab("Launcher").hasAttribute("aria-controls")).toBe(false);

    await user.click(tab("Troubleshooting"));
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
    expect(within(panel()).getByRole("button", { name: "Export diagnostics…" })).toBeTruthy();
    expect(within(panel()).queryByText("Diablo II: Resurrected location")).toBeNull();
  });

  it("moves between sections with the arrow keys, Home and End, with one tab stop", async () => {
    const { user } = await openSettings("ready");
    expect(tab("Game").tabIndex).toBe(0);
    expect(tab("Launcher").tabIndex).toBe(-1);
    tab("Game").focus();
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(tab("Launcher"));
    expect(tab("Launcher").getAttribute("aria-selected")).toBe("true");
    expect(within(panel()).getByRole("switch", { name: "Start with Windows" })).toBeTruthy();
    await user.keyboard("{End}");
    expect(tab("Troubleshooting").getAttribute("aria-selected")).toBe("true");
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(tab("Game"));
    await user.keyboard("{ArrowUp}");
    expect(document.activeElement).toBe(tab("Troubleshooting"));
    await user.keyboard("{Home}");
    expect(tab("Game").getAttribute("aria-selected")).toBe("true");
    // Tab leaves the tab list for the panel's control.
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Change…" }));
  });
});

describe("Game", () => {
  it("shows the D2R location with its store and build", async () => {
    await openSettings("ready");
    expect(within(panel()).getByText("C:\\Program Files (x86)\\Diablo II Resurrected")).toBeTruthy();
    expect(within(panel()).getByText(/Battle\.net · build 3\.3\.0/)).toBeTruthy();
  });

  it("Change… opens the location dialog; choosing moves nothing and a notice reports the recheck", async () => {
    const { api, user } = await openSettings("ready");
    const change = screen.getByRole("button", { name: "Change…" });
    await user.click(change);
    const dialog = await screen.findByRole("dialog", { name: "Choose your D2R installation" });
    await user.click(await within(dialog).findByRole("radio", { name: /D:\\Games/ }));
    await user.click(within(dialog).getByRole("button", { name: "Use this installation" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(api.calls.find((c) => c.name === "selectGameLocation")?.args).toEqual([1]);
    const notice = await screen.findByText(/Nothing was moved, copied or deleted/);
    expect(notice.textContent).toContain("D:\\Games\\Diablo II Resurrected");
    expect(notice.textContent).toContain("PD2R alpha 0.4.2 was found there and is ready to play.");
    expect(names(api)).not.toContain("startOperation");
    expect(document.activeElement).toBe(change);
  });

  it("browsing to a folder without D2R.exe shows an inline error", async () => {
    const { user } = await openSettings("ready", (a) => {
      a.chooseGameLocation = reject("d2r_not_found");
    });
    await user.click(screen.getByRole("button", { name: "Change…" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Browse for another folder…" }));
    expect((await within(dialog).findByRole("alert")).textContent).toMatch(/doesn't contain D2R\.exe/);
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("is disabled while the game runs, with the reason next to it", async () => {
    await openSettings("running");
    const change = screen.getByRole("button", { name: "Change…" }) as HTMLButtonElement;
    expect(change.disabled).toBe(true);
    const why = screen.getByText("Quit the game to change this");
    expect(change.getAttribute("aria-describedby")).toBe(why.id);
  });

  it("explains why Change… is disabled when product state couldn't be read", async () => {
    await openSettings("ready", (a) => {
      a.getProductState = reject("internal");
    }, /couldn't be read/);
    const change = screen.getByRole("button", { name: /Change…|Choose…/ }) as HTMLButtonElement;
    expect(change.disabled).toBe(true);
    const why = screen.getByText(/Couldn't read the launcher's state/);
    expect(change.getAttribute("aria-describedby")).toBe(why.id);
  });

  it("offers Choose… when D2R wasn't found", async () => {
    await openSettings("d2r-missing");
    expect(screen.getByRole("button", { name: "Choose…" })).toBeTruthy();
  });
});

describe("Launcher", () => {
  it("Start with Windows is off by default and toggles with click and keyboard", async () => {
    const { api, user } = await openSettings("ready");
    await user.click(tab("Launcher"));
    const sw = within(panel()).getByRole("switch", { name: "Start with Windows" });
    await waitFor(() => expect(names(api)).toContain("getStartWithWindows"));
    expect(sw.getAttribute("aria-checked")).toBe("false");
    expect(within(panel()).getByText(/Opens in the tray/)).toBeTruthy();
    await user.click(sw);
    expect(sw.getAttribute("aria-checked")).toBe("true");
    await user.keyboard(" ");
    expect(sw.getAttribute("aria-checked")).toBe("false");
    await user.keyboard("{Enter}");
    expect(sw.getAttribute("aria-checked")).toBe("true");
    expect(api.calls.filter((c) => c.name === "setStartWithWindows").map((c) => c.args)).toEqual([[true], [false], [true]]);
  });

  it("reverts the switch and shows an error toast when it can't be changed", async () => {
    const { user } = await openSettings("ready", (a) => {
      a.setStartWithWindows = reject("access_denied");
    });
    await user.click(tab("Launcher"));
    const sw = within(panel()).getByRole("switch", { name: "Start with Windows" });
    await user.click(sw);
    expect((await screen.findByRole("alert")).textContent).toMatch(/Couldn't change Start with Windows/);
    expect(sw.getAttribute("aria-checked")).toBe("false");
  });

  it("About and credits lists each hero with its name and source, and says placeholders are placeholders", async () => {
    const { user } = await openSettings("ready");
    await user.click(tab("Launcher"));
    const credits = within(within(panel()).getByRole("region", { name: "About and credits" })).getAllByRole("listitem");
    expect(credits.map((c) => c.querySelector("b")?.textContent)).toEqual([
      "Project Diablo 2: Resurrected",
      "Diablo II: Resurrected Forever",
      "Diablo II: Resurrected 09",
    ]);
    for (const c of credits) {
      expect(c.textContent).toContain("Placeholder hero · Original placeholder made for this launcher");
      expect(c.textContent).toContain("Placeholder art, not game art.");
    }
  });
});

describe("Folders and plugins", () => {
  it("opens each PD2R folder and the global plugins folder through openFolder", async () => {
    const { api, user } = await openSettings("ready");
    await user.click(tab("Folders and plugins"));
    const pd2r = within(panel()).getByRole("region", { name: "PD2R" });
    expect(within(pd2r).getByText("Loads only with PD2R.", { exact: false })).toBeTruthy();
    expect(within(pd2r).getByText("core-*")).toBeTruthy();
    const buttons = within(pd2r).getAllByRole("button");
    for (const b of buttons) await user.click(b);
    const all = within(panel()).getByRole("region", { name: "All products" });
    expect(within(all).getByText("Loads for every product.")).toBeTruthy();
    await user.click(within(all).getByRole("button", { name: "Open plugins folder" }));
    expect(api.calls.filter((c) => c.name === "openFolder").map((c) => c.args)).toEqual([
      ["pd2r", "saves"],
      ["pd2r", "settings"],
      ["pd2r", "product_plugins"],
      [null, "global_plugins"],
    ]);
    expect(within(panel()).getByText(/doesn't list, enable or disable plugins/)).toBeTruthy();
    expect(within(panel()).getByText(/Forever and 09 aren't installed/)).toBeTruthy();
  });

  it("says a plugins folder was created on its first open only", async () => {
    const { user } = await openSettings("ready");
    await user.click(tab("Folders and plugins"));
    const open = () => within(within(panel()).getByRole("region", { name: "All products" })).getByRole("button");
    await user.click(open());
    expect(await screen.findByText(/It didn't exist yet, so the launcher created it empty/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    await user.click(open());
    expect(screen.getByText("Opened the global plugins folder in File Explorer.")).toBeTruthy();
    expect(screen.queryByText(/created it/)).toBeNull();
  });

  it("takes the created message from openFolder's result, not a guess", async () => {
    const { user } = await openSettings("ready", (a) => {
      a.openFolder = () => Promise.resolve(false);
    });
    await user.click(tab("Folders and plugins"));
    const pd2r = within(panel()).getByRole("region", { name: "PD2R" });
    await user.click(within(pd2r).getByRole("button", { name: "Open plugins folder" }));
    expect(await screen.findByText("Opened the PD2R plugins folder in File Explorer.")).toBeTruthy();
    expect(screen.queryByText(/created it/)).toBeNull();
  });

  it("a failure shows a red toast that never mentions administrator rights", async () => {
    const { user } = await openSettings("ready", (a) => {
      a.openFolder = reject("access_denied");
    });
    await user.click(tab("Folders and plugins"));
    const pd2r = within(panel()).getByRole("region", { name: "PD2R" });
    await user.click(within(pd2r).getByRole("button", { name: "Open plugins folder" }));
    const alert = await screen.findByRole("alert");
    expect(alert.closest(".toast")?.classList.contains("error")).toBe(true);
    expect(alert.textContent).toMatch(/Couldn't open the PD2R plugins folder: access was denied/);
    expect(alert.textContent).not.toMatch(/admin/i);
  });

  it("has no buttons for products that aren't installed", async () => {
    const { user } = await openSettings("install");
    await user.click(tab("Folders and plugins"));
    expect(within(panel()).queryByRole("region", { name: "PD2R" })).toBeNull();
    expect(within(panel()).getAllByRole("button").map((b) => b.textContent)).toEqual(["Open plugins folder"]);
    expect(within(panel()).getByText(/PD2R, Forever and 09 aren't installed/)).toBeTruthy();
  });
});

describe("Troubleshooting", () => {
  it("reviews the full report, re-previews with the log, and saves with Show in folder", async () => {
    const { api, user } = await openSettings("ready");
    await user.click(tab("Troubleshooting"));
    expect(within(panel()).getByText(/Repair is in PD2R's ⋯ Manage menu/)).toBeTruthy();
    const exportButton = within(panel()).getByRole("button", { name: "Export diagnostics…" });
    await user.click(exportButton);
    const dialog = await screen.findByRole("dialog", { name: "Review diagnostics before saving" });
    const report = within(dialog).getByLabelText("Report text");
    await waitFor(() => expect(report.textContent).toContain("Log included: no"));
    expect(report.textContent).toContain("User: <you>");
    expect(within(dialog).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["Saves", "Account details"]);
    expect(within(dialog).getByText("<you>")).toBeTruthy();

    await user.click(within(dialog).getByRole("checkbox", { name: "Include the launcher log" }));
    await waitFor(() => expect(report.textContent).toContain("Log included: yes"));
    expect(api.calls.filter((c) => c.name === "previewDiagnostics").map((c) => c.args)).toEqual([[false], [true]]);

    await user.click(within(dialog).getByRole("button", { name: "Save as file…" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(api.calls.find((c) => c.name === "saveDiagnostics")?.args).toEqual([true]);
    expect(screen.getByText(/Diagnostics saved\. Nothing was sent/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Show in folder" }));
    expect(names(api)).toContain("showDiagnosticsInFolder");
  });

  it("the review dialog traps focus, closes on Esc and returns focus to Export diagnostics…", async () => {
    const { user } = await openSettings("ready");
    await user.click(tab("Troubleshooting"));
    const exportButton = within(panel()).getByRole("button", { name: "Export diagnostics…" });
    await user.click(exportButton);
    const dialog = await screen.findByRole("dialog");
    await waitFor(() => expect((within(dialog).getByRole("button", { name: "Save as file…" }) as HTMLButtonElement).disabled).toBe(false));
    for (let i = 0; i < 8; i++) {
      await user.tab();
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(exportButton);
  });

  it("locks the log checkbox while saving, so the file matches the previewed text", async () => {
    let finish: (saved: boolean) => void = () => {};
    const { api, user } = await openSettings("ready", (a) => {
      a.saveDiagnostics = (includeLog) => {
        a.calls.push({ name: "saveDiagnostics", args: [includeLog] });
        return new Promise<boolean>((resolve) => (finish = resolve));
      };
    });
    await user.click(tab("Troubleshooting"));
    await user.click(within(panel()).getByRole("button", { name: "Export diagnostics…" }));
    const dialog = await screen.findByRole("dialog");
    const save = within(dialog).getByRole("button", { name: "Save as file…" }) as HTMLButtonElement;
    const log = within(dialog).getByRole("checkbox", { name: "Include the launcher log" }) as HTMLInputElement;
    await waitFor(() => expect(save.disabled).toBe(false));
    await user.click(save);
    expect(log.disabled).toBe(true);
    await user.click(log);
    expect(log.checked).toBe(false);
    expect(api.calls.filter((c) => c.name === "previewDiagnostics").map((c) => c.args)).toEqual([[false]]);
    finish(false);
    await waitFor(() => expect(log.disabled).toBe(false));
    expect(api.calls.find((c) => c.name === "saveDiagnostics")?.args).toEqual([false]);
  });

  it("stays open when the save dialog is cancelled", async () => {
    const { api, user } = await openSettings("ready", (a) => {
      a.saveDiagnostics = () => Promise.resolve(false);
    });
    await user.click(tab("Troubleshooting"));
    await user.click(within(panel()).getByRole("button", { name: "Export diagnostics…" }));
    const dialog = await screen.findByRole("dialog");
    const save = within(dialog).getByRole("button", { name: "Save as file…" }) as HTMLButtonElement;
    await waitFor(() => expect(save.disabled).toBe(false));
    await user.click(save);
    await waitFor(() => expect(save.disabled).toBe(false));
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(names(api)).not.toContain("showDiagnosticsInFolder");
  });
});
