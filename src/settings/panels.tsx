import { useEffect, useId, useState } from "react";
import { useApi } from "../api/context";
import type { Folder, ProductState } from "../api/types";
import { isLauncherError } from "../api/types";
import { HERO_ART } from "../art";
import { LocationDialog } from "../product/dialogs";
import { PRODUCTS, type Product } from "../products";
import { Notice } from "../ui/Notice";
import { Switch } from "../ui/Switch";
import { useToast } from "../ui/Toast";
import {
  folderFailureText,
  folderName,
  GAME_RUNNING_REASON,
  STATE_FAILED_REASON,
  isPluginFolder,
  recheckText,
  STORE_NAME,
} from "./copy";
import { DiagnosticsDialog } from "./DiagnosticsDialog";
import { Row, Section } from "./Row";

export interface PanelProps {
  /** The product whose D2R location and folders Settings shows (PD2R today). */
  product: Product;
  state: ProductState | null;
}

// ---- Game ----

export function GamePanel({ product, state, stateFailed = false }: PanelProps & { stateFailed?: boolean }) {
  const api = useApi();
  const [choosing, setChoosing] = useState(false);
  const [recheck, setRecheck] = useState<ProductState | null>(null);
  const location = state?.location ?? null;
  const running = state?.readiness.status === "running";
  // The location can't be changed until product state has loaded; say so if loading failed.
  const reason = running ? GAME_RUNNING_REASON : state === null && stateFailed ? STATE_FAILED_REASON : null;
  const why = useId();

  const closed = async (chosen: boolean) => {
    setChoosing(false);
    if (!chosen) return;
    try {
      setRecheck(await api.getProductState(product.id));
    } catch {
      setRecheck(null);
    }
  };

  return (
    <Section title="Game">
      <Row
        label="Diablo II: Resurrected location"
        hint={
          state === null ? (
            stateFailed ? "The D2R location couldn't be read." : "Checking…"
          ) : location ? (
            <>
              <code className="path">{location.path}</code>
              <br />
              {STORE_NAME[location.store]} · build {location.build}
            </>
          ) : (
            "D2R wasn't found on this PC. Choose the folder that contains D2R.exe."
          )
        }
      >
        <button
          type="button"
          className="btn ghost small"
          disabled={running || state === null}
          aria-describedby={reason ? why : undefined}
          onClick={() => setChoosing(true)}
        >
          {location ? "Change…" : "Choose…"}
        </button>
        {reason && (
          <span id={why} className="why">
            {reason}
          </span>
        )}
      </Row>
      {recheck && (
        <Notice>
          <b>Rechecked {recheck.location ? <code className="path">{recheck.location.path}</code> : "D2R"}.</b>{" "}
          {recheckText(recheck, product)} Nothing was moved, copied or deleted.
        </Notice>
      )}
      {choosing && <LocationDialog product={product} onClose={(chosen) => void closed(chosen)} />}
    </Section>
  );
}

// ---- Launcher ----

export function LauncherPanel() {
  const api = useApi();
  const toast = useToast();
  const [startup, setStartup] = useState<boolean | null>(null);

  useEffect(() => {
    let live = true;
    api.getStartWithWindows().then(
      (on) => live && setStartup(on),
      () => live && setStartup(false),
    );
    return () => {
      live = false;
    };
  }, [api]);

  const change = async (on: boolean) => {
    setStartup(on);
    try {
      await api.setStartWithWindows(on);
    } catch {
      setStartup(!on);
      toast.show({ tone: "error", text: "Couldn't change Start with Windows. Try again." });
    }
  };

  return (
    <>
      <Section title="Launcher">
        <Row
          label="Start with Windows"
          hint="Opens in the tray when you sign in to Windows, with no window. Closing the window always keeps the launcher in the tray; use Quit in the tray menu to exit."
        >
          <Switch label="Start with Windows" checked={startup ?? false} onChange={(on) => void change(on)} />
        </Row>
        {/* #78 adds the Launcher version and automatic-download rows here. */}
      </Section>
      <Section title="About and credits">
        <ul className="credits">
          {PRODUCTS.map((p) => {
            const art = HERO_ART[p.hero];
            return (
              <li key={p.id}>
                <b>{p.name}</b>
                <span>
                  {art.name} · {art.source}
                </span>
                <span className="hint">
                  {art.placeholder
                    ? "Placeholder art, not game art. It is replaced before a public release."
                    : "Promotional art, not mod gameplay."}
                </span>
              </li>
            );
          })}
        </ul>
      </Section>
    </>
  );
}

// ---- Folders and plugins ----

/** localStorage: plugin folders the player has opened before (a missing one is created on first open). */
export const openedKey = (productId: string | null) => `launcher.pluginsOpened.${productId ?? "global"}`;

function firstOpen(key: string): boolean {
  try {
    if (localStorage.getItem(key)) return false;
    localStorage.setItem(key, "1");
    return true;
  } catch {
    return false;
  }
}

export function FoldersPanel({ product, state }: PanelProps) {
  const api = useApi();
  const toast = useToast();
  const installed = Boolean(state?.installed);
  const hasLocation = Boolean(state?.location);

  const open = async (productId: string | null, folder: Folder) => {
    const owner = productId ? product : null;
    const name = folderName(folder, owner);
    try {
      await api.openFolder(productId, folder);
      const created = isPluginFolder(folder) && firstOpen(openedKey(productId));
      toast.show({
        tone: "info",
        text: created
          ? `Opened the ${name} in File Explorer. If it didn't exist yet, the launcher created it empty.`
          : `Opened the ${name} in File Explorer.`,
      });
    } catch (e) {
      toast.show({ tone: "error", text: folderFailureText(isLauncherError(e) ? e : null, name, owner) });
    }
  };

  const openButton = (productId: string | null, folder: Folder, label: string) => (
    <button type="button" className="btn ghost small" onClick={() => void open(productId, folder)}>
      {label}
    </button>
  );

  const missing = [...(installed ? [] : [product]), ...PRODUCTS.filter((p) => !p.hasRelease)].map((p) => p.short);
  return (
    <>
      {installed && (
        <Section title={product.short}>
          <Row label={`${product.short} saves`} hint="Your characters. Back them up before an update.">
            {openButton(product.id, "saves", "Open folder")}
          </Row>
          <Row label={`${product.short} settings and filters`} hint="Your loot filters and the mod's settings.">
            {openButton(product.id, "settings", "Open folder")}
          </Row>
          <Row
            label={`${product.short} plugins`}
            hint={
              <>
                Loads only with {product.short}. Its <code>core-*</code> plugins are part of the mod: leave them in
                place.
              </>
            }
          >
            {openButton(product.id, "product_plugins", "Open plugins folder")}
          </Row>
        </Section>
      )}
      <Section title="All products">
        <Row label="Global plugins" hint="Loads for every product.">
          {hasLocation ? (
            openButton(null, "global_plugins", "Open plugins folder")
          ) : (
            <span className="why">Choose the D2R location in Game first</span>
          )}
        </Row>
      </Section>
      <p className="fine">
        The launcher doesn't list, enable or disable plugins. Plugins are allowed offline and in direct TCP/IP co-op
        only. {joinNames(missing)} {missing.length === 1 ? "isn't" : "aren't"} installed, so{" "}
        {missing.length === 1 ? "it has" : "they have"} no folder buttons.
      </p>
    </>
  );
}

function joinNames(names: string[]): string {
  return names.length <= 1 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;
}

// ---- Troubleshooting ----

export function TroubleshootingPanel({ product }: { product: Product }) {
  const [reviewing, setReviewing] = useState(false);
  return (
    <Section title="Troubleshooting">
      <Row
        label="Diagnostics"
        hint="A report of launcher logs and your setup, for bug reports. Nothing is sent: you review it, then save it as a file to share yourself."
      >
        <button type="button" className="btn ghost small" onClick={() => setReviewing(true)}>
          Export diagnostics…
        </button>
      </Row>
      <p className="fine">
        For game problems, Repair is in {product.short}'s ⋯ Manage menu next to Play.
      </p>
      {reviewing && <DiagnosticsDialog onClose={() => setReviewing(false)} />}
    </Section>
  );
}
