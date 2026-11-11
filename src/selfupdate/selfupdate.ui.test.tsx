// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../App";
import { ApiContext } from "../api/context";
import { createFixtureApi, sceneByKey, type FixtureApi } from "../api/fixtures";
import { Shell } from "../shell/Shell";
import { TOAST_MS, ToastProvider } from "../ui/Toast";
import { createSelfUpdateFixture, SELF_UPDATE_FIXTURES, type SelfUpdatePhase, type SelfUpdateFixtureApi } from "./api";
import { AUTO_DOWNLOAD_OFF_HINT, AUTO_DOWNLOAD_ON_HINT, holdText, versionStatus } from "./copy";
import { selectSelfUpdate, selfUpdateEnabled } from "./flag";
import { restartHold } from "./hold";
import { SelfUpdateProvider } from "./SelfUpdate";
import type { ProductMap } from "../product/store";

let api: FixtureApi | null = null;
let su: SelfUpdateFixtureApi | null = null;

function mount(phase: string, scene = "ready", tick = 20, prepare?: (api: FixtureApi) => void) {
  api = createFixtureApi(scene, { tick: 60_000 });
  prepare?.(api);
  su = createSelfUpdateFixture(phase, { tick });
  const user = userEvent.setup(vi.isFakeTimers() ? { advanceTimers: vi.advanceTimersByTime } : {});
  render(
    <ApiContext.Provider value={api}>
      <ToastProvider>
        <SelfUpdateProvider api={su}>
          <Shell product={sceneByKey(scene)!.product} />
        </SelfUpdateProvider>
      </ToastProvider>
    </ApiContext.Provider>,
  );
  return { user, su };
}

async function openLauncherSettings(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: /^Settings/ }));
  await user.click(screen.getByRole("tab", { name: "Launcher" }));
  return screen.getByRole("tabpanel");
}

/**
 * Fake timers that move only when the test moves them, so real time never decides a result. A toast's
 * auto-dismiss timer starts in an effect after its text renders; on a slow runner that effect landed
 * after a findBy* had resolved and the clock had been advanced, so the timer never fired (#109).
 */
const useFakeClock = () => {
  vi.useFakeTimers();
  // Testing Library drains a setTimeout(0) after each user event, and advances it only for Jest's fake timers.
  vi.stubGlobal("jest", { advanceTimersByTime: (ms: number) => vi.advanceTimersByTime(ms) });
};
/** Runs pending promise-driven updates, renders and their effects inside act, without moving the clock. */
const settle = () => act(() => vi.advanceTimersByTimeAsync(0));
/** Moves the fake clock, running whatever falls due, and the updates it causes, inside act. */
const advance = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

const notifications = () => screen.getByRole("region", { name: "Notifications" });
const settingsItem = () => screen.getByRole("button", { name: /^Settings/ });
const names = (a: SelfUpdateFixtureApi) => a.calls.map((c) => c.name);

beforeEach(() => localStorage.clear());
afterEach(() => {
  cleanup();
  api?.dispose();
  su?.dispose();
  api = su = null;
  vi.useRealTimers();
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

describe("self-update flag", () => {
  it("is off unless VITE_SELF_UPDATE is 1", () => {
    expect(selfUpdateEnabled()).toBe(false);
    vi.stubEnv("VITE_SELF_UPDATE", "true");
    expect(selfUpdateEnabled()).toBe(false);
    vi.stubEnv("VITE_SELF_UPDATE", "1");
    expect(selfUpdateEnabled()).toBe(true);
  });

  it("off: no fixture is created, so nothing runs", () => {
    expect(selectSelfUpdate("ready", false)).toBeNull();
  });

  it("off: nothing self-update-related renders, and Launcher shows only Settings' own rows", async () => {
    const user = userEvent.setup();
    render(<App search="?fixture=ready&selfupdate=ready" dev />);
    await screen.findByText(/Ready · alpha/);
    expect(within(notifications()).queryByText(/launcher/i)).toBeNull();
    expect(settingsItem().textContent).toBe("Settings");
    const panel = await openLauncherSettings(user);
    expect(within(panel).getAllByRole("switch").map((s) => s.getAttribute("aria-label"))).toEqual(["Start with Windows"]);
    expect(within(panel).queryByText("Launcher version")).toBeNull();
    expect(within(panel).queryByText(/launcher updates/i)).toBeNull();
    expect(screen.queryByText("Fixture: ready · launcher ready · ← →")).toBeNull();
  });

  it("on: ?selfupdate= picks the phase, and an unknown one is a fixture error", async () => {
    render(<App search="?fixture=ready&selfupdate=ready" dev selfUpdate />);
    expect(await within(notifications()).findByText("Launcher 1.3.0 is ready")).toBeTruthy();
    expect(screen.getByText(/launcher ready/)).toBeTruthy();
    cleanup();
    render(<App search="?fixture=ready&selfupdate=nope" dev selfUpdate />);
    expect(screen.getByRole("alert").textContent).toMatch(/Unknown self-update fixture "nope"/);
  });

  it("on outside dev: ignores ?selfupdate= and starts up to date", async () => {
    render(<App search="?selfupdate=nope" dev={false} selfUpdate />);
    await screen.findByText(/Ready · alpha/);
    expect(screen.queryByRole("alert")).toBeNull();
    const s = selectSelfUpdate(null, true);
    expect(s?.ok && s.key).toBe("up-to-date");
    if (s?.ok) s.api.dispose();
  });
});

describe("fixtures", () => {
  it("cover every phase", () => {
    const phases = new Set(SELF_UPDATE_FIXTURES.map((f) => f.status.phase));
    const all: SelfUpdatePhase[] = [
      "up_to_date",
      "available",
      "downloading",
      "ready",
      "restarting",
      "quitting",
      "updated",
      "download_failed",
      "signature_rejected",
    ];
    expect([...phases].sort()).toEqual([...all].sort());
    expect(SELF_UPDATE_FIXTURES.find((f) => f.key === "available")?.autoDownload).toBe(false);
  });

  it("version status copy", () => {
    const st = (key: string) => versionStatus(SELF_UPDATE_FIXTURES.find((f) => f.key === key)!.status);
    expect(st("up-to-date")).toBe("Up to date");
    expect(st("downloading")).toBe("Downloading 1.3.0 · 38%");
    expect(st("available")).toBe("1.3.0 is available");
    expect(st("download-failed")).toBe("Couldn't download 1.3.0");
  });
});

describe("toasts", () => {
  it("the notice never enters the product page", async () => {
    mount("ready");
    await within(notifications()).findByText("Launcher 1.3.0 is ready");
    const main = screen.getByRole("main");
    expect(within(main).queryByText(/launcher 1\.3\.0/i)).toBeNull();
    expect(within(main).queryByRole("button", { name: "Restart to update" })).toBeNull();
  });

  it("ready doesn't auto-dismiss; Later leaves a dot on Settings until Settings opens", async () => {
    useFakeClock();
    const { user } = mount("ready");
    await settle();
    const toast = within(notifications()).getByText("Launcher 1.3.0 is ready").closest(".toast") as HTMLElement;
    expect(within(toast).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Restart to update",
      "What's new",
      "Later",
    ]);
    await advance(TOAST_MS * 3);
    expect(within(notifications()).getByText("Launcher 1.3.0 is ready")).toBeTruthy();
    expect(settingsItem().textContent).toBe("Settings");

    await user.click(within(toast).getByRole("button", { name: "Later" }));
    expect(within(notifications()).queryByText("Launcher 1.3.0 is ready")).toBeNull();
    expect(screen.getByRole("img", { name: "launcher update" })).toBeTruthy();
    expect(settingsItem().getAttribute("aria-label") ?? settingsItem().textContent).toMatch(/Settings/);

    await user.click(settingsItem());
    expect(screen.queryByRole("img", { name: "launcher update" })).toBeNull();
    // Leaving Settings doesn't bring it back.
    await user.click(screen.getByRole("button", { name: /Project Diablo 2/ }));
    expect(screen.queryByRole("img", { name: "launcher update" })).toBeNull();
  });

  it("keyboard: toasts don't take focus; Tab reaches their actions and Enter chooses Later", async () => {
    const { user } = mount("ready");
    await within(notifications()).findByText("Launcher 1.3.0 is ready");
    expect(notifications().contains(document.activeElement)).toBe(false);
    const later = within(notifications()).getByRole("button", { name: "Later" });
    // Toasts come last in the tab order.
    for (let i = 0; i < 40 && document.activeElement !== later; i++) await user.tab();
    expect(document.activeElement).toBe(later);
    await user.keyboard("{Enter}");
    expect(screen.getByRole("img", { name: "launcher update" })).toBeTruthy();
  });

  it("held: Restart to update is disabled and the text says why", async () => {
    mount("ready", "running");
    const text = await within(notifications()).findByText(/Launcher 1.3.0 is ready/);
    expect(text.textContent).toBe(
      "Launcher 1.3.0 is ready. Restarts after you quit PD2R. Quitting the launcher while PD2R runs doesn't install it; it stays ready for later.",
    );
    const restart = within(notifications()).getByRole("button", { name: "Restart to update" }) as HTMLButtonElement;
    expect(restart.getAttribute("aria-disabled")).toBe("true");
    expect(restart.getAttribute("aria-describedby")).toBe(text.id);
    cleanup();
    su?.dispose();
    mount("ready", "downloading");
    expect((await within(notifications()).findByText(/Restarts after PD2R finishes downloading/)).textContent).toBe(
      "Launcher 1.3.0 is ready. Restarts after PD2R finishes downloading. After that, it also installs when you Quit.",
    );
  });

  it("the hold follows product state: quitting the game updates the open toast in place", async () => {
    const changed: (() => void)[] = [];
    mount("ready", "running", 20, (a) => {
      const subscribe = a.onStateChanged.bind(a);
      a.onStateChanged = (h) => (changed.push(h), subscribe(h));
    });
    await within(notifications()).findByText(/Restarts after you quit PD2R/);
    const quit = sceneByKey("ready")!.state;
    api!.getProductState = () => Promise.resolve(structuredClone(quit));
    act(() => changed.forEach((h) => h()));
    await waitFor(() =>
      expect(within(notifications()).getByRole("button", { name: "Restart to update" }).hasAttribute("aria-disabled")).toBe(false),
    );
    expect(within(notifications()).getAllByText(/Launcher 1.3.0 is ready/)).toHaveLength(1);
    expect(within(notifications()).getByText("Launcher 1.3.0 is ready")).toBeTruthy();
  });

  it("held text names what the product is busy with", () => {
    const busy = (operation: "install" | "update" | "repair" | "uninstall" | "adopt" | "clean_install") =>
      holdText({ reason: "product_busy", product_id: "pd2r", operation });
    expect(holdText({ reason: "product_running", product_id: "pd2r" })).toBe("Restarts after you quit PD2R");
    expect(busy("install")).toBe("Restarts after PD2R finishes downloading");
    expect(busy("update")).toBe("Restarts after PD2R finishes downloading");
    expect(busy("repair")).toBe("Restarts after PD2R finishes repairing");
    expect(busy("uninstall")).toBe("Restarts after PD2R finishes uninstalling");
    expect(busy("adopt")).toBe("Restarts after PD2R finishes setting up");
  });

  it("restartHold: a running game first, then any operation in progress, else none", () => {
    const map = (...keys: string[]): ProductMap =>
      Object.fromEntries(keys.map((k) => { const st = sceneByKey(k)!.state; return [st.product_id, { state: st, error: null, samples: [], backups: [] }]; }));
    expect(restartHold(map("ready"))).toBeNull();
    expect(restartHold(map("running"))).toEqual({ reason: "product_running", product_id: "pd2r" });
    expect(restartHold(map("downloading"))).toEqual({ reason: "product_busy", product_id: "pd2r", operation: "install" });
    expect(restartHold({})).toBeNull();
  });

  it("available auto-dismisses like other information toasts, without leaving the dot", async () => {
    useFakeClock();
    mount("available");
    await settle();
    await advance(TOAST_MS - 1);
    expect(within(notifications()).getByText("Launcher 1.3.0 is available")).toBeTruthy();
    await advance(1);
    expect(within(notifications()).queryByText("Launcher 1.3.0 is available")).toBeNull();
    expect(screen.queryByRole("img", { name: "launcher update" })).toBeNull();
  });

  it("a failure that follows Try again directly (no downloading event) still shows its toast", async () => {
    const { user, su } = mount("download-failed");
    const failNow = () => {
      su.emit({ phase: "download_failed", current: "1.2.0", next: "1.3.0" });
      return Promise.resolve();
    };
    su.download = failNow;
    await user.click(within(await within(notifications()).findByRole("alert")).getByRole("button", { name: "Try again" }));
    expect(within(notifications()).getAllByRole("alert")).toHaveLength(1);
    // From Settings too, while the failure toast is still open: it is replaced, not duplicated or lost.
    const panel = await openLauncherSettings(user);
    await user.click(within(panel).getByRole("button", { name: "Try again" }));
    expect(within(notifications()).getAllByRole("alert")).toHaveLength(1);
    // And after Later closed it.
    await user.click(within(notifications()).getByRole("button", { name: "Later" }));
    await user.click(within(panel).getByRole("button", { name: "Try again" }));
    expect(within(notifications()).getAllByRole("alert")).toHaveLength(1);
  });

  it("keeps focus on Restart to update when a hold starts while it is focused", async () => {
    const changed: (() => void)[] = [];
    mount("ready", "ready", 20, (a) => {
      const subscribe = a.onStateChanged.bind(a);
      a.onStateChanged = (h) => (changed.push(h), subscribe(h));
    });
    const restart = await within(notifications()).findByRole("button", { name: "Restart to update" });
    restart.focus();
    const playing = sceneByKey("running")!.state;
    api!.getProductState = () => Promise.resolve(structuredClone(playing));
    act(() => changed.forEach((h) => h()));
    await waitFor(() => expect(restart.getAttribute("aria-disabled")).toBe("true"));
    expect(document.activeElement).toBe(restart);
  });

  it("a failure after Try again shows its error toast again", async () => {
    useFakeClock();
    const { user, su } = mount("download-failed");
    await settle();
    const first = within(notifications()).getByRole("alert");
    await user.click(within(first).getByRole("button", { name: "Try again" }));
    expect(within(notifications()).queryByRole("alert")).toBeNull();
    act(() => su.emit({ phase: "downloading", current: "1.2.0", next: "1.3.0", percent: 12 }));
    act(() => su.emit({ phase: "download_failed", current: "1.2.0", next: "1.3.0" }));
    expect(within(notifications()).getByRole("alert").textContent).toContain("Couldn't download the launcher update");
  });

  it("a signature rejected again after Try again in Settings shows its error toast again", async () => {
    useFakeClock();
    const { user, su } = mount("signature-rejected");
    await settle();
    await user.click(within(within(notifications()).getByRole("alert")).getByRole("button", { name: "Later" }));
    const panel = await openLauncherSettings(user);
    await user.click(within(panel).getByRole("button", { name: "Try again" }));
    act(() => su.emit({ phase: "signature_rejected", current: "1.2.0", next: "1.3.0" }));
    expect(within(notifications()).getByRole("alert").textContent).toContain("Launcher update rejected");
  });

  it("available offers Download, What's new and Later; Download starts the download silently", async () => {
    useFakeClock();
    const { user, su } = mount("available");
    await settle();
    const toast = within(notifications()).getByText("Launcher 1.3.0 is available").closest(".toast") as HTMLElement;
    expect(within(toast).getAllByRole("button").map((b) => b.textContent)).toEqual(["Download", "What's new", "Later"]);
    await user.click(within(toast).getByRole("button", { name: "Download" }));
    await advance(20 * 3);
    expect(names(su)).toContain("download");
    expect(within(notifications()).queryByText(/Launcher/)).toBeNull();
    expect(settingsItem().textContent).toBe("Settings");
    // Ready arrives once the download finishes (four 20 ms steps, then ready).
    await advance(20);
    expect(within(notifications()).getByText("Launcher 1.3.0 is ready")).toBeTruthy();
  });

  it("What's new opens the release notes and doesn't leave the dot", async () => {
    const { user, su } = mount("available");
    await user.click(await within(notifications()).findByRole("button", { name: "What's new" }));
    expect(names(su)).toContain("openWhatsNew");
    expect(screen.queryByRole("img", { name: "launcher update" })).toBeNull();
  });

  it("Restart to update shows the restarting screen, then Updated with What's new, which auto-dismisses", async () => {
    useFakeClock();
    const { user } = mount("ready");
    await settle();
    await user.click(within(notifications()).getByRole("button", { name: "Restart to update" }));
    const screenEl = within(screen.getByRole("main")).getByRole("status");
    expect(screenEl.textContent).toContain("Updating the launcher…");
    expect(screenEl.textContent).toContain("Back in a few seconds.");
    expect(screenEl.querySelector("img")?.getAttribute("alt")).toBe("");
    await advance(100);
    expect(screen.queryByText("Updating the launcher…")).toBeNull();
    const updated = within(notifications()).getByText("Updated to launcher 1.3.0").closest(".toast") as HTMLElement;
    expect(within(updated).getAllByRole("button").map((b) => b.textContent)).toEqual(["What's new", "Dismiss"]);
    await advance(TOAST_MS);
    expect(within(notifications()).queryByText("Updated to launcher 1.3.0")).toBeNull();
    expect(settingsItem().textContent).toBe("Settings");
  });

  it("updated with release notes opens What's new once, instead of the Updated toast (#189)", async () => {
    const { user, su } = mount("updated-notes");
    const dialog = await screen.findByRole("dialog", { name: "What's new in launcher 1.3.0" });
    expect(dialog.textContent).toContain("Faster patch checks and a clearer restart prompt.");
    expect(within(dialog).getByRole("heading", { name: "Improvements" })).toBeTruthy();
    expect(within(dialog).getByText("Restart to update").tagName).toBe("STRONG");
    expect(within(notifications()).queryByText("Updated to launcher 1.3.0")).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Release page" }));
    expect(names(su)).toContain("openWhatsNew");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();

    // The next start of the same version doesn't open it again.
    cleanup();
    su.dispose();
    mount("updated-notes");
    await act(() => new Promise((r) => setTimeout(r, 50)));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("updated with notes already seen falls back to the Updated toast (#189)", async () => {
    localStorage.setItem("launcher.whatsNewSeen", "1.3.0");
    mount("updated-notes");
    await within(notifications()).findByText("Updated to launcher 1.3.0");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("updated with notes falls back to the Updated toast when storage throws (#189)", async () => {
    const real = Storage.prototype.getItem;
    const get = vi.spyOn(Storage.prototype, "getItem").mockImplementation(function (this: Storage, key: string) {
      if (key === "launcher.whatsNewSeen") throw new Error("denied");
      return real.call(this, key);
    });
    try {
      mount("updated-notes");
      await within(notifications()).findByText("Updated to launcher 1.3.0");
      expect(screen.queryByRole("dialog")).toBeNull();
    } finally {
      get.mockRestore();
    }
  });

  it("updated without notes keeps the Updated toast and opens no dialog", async () => {
    mount("updated");
    await within(notifications()).findByText("Updated to launcher 1.3.0");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("installing on Quit shows Quit copy, not the restart promise, and no toast", async () => {
    mount("quitting");
    const screenEl = await within(screen.getByRole("main")).findByRole("status");
    expect(screenEl.textContent).toContain("Updating the launcher…");
    expect(screenEl.textContent).toContain("The launcher closes now. Start it again whenever you like.");
    expect(screenEl.textContent).not.toContain("Back in a few seconds.");
    expect(within(notifications()).queryByText(/launcher/i)).toBeNull();
  });

  it("download failed: an error toast with Try again that stays until dismissed", async () => {
    useFakeClock();
    const { user, su } = mount("download-failed");
    await settle();
    const alert = within(notifications()).getByRole("alert");
    expect(alert.textContent).toContain("Couldn't download the launcher update. Launcher 1.2.0 keeps working.");
    await advance(TOAST_MS * 2);
    expect(within(notifications()).getByRole("alert")).toBe(alert);
    await user.click(within(alert).getByRole("button", { name: "Try again" }));
    expect(names(su)).toContain("download");
  });

  it("signature rejected: Export diagnostics opens the diagnostics dialog and leaves no dot", async () => {
    const { user } = mount("signature-rejected");
    const alert = await within(notifications()).findByRole("alert");
    expect(alert.textContent).toContain(
      "Launcher update rejected. 1.3.0 didn't pass its signature check, so it was deleted and nothing was installed.",
    );
    await user.click(within(alert).getByRole("button", { name: "Export diagnostics" }));
    expect(screen.getByRole("dialog", { name: "Review diagnostics before saving" })).toBeTruthy();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("img", { name: "launcher update" })).toBeNull();
  });

  it("Later on a failure leaves the dot", async () => {
    const { user } = mount("download-failed");
    await user.click(within(await within(notifications()).findByRole("alert")).getByRole("button", { name: "Later" }));
    expect(screen.getByRole("img", { name: "launcher update" })).toBeTruthy();
  });

  it("downloading, restarting and up to date are silent", async () => {
    for (const phase of ["downloading", "up-to-date"]) {
      mount(phase);
      await waitFor(() => expect(su!.calls.length).toBeGreaterThan(0));
      expect(within(notifications()).queryByText(/launcher/i)).toBeNull();
      cleanup();
      su?.dispose();
    }
  });
});

describe("Settings → Launcher", () => {
  it("lists Start with Windows, Launcher version and automatic downloads, in that order", async () => {
    const { user } = mount("up-to-date");
    const panel = await openLauncherSettings(user);
    const labels = [...panel.querySelectorAll(".srow-text b")].map((b) => b.textContent);
    expect(labels.slice(0, 3)).toEqual(["Start with Windows", "Launcher version", "Download launcher updates automatically"]);
    expect(await within(panel).findByText("Launcher 1.2.0 · Up to date")).toBeTruthy();
    expect(within(panel).getByText(AUTO_DOWNLOAD_ON_HINT)).toBeTruthy();
  });

  it("Check for updates shows Checking…, then downloads in the background", async () => {
    useFakeClock();
    const { user, su } = mount("up-to-date");
    await settle();
    const panel = await openLauncherSettings(user);
    const check = within(panel).getByRole("button", { name: "Check for updates" });
    await user.click(check);
    await advance(19);
    expect(within(panel).getByText("Launcher 1.2.0 · Checking…")).toBeTruthy();
    expect(check.getAttribute("aria-disabled")).toBe("true");
    // The check resolves at 20 ms and starts the download; its first step is a 0 ms timer, which runs 1 ms later.
    await advance(2);
    expect(within(panel).getByText("Launcher 1.2.0 · Downloading 1.3.0 · 12%")).toBeTruthy();
    expect(names(su)).toContain("checkForUpdates");
    expect(within(notifications()).queryByText(/Launcher/)).toBeNull();
    await advance(20 * 4);
    expect(within(panel).getByRole("button", { name: "Restart to update" })).toBeTruthy();
  });

  it("available: Download replaces Check for updates; the switch is off with its hint, and Space turns it on", async () => {
    useFakeClock();
    const { user, su } = mount("available");
    await settle();
    const panel = await openLauncherSettings(user);
    expect(within(panel).getByText("Launcher 1.2.0 · 1.3.0 is available")).toBeTruthy();
    expect(within(panel).queryByRole("button", { name: "Check for updates" })).toBeNull();
    const sw = within(panel).getByRole("switch", { name: "Download launcher updates automatically" });
    expect(sw.getAttribute("aria-checked")).toBe("false");
    expect(within(panel).getByText(AUTO_DOWNLOAD_OFF_HINT)).toBeTruthy();
    sw.focus();
    await user.keyboard(" ");
    expect(sw.getAttribute("aria-checked")).toBe("true");
    expect(within(panel).getByText(AUTO_DOWNLOAD_ON_HINT)).toBeTruthy();
    expect(su.calls.find((c) => c.name === "setAutoDownload")?.args).toEqual([true]);
    within(panel).getByRole("button", { name: "Download" }).focus();
    await user.keyboard("{Enter}");
    await advance(1);
    expect(within(panel).getByText(/Downloading 1\.3\.0 · 12%/)).toBeTruthy();
  });

  it("rapid toggles: only the latest request decides, and a failure reverts to the last confirmed value", async () => {
    const { user, su } = mount("up-to-date");
    const pending: { on: boolean; resolve: () => void; reject: () => void }[] = [];
    su.setAutoDownload = (on) =>
      new Promise<void>((resolve, reject) => pending.push({ on, resolve, reject: () => reject(new Error("nope")) }));
    const panel = await openLauncherSettings(user);
    const sw = within(panel).getByRole("switch", { name: "Download launcher updates automatically" });
    await waitFor(() => expect(sw.getAttribute("aria-checked")).toBe("true"));
    await user.click(sw); // A: off
    await user.click(sw); // B: on
    await user.click(sw); // C: off
    expect(pending.map((p) => p.on)).toEqual([false, true, false]);
    await act(async () => pending[2].resolve());
    await act(async () => pending[0].reject()); // a late failure of an older request
    await act(async () => pending[1].resolve());
    expect(sw.getAttribute("aria-checked")).toBe("false");
    expect(within(notifications()).queryByRole("alert")).toBeNull();

    await user.click(sw); // D: on, fails: back to C's confirmed off
    await act(async () => pending[3].reject());
    expect(sw.getAttribute("aria-checked")).toBe("false");
    expect(within(notifications()).getByRole("alert").textContent).toMatch(/Couldn't change automatic launcher downloads/);
  });

  it("a failed switch change reverts and says so", async () => {
    const { user, su } = mount("up-to-date");
    su.setAutoDownload = () => Promise.reject(new Error("nope"));
    const panel = await openLauncherSettings(user);
    const sw = within(panel).getByRole("switch", { name: "Download launcher updates automatically" });
    await waitFor(() => expect(sw.getAttribute("aria-checked")).toBe("true"));
    await user.click(sw);
    await waitFor(() => expect(sw.getAttribute("aria-checked")).toBe("true"));
    expect(within(notifications()).getByRole("alert").textContent).toMatch(/Couldn't change automatic launcher downloads/);
  });

  it("ready: Restart to update, reachable with the keyboard", async () => {
    useFakeClock();
    const { user, su } = mount("ready");
    await settle();
    const panel = await openLauncherSettings(user);
    const restart = within(panel).getByRole("button", { name: "Restart to update" });
    restart.focus();
    await user.keyboard("{Enter}");
    expect(names(su)).toContain("restartToUpdate");
    expect(screen.getByText("Updating the launcher…")).toBeTruthy();
  });

  it("held: Restart to update is disabled with the reason next to it", async () => {
    const { user } = mount("ready", "downloading");
    const panel = await openLauncherSettings(user);
    const restart = (await within(panel).findByRole("button", { name: "Restart to update" })) as HTMLButtonElement;
    expect(restart.getAttribute("aria-disabled")).toBe("true");
    const why = document.getElementById(restart.getAttribute("aria-describedby")!);
    expect(why?.textContent).toBe("Restarts after PD2R finishes downloading");
  });

  it("failures offer Try again", async () => {
    for (const phase of ["download-failed", "signature-rejected"]) {
      const { user, su } = mount(phase);
      const panel = await openLauncherSettings(user);
      await user.click(await within(panel).findByRole("button", { name: "Try again" }));
      expect(names(su)).toContain("download");
      cleanup();
      su.dispose();
    }
  });

  it("downloading: Check for updates is disabled and described by the progress", async () => {
    const { user } = mount("downloading", "ready", 60_000);
    const panel = await openLauncherSettings(user);
    const check = (await within(panel).findByRole("button", { name: "Check for updates" })) as HTMLButtonElement;
    expect(check.getAttribute("aria-disabled")).toBe("true");
    expect(document.getElementById(check.getAttribute("aria-describedby")!)?.textContent).toBe(
      "Launcher 1.2.0 · Downloading 1.3.0 · 38%",
    );
  });
});
