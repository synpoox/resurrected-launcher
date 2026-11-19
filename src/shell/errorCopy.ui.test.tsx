// @vitest-environment jsdom
// #272: player-facing error copy and stale UI states.
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ApiContext } from "../api/context";
import { createFixtureApi, sceneByKey, type FixtureApi } from "../api/fixtures";
import { resetSaveBreakingAcks } from "../product/saveAck";
import { ToastProvider } from "../ui/Toast";
import { Shell } from "./Shell";

let api: FixtureApi | null = null;

function mount(key: string, prepare?: (api: FixtureApi) => void) {
  api = createFixtureApi(key, { tick: 60_000 });
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

const calls = (a: FixtureApi, name: string) => a.calls.filter((c) => c.name === name);

beforeEach(() => {
  localStorage.clear();
  resetSaveBreakingAcks();
});
afterEach(() => {
  cleanup();
  api?.dispose();
  api = null;
});

describe("catalog problems (#272)", () => {
  it("a catalog that needs a newer launcher says so instead of Offline", async () => {
    mount("offline", (a) => {
      a.scene.state.catalog = { status: "failed", error: { code: "launcher_update_required", message: "min_launcher 9" } };
    });
    expect(
      await screen.findByText(
        "alpha 0.4.1 verified · A launcher update is needed for the latest PD2R · Settings → Launcher → Check for updates",
      ),
    ).toBeTruthy();
    expect(screen.queryByText(/^Offline/)).toBeNull();
  });

  it("a bad manifest on a never-installed product has its own copy", async () => {
    mount("offline-empty", (a) => {
      a.scene.state.catalog = { status: "failed", error: { code: "metadata_invalid", message: "bad json at 3:4" } };
    });
    expect(await screen.findByText("The latest PD2R release information couldn't be read · Try again later")).toBeTruthy();
  });
});

describe("error copy (#272)", () => {
  it("a failed action toasts player copy, never the Rust message", async () => {
    const { api, user } = mount("update");
    api.startOperation = () =>
      Promise.reject({ code: "inventory_mismatch", message: "Debug { path: \"C:\\\\x\" } mismatch" });
    await user.click(await screen.findByRole("button", { name: "Update" }));
    expect(await screen.findByText(/aren't the ones the launcher installed/)).toBeTruthy();
    expect(screen.queryByText(/Debug/)).toBeNull();
  });

  it("a tray toast shows human sizes for insufficient_space", async () => {
    const { api } = mount("update");
    await screen.findByRole("tablist");
    act(() =>
      api.emitTrayRequest({
        product_id: "pd2r",
        action: "overview",
        error: { code: "insufficient_space", message: "2147483648 bytes needed, 1073741824 free on C:\\Games" },
      } as never),
    );
    expect(await screen.findByText("Not enough free disk space: 2.0 GB needed, 1.0 GB free. Free up space, then try again.")).toBeTruthy();
  });
});

describe("edited files block Update (#272)", () => {
  it("opens Clean install consent instead of starting", async () => {
    const { api, user } = mount("update", (a) => {
      a.scene.inspections = { update: { conflicts: [{ type: "unknown_file", path: "mod/x.txt" }] } };
    });
    await user.click(await screen.findByRole("button", { name: "Update" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/can't be installed over the files or settings/)).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Clean install" })).toBeTruthy();
    expect(calls(api, "startOperation")).toHaveLength(0);
  });
});

describe("install inspection (#272)", () => {
  it("is read again when the D2R location changes", async () => {
    const { api } = mount("install");
    await screen.findByRole("button", { name: "Install" });
    await waitFor(() => expect(calls(api, "inspectOperation")).toHaveLength(1));
    await act(async () => {
      await api.detectGameLocations();
      await api.selectGameLocation(1);
    });
    await waitFor(() => expect(calls(api, "inspectOperation").length).toBeGreaterThan(1));
  });
});

describe("install inspection, recommended release (#272)", () => {
  it("is read again when the recommended release changes", async () => {
    const { api } = mount("install");
    await screen.findByRole("button", { name: "Install" });
    await waitFor(() => expect(calls(api, "inspectOperation")).toHaveLength(1));
    const read = api.getProductState.bind(api);
    api.getProductState = async (id) => {
      const s = await read(id);
      return { ...s, recommended: { release_id: "pd2r-0.4.3-alpha", display_version: "0.4.3", save_breaking: false } };
    };
    act(() => api.emitOutcome({ status: "cancelled", product_id: "pd2r", kind: "install" }));
    await waitFor(() => expect(calls(api, "inspectOperation").length).toBeGreaterThan(1));
  });
});

describe("Settings (#272)", () => {
  it("Change… waits for a running operation and says which", async () => {
    const { user } = mount("ready", (a) => {
      a.scene.state.running = "update";
    });
    await user.click(await screen.findByRole("button", { name: "Settings" }));
    const change = await screen.findByRole("button", { name: "Change…" });
    expect((change as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText("Wait for the update to finish")).toBeTruthy();
  });

  it("Export diagnostics opens Troubleshooting", async () => {
    const { user } = mount("startup-failed");
    await user.click(await screen.findByRole("button", { name: "Export diagnostics" }));
    const tab = await screen.findByRole("tab", { name: "Troubleshooting" });
    expect(tab.getAttribute("aria-selected")).toBe("true");
  });
});

describe("game start time (#272)", () => {
  it("an unknown start time says it can't tell, not hours ago", async () => {
    mount("running", (a) => {
      a.scene.state.readiness = { status: "running", since: 0 };
    });
    expect(await screen.findByText(/Can't tell whether D2R is running/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Check again" })).toBeTruthy();
    expect(screen.queryByText(/h .* ago/)).toBeNull();
  });
});
