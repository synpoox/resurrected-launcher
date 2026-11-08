// Shared mounting for the slice 4 gate (#81): the real Shell on the fixture API, optionally under the
// self-update provider (flag on), plus an axe-core run limited to the WCAG 2.2 A/AA rules.
import { readFileSync } from "node:fs";
import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { ApiContext } from "../api/context";
import { createFixtureApi, sceneByKey, type FixtureApi } from "../api/fixtures";
import { createSelfUpdateFixture, type SelfUpdateFixtureApi } from "../selfupdate/api";
import { SelfUpdateProvider } from "../selfupdate/SelfUpdate";
import { Shell } from "../shell/Shell";
import { ToastProvider } from "../ui/Toast";

export interface Mounted {
  api: FixtureApi;
  su: SelfUpdateFixtureApi | null;
  user: ReturnType<typeof userEvent.setup>;
}

let live: Mounted[] = [];

export interface MountOptions {
  tab?: "notes";
  /** A self-update phase key: mounts under SelfUpdateProvider, as App does with the flag on. */
  selfUpdate?: string;
  tick?: number;
  prepare?: (api: FixtureApi) => void;
}

export function mount(key: string, options: MountOptions = {}): Mounted {
  const api = createFixtureApi(key, { tick: options.tick ?? 60_000 });
  options.prepare?.(api);
  const scene = sceneByKey(key)!;
  const su = options.selfUpdate ? createSelfUpdateFixture(options.selfUpdate, { tick: 60_000 }) : null;
  const shell = <Shell product={scene.product} open={scene.open} tab={options.tab} />;
  render(
    <ApiContext.Provider value={api}>
      <ToastProvider>{su ? <SelfUpdateProvider api={su}>{shell}</SelfUpdateProvider> : shell}</ToastProvider>
    </ApiContext.Provider>,
  );
  const mounted = { api, su, user: userEvent.setup() };
  live.push(mounted);
  return mounted;
}

export function disposeAll() {
  for (const m of live) {
    m.api.dispose();
    m.su?.dispose();
  }
  live = [];
}

/** The shipped index.html, which the per-state runs don't render. */
export const INDEX_HTML = readFileSync("index.html", "utf8");
const INDEX_LANG = INDEX_HTML.match(/<html[^>]* lang="([^"]*)"/)?.[1] ?? "";

/** WCAG 2.0/2.1/2.2 level A and AA rules. */
const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22a", "wcag22aa"];

/**
 * axe-core violations on the whole document, as "rule: target" lines. Colour contrast is off here
 * because jsdom has no layout or canvas to resolve layered colours; `contrast.gate.test.ts` measures
 * the token pairs instead, and the manual pass checks the rendered window. `document-title` is a
 * property of index.html, not of a state, so `a11y.gate.test.tsx` checks it once against that file.
 */
export async function violations(): Promise<string[]> {
  // The document element comes from index.html in the app; jsdom starts from a bare one.
  document.documentElement.lang = INDEX_LANG;
  const result = await axe.run(document, {
    runOnly: { type: "tag", values: TAGS },
    rules: { "color-contrast": { enabled: false }, "document-title": { enabled: false } },
    resultTypes: ["violations"],
  });
  return result.violations.flatMap((v) => v.nodes.map((n) => `${v.id}: ${n.target.join(" ")} — ${v.help}`));
}
