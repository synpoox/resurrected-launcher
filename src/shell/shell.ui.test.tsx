// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../App";
import { ApiContext } from "../api/context";
import { createFixtureApi, sceneByKey, type FixtureApi } from "../api/fixtures";
import { seenKey } from "../product/ProductPage";
import { ToastProvider } from "../ui/Toast";
import { BRANDING } from "./branding";
import { LAST_PRODUCT_KEY, Shell } from "./Shell";

let api: FixtureApi | null = null;

function mount(key: string, prepare?: (api: FixtureApi) => void, tick = 60_000) {
  api = createFixtureApi(key, { tick });
  prepare?.(api);
  const scene = sceneByKey(key)!;
  const user = userEvent.setup();
  render(
    <ApiContext.Provider value={api}>
      <ToastProvider>
        <Shell product={scene.product} open={scene.open} />
      </ToastProvider>
    </ApiContext.Provider>,
  );
  return { api, user };
}

const names = (a: FixtureApi) => a.calls.map((c) => c.name);
const focused = () => document.activeElement as HTMLElement;
const loaded = () => screen.findByRole("tablist");

beforeEach(() => localStorage.clear());
afterEach(() => {
  cleanup();
  api?.dispose();
  api = null;
});

describe("shell", () => {
  it("shows the branding, products in spec order and Settings at the bottom", async () => {
    mount("ready");
    await loaded();
    expect(screen.getByText(BRANDING.name)).toBeTruthy();
    const products = within(screen.getByRole("list", { name: "Products" })).getAllByRole("button");
    expect(products.map((b) => b.textContent)).toEqual([
      "Project Diablo 2: Resurrected",
      "Diablo II: Resurrected Forever",
      "Diablo II: Resurrected 09",
    ]);
    expect(products[0].getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("button", { name: "Settings" })).toBeTruthy();
  });

  it("opens on the last product, or PD2R", async () => {
    localStorage.setItem(LAST_PRODUCT_KEY, "project-09");
    const { user } = mount("ready");
    expect(screen.getByRole("button", { name: "Diablo II: Resurrected 09" }).getAttribute("aria-current")).toBe("page");
    await user.click(screen.getByRole("button", { name: "Diablo II: Resurrected Forever" }));
    expect(localStorage.getItem(LAST_PRODUCT_KEY)).toBe("project-forever");
  });

  it("window controls and the drag region call the window commands", async () => {
    const { api, user } = mount("ready");
    await loaded();
    await user.click(screen.getByRole("button", { name: "Minimise" }));
    await user.click(screen.getByRole("button", { name: "Maximise" }));
    await user.click(screen.getByRole("button", { name: "Close" }));
    await user.pointer({ keys: "[MouseLeft]", target: screen.getByTestId("drag-region") });
    expect(names(api)).toEqual(expect.arrayContaining(["minimizeWindow", "toggleMaximizeWindow", "closeWindow", "startWindowDrag"]));
  });

  it("tab order: window controls → sidebar → tabs → action row → news", async () => {
    const { user } = mount("ready");
    await loaded();
    await screen.findByText(/TCP\/IP co-op testing weekend/);
    const order: string[] = [];
    for (let i = 0; i < 11; i++) {
      await user.tab();
      order.push(focused().getAttribute("aria-label") ?? focused().textContent!.replace(/\s+/g, " ").trim());
    }
    expect(order.slice(0, 8)).toEqual([
      "Minimise",
      "Maximise",
      "Close",
      "Project Diablo 2: Resurrected",
      "Settings",
      "Overview",
      "Play",
      "Manage PD2R",
    ]);
    expect(order.slice(8).every((t) => /ago/.test(t))).toBe(true);
  });

  it("arrow keys move within the sidebar list; Enter activates", async () => {
    const { user } = mount("ready");
    await loaded();
    screen.getByRole("button", { name: "Project Diablo 2: Resurrected" }).focus();
    await user.keyboard("{ArrowDown}");
    expect(focused().textContent).toBe("Diablo II: Resurrected Forever");
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(focused().textContent).toBe("Diablo II: Resurrected 09");
    await user.keyboard("{Enter}");
    expect(focused().getAttribute("aria-current")).toBe("page");
  });

  it("shows sidebar badges with accessible names", async () => {
    mount("blocked");
    expect(await screen.findByRole("img", { name: "error" })).toBeTruthy();
    cleanup();
    mount("update");
    expect(await screen.findByRole("img", { name: "update available" })).toBeTruthy();
    cleanup();
    mount("downloading");
    expect(await screen.findByRole("img", { name: /^\d+% done$/ })).toBeTruthy();
  });

  it("Settings renders src/settings", async () => {
    const { user } = mount("ready");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("heading", { name: "Settings" })).toBeTruthy();
  });
});

describe("product page", () => {
  it("tabs use tab roles; arrow keys switch them", async () => {
    const { user } = mount("ready");
    const overview = within(await loaded()).getByRole("tab", { name: "Overview" });
    expect(overview.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel").getAttribute("aria-labelledby")).toBe(overview.id);
    overview.focus();
    await user.keyboard("{ArrowRight}");
    const notes = screen.getByRole("tab", { name: "Patch Notes" });
    expect(focused()).toBe(notes);
    expect(notes.getAttribute("aria-selected")).toBe("true");
    expect(await screen.findByRole("heading", { name: "PD2R alpha 0.4.2" })).toBeTruthy();
  });

  it("the hero title is always exactly two lines and the art has an accessible name", async () => {
    mount("ready");
    await loaded();
    const title = screen.getByRole("heading", { level: 1 });
    expect(title.children).toHaveLength(2);
    expect(screen.getByRole("img", { name: /Placeholder hero art for Project Diablo 2/ })).toBeTruthy();
  });

  it("products without a release: no tabs, ribbon, description and About", async () => {
    const { api, user } = mount("coming-soon");
    await user.click(await screen.findByRole("button", { name: "About Forever" }));
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.getByText("Coming soon")).toBeTruthy();
    expect(names(api)).toContain("openProductPage");
    expect(api.calls.find((c) => c.name === "openProductPage")!.args).toEqual(["project-forever"]);
  });

  it("news: up to three items, opened by index", async () => {
    const { api, user } = mount("ready", (a) => {
      a.scene.news.pd2r = {
        status: "ok",
        items: ["2026-11-01", "2026-11-05", "2026-10-01", "2026-11-03"].map((d, i) => ({ tag: "Guide", title: `N${i}`, published: d })),
      };
    });
    await screen.findByText("N1");
    const feed = screen.getByRole("region", { name: "News" });
    const items = within(feed).getAllByRole("button");
    expect(items.map((b) => b.textContent!.match(/N\d/)![0])).toEqual(["N1", "N3", "N0"]);
    await user.click(items[0]);
    expect(api.calls.at(-1)).toEqual({ name: "openNewsLink", args: ["pd2r", 1] });
  });

  it("news: empty reads Nothing yet; offline offers Retry", async () => {
    mount("ready", (a) => (a.scene.news.pd2r = { status: "ok", items: [] }));
    expect(await screen.findByText("Nothing yet.")).toBeTruthy();
    cleanup();
    localStorage.clear();
    const { api, user } = mount("offline");
    expect(await screen.findByText(/News needs a connection/)).toBeTruthy();
    await user.click(within(screen.getByRole("region", { name: "News" })).getByRole("button", { name: "Retry" }));
    expect(names(api).filter((n) => n === "getNews")).toHaveLength(2);
  });

  it("Patch Notes render plain text with line breaks; markup is not interpreted", async () => {
    const { user } = mount("ready", (a) => {
      a.scene.notes = [{ ...a.scene.notes[0], notes: "Summary <b>bold</b>\n\nLine one\n<img src=x>" }];
    });
    await user.click(within(await loaded()).getByRole("tab", { name: "Patch Notes" }));
    const body = await screen.findByText(/Line one/);
    expect(body.textContent).toBe("Line one\n<img src=x>");
    expect(document.querySelector(".notes img, .notes b")).toBeNull();
    expect(screen.getByText("Summary <b>bold</b>")).toBeTruthy();
  });

  it("a null note shows the offline line with Retry", async () => {
    const { api, user } = mount("offline");
    await user.click(within(await loaded()).getByRole("tab", { name: "Patch Notes" }));
    await screen.findByText(/Patch notes for newer releases need a connection/);
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(names(api).filter((n) => n === "getReleaseNotes")).toHaveLength(2);
  });

  it("Patch Notes keeps the main action top-left and puts a labelled dot on Overview", async () => {
    const { user } = mount("blocked");
    await user.click(within(await loaded()).getByRole("tab", { name: "Patch Notes" }));
    expect(screen.getByRole("img", { name: "needs attention" }).closest("[role=tab]")!.textContent).toMatch(/Overview/);
    expect(screen.queryByRole("button", { name: "Manage PD2R" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Check again" })).toBeNull();
    const play = screen.getByRole("button", { name: "Play" });
    expect((play as HTMLButtonElement).disabled).toBe(true);
    expect(document.getElementById(play.getAttribute("aria-describedby")!)!.textContent).toMatch(/Can't play/);
  });

  it("the first launch after an update opens Patch Notes once", async () => {
    localStorage.setItem(seenKey("pd2r"), "pd2r-0.4.1-alpha");
    mount("ready");
    const tabs = await loaded();
    expect(within(tabs).getByRole("tab", { name: "Patch Notes" }).getAttribute("aria-selected")).toBe("true");
    expect(localStorage.getItem(seenKey("pd2r"))).toBe("pd2r-0.4.2-alpha");
    cleanup();
    mount("ready");
    expect(within(await loaded()).getByRole("tab", { name: /Overview/ }).getAttribute("aria-selected")).toBe("true");
  });

  it("disabled controls explain why next to them", async () => {
    mount("running");
    const playing = await screen.findByRole("button", { name: /Playing/ });
    expect((playing as HTMLButtonElement).disabled).toBe(true);
    expect(document.getElementById(playing.getAttribute("aria-describedby")!)!.textContent).toMatch(/wait until you quit/);
    cleanup();
    mount("blocked");
    const play = await screen.findByRole("button", { name: "Play" });
    expect(document.getElementById(play.getAttribute("aria-describedby")!)!.textContent).toMatch(/isn't supported/);
  });
});

describe("menus and dialogs", () => {
  it("⋯ menu: arrow keys, Esc closes and returns focus to ⋯", async () => {
    const { api, user } = mount("ready");
    await loaded();
    const trigger = screen.getByRole("button", { name: "Manage PD2R" });
    await user.click(trigger);
    const menu = screen.getByRole("menu");
    expect(within(menu).getAllByRole("menuitem").map((m) => m.textContent)).toEqual([
      "Repair",
      "Open saves folder",
      "Open settings folder",
      "Uninstall…",
    ]);
    expect(focused().textContent).toBe("Repair");
    await user.keyboard("{ArrowDown}");
    expect(focused().textContent).toBe("Open saves folder");
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(focused().textContent).toBe("Uninstall…");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(focused()).toBe(trigger);
    await user.click(trigger);
    await user.keyboard("{ArrowDown}{Enter}");
    expect(api.calls.at(-1)).toEqual({ name: "openFolder", args: ["pd2r", "saves"] });
    expect(focused()).toBe(trigger);
  });

  it("d2r-choose: radio list from detectGameLocations, chosen by index", async () => {
    const { api, user } = mount("d2r-missing");
    const locate = await screen.findByRole("button", { name: "Locate D2R" });
    await user.click(locate);
    const dialog = await screen.findByRole("dialog", { name: "Choose your D2R installation" });
    const radios = await within(dialog).findAllByRole("radio");
    expect(radios).toHaveLength(2);
    expect(within(dialog).getByText(/Battle\.net · build 3\.3\.0 · PD2R is installed here/)).toBeTruthy();
    await user.click(radios[1]);
    await user.click(within(dialog).getByRole("button", { name: "Use this installation" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(api.calls.find((c) => c.name === "selectGameLocation")!.args).toEqual([1]);
    expect(names(api).indexOf("detectGameLocations")).toBeLessThan(names(api).indexOf("selectGameLocation"));
  });

  it("d2r-choose: Browse calls chooseGameLocation and explains a folder without D2R.exe", async () => {
    const { api, user } = mount("d2r-choose", (a) => {
      a.chooseGameLocation = () => Promise.reject({ code: "d2r_not_found", message: "no" });
    });
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Browse for another folder…" }));
    expect((await within(dialog).findByRole("alert")).textContent).toMatch(/doesn't contain D2R\.exe/);
    expect(api.calls.every((c) => c.name !== "selectGameLocation")).toBe(true);
  });

  it("dialogs trap focus, close on Esc and return focus to the opener", async () => {
    const { user } = mount("d2r-missing");
    const locate = await screen.findByRole("button", { name: "Locate D2R" });
    await user.click(locate);
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findAllByRole("radio");
    for (let i = 0; i < 8; i++) {
      await user.tab();
      expect(dialog.contains(focused())).toBe(true);
    }
    await user.tab({ shift: true });
    expect(dialog.contains(focused())).toBe(true);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(focused()).toBe(locate);
  });

  it("save-breaking: the acknowledgement checkbox enables Update", async () => {
    const { api, user } = mount("save-breaking");
    const dialog = await screen.findByRole("dialog", { name: /can't use your current characters/ });
    const update = within(dialog).getByRole("button", { name: "Update to 0.5.0" }) as HTMLButtonElement;
    expect(update.disabled).toBe(true);
    expect(within(dialog).getByText(/changes the save format/)).toBeTruthy();
    await user.click(within(dialog).getByRole("checkbox", { name: /I understand/ }));
    expect(update.disabled).toBe(false);
    await user.click(update);
    const start = api.calls.find((c) => c.name === "startOperation")!;
    expect(start.args[0]).toEqual({ product_id: "pd2r", kind: "update", acknowledge_save_breaking: true });
  });

  it("save-breaking: Not now starts nothing", async () => {
    const { api, user } = mount("save-breaking");
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Not now" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(names(api)).not.toContain("startOperation");
  });

  it("uninstall: Kept and Not touched, then the outcome report", async () => {
    const { user } = mount("uninstall");
    const dialog = await screen.findByRole("dialog", { name: "Uninstall Project Diablo 2: Resurrected?" });
    expect(within(dialog).getByText(/Kept:/)).toBeTruthy();
    expect(within(dialog).getByText(/Diablo II: Resurrected, D2RLoader and other products/)).toBeTruthy();
    expect(within(dialog).getByText("lootfilter/default.filter")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Uninstall" }));
    const done = await screen.findByRole("dialog", { name: "PD2R was uninstalled" });
    expect(within(done).getByText("lootfilter/my-filter.filter")).toBeTruthy();
    expect(within(done).getByText(/Not touched:/)).toBeTruthy();
  });

  it("quit while busy: Cancel download and quit waits for the cancel, then quits", async () => {
    const { api, user } = mount("quit-busy", undefined, 50);
    const dialog = await screen.findByRole("dialog", { name: "Quit while PD2R is downloading?" });
    expect(within(dialog).getByText(/Nothing is installed until a download finishes/)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Cancel download and quit" }));
    expect(within(dialog).getByRole("button", { name: "Stopping safely…" })).toBeTruthy();
    await waitFor(() => expect(names(api)).toContain("quitLauncher"));
    expect(names(api).indexOf("cancelOperation")).toBeLessThan(names(api).indexOf("quitLauncher"));
  });

  it("quit while busy: Keep downloading closes the dialog", async () => {
    const { api, user } = mount("quit-busy");
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Keep downloading" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(names(api)).not.toContain("cancelOperation");
  });
});

describe("journey states", () => {
  it("download progress shows %, size, speed and time left from successive events", async () => {
    // Pin the clock so both samples are exactly a second apart, however slow the runner.
    let clock = Date.now();
    const now = vi.spyOn(Date, "now").mockImplementation(() => clock);
    try {
      const { api } = mount("downloading");
      await screen.findByText(/Downloading alpha 0\.4\.2/);
      expect(screen.getByText(/540 MB of 1\.2 GB/)).toBeTruthy();
      expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("43");
      clock += 1000;
      await act(async () => {
        api.emitProgress((api.scene.progress ?? [])[1]);
      });
    } finally {
      now.mockRestore();
    }
    expect(screen.getByText(/552 MB of 1\.2 GB · 12 MB\/s · about \d+ s left/)).toBeTruthy();
    await userEvent.setup().click(screen.getByRole("button", { name: "Cancel" }));
    expect(names(api!)).toContain("cancelOperation");
  });

  it("repair shows files checked and to restore", async () => {
    mount("repair");
    expect(await screen.findByText(/Checked 1,478 of 2,341 files · 3 to restore · saves and edited filters are kept/)).toBeTruthy();
  });

  it("each state shows its main action", async () => {
    const expectations: [string, RegExp][] = [
      ["d2r-missing", /^Locate D2R$/],
      ["loader-missing", /^Get D2RLoader$/],
      ["install", /^Install$/],
      ["ready", /^Play$/],
      ["update", /^Update$/],
      ["startup-failed", /^Try again$/],
      ["offline", /^Play$/],
      ["offline-empty", /^Try again$/],
      ["applying", /^Cancel safely$/],
    ];
    for (const [key, name] of expectations) {
      mount(key);
      expect(await screen.findByRole("button", { name }), key).toBeTruthy();
      cleanup();
      api!.dispose();
    }
  });

  it("cancelling shows the striped bar and a disabled Cancel that says why", async () => {
    mount("cancelling");
    await screen.findByText("Stopping safely…");
    const cancel = screen.getByRole("button", { name: "Cancel" }) as HTMLButtonElement;
    expect(cancel.disabled).toBe(true);
    expect(document.getElementById(cancel.getAttribute("aria-describedby")!)!.textContent).toMatch(/Don't close the launcher/);
    expect(document.querySelector(".fill.striped")).not.toBeNull();
  });

  it("loader-missing names the allowed versions and opens the official page natively", async () => {
    const { api, user } = mount("loader-missing");
    expect(await screen.findByText(/This release allows 1\.3\.1-beta/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Get D2RLoader" }));
    expect(names(api)).toContain("openLoaderDownloadPage");
  });

  it("install shows the release and free space needed", async () => {
    mount("install");
    expect(await screen.findByText("alpha 0.4.2 · 3.6 GB free space needed")).toBeTruthy();
  });

  it("offline keeps Play and names the verified release", async () => {
    mount("offline");
    expect(await screen.findByText("Offline · alpha 0.4.1 verified · can't check for updates")).toBeTruthy();
  });
});

describe("dev fixture selection", () => {
  it("an unknown ?fixture= shows an error", () => {
    render(<App search="?fixture=nope" dev />);
    expect(screen.getByRole("alert").textContent).toMatch(/Unknown fixture "nope"/);
  });

  it("← → cycle through the fixtures", async () => {
    const user = userEvent.setup();
    render(<App search="?fixture=ready" dev />);
    expect(screen.getByText(/Fixture: ready/)).toBeTruthy();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByText(/Fixture: update/)).toBeTruthy();
    await user.keyboard("{ArrowLeft}{ArrowLeft}");
    expect(screen.getByText(/Fixture: cancelling/)).toBeTruthy();
  });
});
