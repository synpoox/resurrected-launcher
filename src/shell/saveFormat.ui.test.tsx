// @vitest-environment jsdom
// Save compatibility prompt before an update (issue 213). The launcher decides from release
// metadata (`Inspection.breaks_saves`); these scenes stand in for the native answer.
import { cleanup, render, screen, within } from "@testing-library/react";
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

const started = (a: FixtureApi) => a.calls.filter((c) => c.name === "startOperation");

beforeEach(() => {
  localStorage.clear();
  resetSaveBreakingAcks();
});
afterEach(() => {
  cleanup();
  api?.dispose();
  api = null;
});

describe("save compatibility prompt (issue 213)", () => {
  it("a compatible update shows no prompt and starts", async () => {
    const { api, user } = mount("update");
    await user.click(await screen.findByRole("button", { name: /^Update$/ }));
    await screen.findByRole("tablist");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(started(api)).toHaveLength(1);
    expect(started(api)[0].args[0]).toEqual({ product_id: "pd2r", kind: "update" });
  });

  it("a missing save-format signal is compatible: the inspection says no break and no prompt opens", async () => {
    const { api, user } = mount("update");
    const insp = await api.inspectOperation({ product_id: "pd2r", kind: "update" });
    expect(insp.breaks_saves).toBe(false);
    expect(insp.save_breaking_notice).toBeNull();
    await user.click(await screen.findByRole("button", { name: /^Update$/ }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("a breaking update asks for acknowledgement and starts only after it", async () => {
    const { api, user } = mount("save-breaking");
    const dialog = await screen.findByRole("dialog", { name: /can't use your current characters/ });
    expect(within(dialog).getByText(/changes the save format/)).toBeTruthy();
    expect(started(api)).toHaveLength(0);
    await user.click(within(dialog).getByRole("checkbox", { name: /I understand/ }));
    await user.click(within(dialog).getByRole("button", { name: "Update to 0.5.0" }));
    expect(started(api)[0].args[0]).toEqual({ product_id: "pd2r", kind: "update", acknowledge_save_breaking: true });
  });

  it("an update that skips a breaking release is prompted although the target has no notice of its own", async () => {
    const { api, user } = mount("save-breaking-skipped");
    const dialog = await screen.findByRole("dialog", { name: /can't use your current characters/ });
    expect(within(dialog).getByText("Your character saves won't be compatible with the latest patch (0.5.1).")).toBeTruthy();
    expect(started(api)).toHaveLength(0);
    await user.click(within(dialog).getByRole("checkbox", { name: /I understand/ }));
    await user.click(within(dialog).getByRole("button", { name: "Update to 0.5.1" }));
    expect(started(api)[0].args[0]).toMatchObject({ kind: "update", acknowledge_save_breaking: true });
  });

  it("Not now leaves the install untouched and asks again next time", async () => {
    const { api, user } = mount("save-breaking");
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Not now" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(started(api)).toHaveLength(0);
    expect((await api.getProductState("pd2r")).installed?.display_version).toBe("0.4.1");
    await user.click(await screen.findByRole("button", { name: /^Update$/ }));
    expect(await screen.findByRole("dialog", { name: /can't use your current characters/ })).toBeTruthy();
    expect(started(api)).toHaveLength(0);
  });

  it("a clean install across a save-format change asks for acknowledgement after consent, then starts with both (issue 215)", async () => {
    const { api, user } = mount("clean-install", (a) => {
      const clean = a.scene.inspections?.clean_install;
      a.scene.inspections = { ...a.scene.inspections, clean_install: { ...clean, breaks_saves: true } };
    });
    const consent = await screen.findByRole("dialog", { name: /Replace the PD2R files/ });
    await user.click(await within(consent).findByRole("button", { name: "Clean install" }));
    const dialog = await screen.findByRole("dialog", { name: /This clean install can't use your current characters/ });
    expect(started(api)).toHaveLength(0);
    expect(within(dialog).getByRole("button", { name: /^Clean install 0\.4\.2$/ }).hasAttribute("disabled")).toBe(true);
    await user.click(within(dialog).getByRole("checkbox", { name: /after doing a clean install/ }));
    await user.click(within(dialog).getByRole("button", { name: /^Clean install 0\.4\.2$/ }));
    expect(started(api)[0].args[0]).toEqual({
      product_id: "pd2r",
      kind: "clean_install",
      consent_clean_install: true,
      acknowledge_save_breaking: true,
    });
  });

  it("an install across a save-format change shows the same prompt (issue 215)", async () => {
    const { api, user } = mount("install", (a) => {
      a.scene.inspections = { ...a.scene.inspections, install: { ...a.scene.inspections?.install, breaks_saves: true } };
    });
    await user.click(await screen.findByRole("button", { name: /^Install$/ }));
    const dialog = await screen.findByRole("dialog", { name: /This install can't use your current characters/ });
    expect(started(api)).toHaveLength(0);
    await user.click(within(dialog).getByRole("checkbox", { name: /after installing/ }));
    await user.click(within(dialog).getByRole("button", { name: /^Install 0\.4\.2$/ }));
    expect(started(api)[0].args[0]).toEqual({ product_id: "pd2r", kind: "install", acknowledge_save_breaking: true });
  });

  it("an acknowledged target release is not asked again after a failed start", async () => {
    const seen: unknown[] = [];
    const { user } = mount("save-breaking", (a) => {
      const real = a.startOperation.bind(a);
      let first = true;
      seen.length = 0;
      a.startOperation = (request, insp) => {
        seen.push(request);
        if (first) {
          first = false;
          return Promise.reject({ code: "network", message: "offline" });
        }
        return real(request, insp);
      };
    });
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("checkbox", { name: /I understand/ }));
    await user.click(within(dialog).getByRole("button", { name: "Update to 0.5.0" }));
    await user.click(await screen.findByRole("button", { name: /^Update$/ }));
    expect(screen.queryByRole("dialog")).toBeNull();
    const calls = seen as { acknowledge_save_breaking?: boolean }[];
    expect(calls).toHaveLength(2);
    expect(calls[1]).toEqual({ product_id: "pd2r", kind: "update", acknowledge_save_breaking: true });
  });
});
