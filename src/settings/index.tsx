// Settings and troubleshooting (spec: Settings, layout B): a 220px sub-nav and one panel at a time.
import { useId, useState } from "react";
import { useApi } from "../api/context";
import { useProducts } from "../product/store";
import { DEFAULT_PRODUCT_ID, productById } from "../products";
import { arrowKeys } from "../ui/focus";
import { FoldersPanel, GamePanel, LauncherPanel, TroubleshootingPanel } from "./panels";
import "./settings.css";

export type SettingsTab = "game" | "launcher" | "folders" | "troubleshooting";

export const SETTINGS_TABS: readonly [SettingsTab, string][] = [
  ["game", "Game"],
  ["launcher", "Launcher"],
  ["folders", "Folders and plugins"],
  ["troubleshooting", "Troubleshooting"],
];

export function Settings({ initialTab = "game" }: { initialTab?: SettingsTab }) {
  const api = useApi();
  const { products } = useProducts(api);
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const base = useId();
  // Only PD2R has a release; its D2R location is the launcher's D2R location.
  const product = productById(DEFAULT_PRODUCT_ID)!;
  const state = products[product.id]?.state ?? null;

  return (
    <div className="settings">
      <div className="subnav">
        <h1>Settings</h1>
        <div
          role="tablist"
          aria-label="Settings sections"
          aria-orientation="vertical"
          onKeyDown={arrowKeys("[role=tab]", "vertical", true)}
        >
          {SETTINGS_TABS.map(([key, name]) => (
            <button
              key={key}
              type="button"
              role="tab"
              id={`${base}-${key}-tab`}
              aria-selected={tab === key}
              aria-controls={`${base}-${key}`}
              tabIndex={tab === key ? 0 : -1}
              onClick={() => setTab(key)}
            >
              {name}
            </button>
          ))}
        </div>
      </div>
      <div className="spanel" role="tabpanel" id={`${base}-${tab}`} aria-labelledby={`${base}-${tab}-tab`}>
        {tab === "game" && <GamePanel product={product} state={state} />}
        {tab === "launcher" && <LauncherPanel />}
        {tab === "folders" && <FoldersPanel product={product} state={state} />}
        {tab === "troubleshooting" && <TroubleshootingPanel product={product} />}
      </div>
    </div>
  );
}
