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
import { AUTO_DOWNLOAD_OFF_HINT, AUTO_DOWNLOAD_ON_HINT, versionStatus } from "./copy";
import { selectSelfUpdate, selfUpdateEnabled } from "./flag";
import { SelfUpdateProvider } from "./SelfUpdate";

let api: FixtureApi | null = null;
let su: SelfUpdateFixtureApi | null = null;

function mount(phase: string, scene = "ready", tick = 20) {
  api = createFixtureApi(scene, { tick: 60_000 });
  su = createSelfUpdateFixture(phase, { tick });
  const user = userEvent.setup();
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
    expect(selectSelfUpdate("?selfupdate=ready", true, false)).toBeNull();
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

  it("on outside dev: ignores ?selfupdate= and starts up to date", () => {
    const s = selectSelfUpdate("?selfupdate=ready", false, true);
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
      "updated",
      "download_failed",
      "signature_rejected",
    ];
    expect([...phases].sort()).toEqual([...all].sort());
    const holds = SELF_UPDATE_FIXTURES.flatMap((f) => (f.status.phase === "ready" && f.status.hold ? [f.status.hold.reason] : []));
    expect(holds).toEqual(["product_running", "product_downloading"]);
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
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    mount("ready");
    const toast = (await within(notifications()).findByText("Launcher 1.3.0 is ready")).closest(".toast") as HTMLElement;
    expect(within(toast).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Restart to update",
      "What's new",
      "Later",
    ]);
    act(() => void vi.advanceTimersByTime(TOAST_MS * 3));
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
    mount("held-running", "running");
    const text = await within(notifications()).findByText(/Launcher 1.3.0 is ready/);
    expect(text.textContent).toBe("Launcher 1.3.0 is ready. Restarts after you quit PD2R, or it installs the next time you Quit.");
    const restart = within(notifications()).getByRole("button", { name: "Restart to update" }) as HTMLButtonElement;
    expect(restart.disabled).toBe(true);
    expect(restart.getAttribute("aria-describedby")).toBe(text.id);
    cleanup();
    su?.dispose();
    mount("held-downloading", "downloading");
    expect(await within(notifications()).findByText(/Restarts after PD2R finishes downloading/)).toBeTruthy();
  });

  it("a hold that ends updates the open toast in place", async () => {
    const { su } = mount("held-running", "running");
    await within(notifications()).findByText(/Restarts after you quit PD2R/);
    act(() => su.emit({ phase: "ready", current: "1.2.0", next: "1.3.0", hold: null }));
    expect(within(notifications()).getAllByText(/Launcher 1.3.0 is ready/)).toHaveLength(1);
    expect((within(notifications()).getByRole("button", { name: "Restart to update" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("available offers Download, What's new and Later; Download starts the download silently", async () => {
    const { user, su } = mount("available");
    const toast = (await within(notifications()).findByText("Launcher 1.3.0 is available")).closest(".toast") as HTMLElement;
    expect(within(toast).getAllByRole("button").map((b) => b.textContent)).toEqual(["Download", "What's new", "Later"]);
    await user.click(within(toast).getByRole("button", { name: "Download" }));
    expect(names(su)).toContain("download");
    expect(within(notifications()).queryByText(/Launcher/)).toBeNull();
    expect(settingsItem().textContent).toBe("Settings");
    // Ready arrives once the download finishes.
    expect(await within(notifications()).findByText("Launcher 1.3.0 is ready")).toBeTruthy();
  });

  it("What's new opens the notes and leaves the dot", async () => {
    const { user, su } = mount("available");
    await user.click(await within(notifications()).findByRole("button", { name: "What's new" }));
    expect(names(su)).toContain("openWhatsNew");
    expect(screen.getByRole("img", { name: "launcher update" })).toBeTruthy();
  });

  it("Restart to update shows the restarting screen, then Updated with What's new, which auto-dismisses", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    mount("ready");
    await user.click(await within(notifications()).findByRole("button", { name: "Restart to update" }));
    const screenEl = within(screen.getByRole("main")).getByRole("status");
    expect(screenEl.textContent).toContain("Updating the launcher…");
    expect(screenEl.textContent).toContain("Back in a few seconds.");
    expect(screenEl.querySelector("img")?.getAttribute("alt")).toBe("");
    act(() => void vi.advanceTimersByTime(100));
    expect(screen.queryByText("Updating the launcher…")).toBeNull();
    const updated = within(notifications()).getByText("Updated to launcher 1.3.0").closest(".toast") as HTMLElement;
    expect(within(updated).getAllByRole("button").map((b) => b.textContent)).toEqual(["What's new", "Dismiss"]);
    act(() => void vi.advanceTimersByTime(TOAST_MS + 10));
    expect(within(notifications()).queryByText("Updated to launcher 1.3.0")).toBeNull();
    expect(settingsItem().textContent).toBe("Settings");
  });

  it("download failed: an error toast with Try again that stays until dismissed", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const { su } = mount("download-failed");
    const alert = await within(notifications()).findByRole("alert");
    expect(alert.textContent).toContain("Couldn't download the launcher update. Launcher 1.2.0 keeps working.");
    act(() => void vi.advanceTimersByTime(TOAST_MS * 2));
    expect(within(notifications()).getByRole("alert")).toBe(alert);
    await user.click(within(alert).getByRole("button", { name: "Try again" }));
    expect(names(su)).toContain("download");
  });

  it("signature rejected: Export diagnostics opens the diagnostics dialog; Later leaves the dot", async () => {
    const { user } = mount("signature-rejected");
    const alert = await within(notifications()).findByRole("alert");
    expect(alert.textContent).toContain(
      "Launcher update rejected. 1.3.0 didn't pass its signature check, so it was deleted and nothing was installed.",
    );
    await user.click(within(alert).getByRole("button", { name: "Export diagnostics" }));
    expect(screen.getByRole("dialog", { name: "Review diagnostics before saving" })).toBeTruthy();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
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
    const { user, su } = mount("up-to-date");
    const panel = await openLauncherSettings(user);
    const check = await within(panel).findByRole("button", { name: "Check for updates" });
    await user.click(check);
    expect(within(panel).getByText("Launcher 1.2.0 · Checking…")).toBeTruthy();
    expect((check as HTMLButtonElement).disabled).toBe(true);
    expect(await within(panel).findByText(/Downloading 1\.3\.0 · \d+%/)).toBeTruthy();
    expect(names(su)).toContain("checkForUpdates");
    expect(within(notifications()).queryByText(/Launcher/)).toBeNull();
    expect(await within(panel).findByRole("button", { name: "Restart to update" })).toBeTruthy();
  });

  it("available: Download replaces Check for updates; the switch is off with its hint, and Space turns it on", async () => {
    const { user, su } = mount("available");
    const panel = await openLauncherSettings(user);
    expect(await within(panel).findByText("Launcher 1.2.0 · 1.3.0 is available")).toBeTruthy();
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
    expect(await within(panel).findByText(/Downloading 1\.3\.0/)).toBeTruthy();
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
    const { user, su } = mount("ready");
    await within(notifications()).findByText("Launcher 1.3.0 is ready");
    const panel = await openLauncherSettings(user);
    const restart = within(panel).getByRole("button", { name: "Restart to update" });
    restart.focus();
    await user.keyboard("{Enter}");
    expect(names(su)).toContain("restartToUpdate");
    expect(screen.getByText("Updating the launcher…")).toBeTruthy();
  });

  it("held: Restart to update is disabled with the reason next to it", async () => {
    const { user } = mount("held-downloading", "downloading");
    const panel = await openLauncherSettings(user);
    const restart = (await within(panel).findByRole("button", { name: "Restart to update" })) as HTMLButtonElement;
    expect(restart.disabled).toBe(true);
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
    expect(check.disabled).toBe(true);
    expect(document.getElementById(check.getAttribute("aria-describedby")!)?.textContent).toBe(
      "Launcher 1.2.0 · Downloading 1.3.0 · 38%",
    );
  });
});
