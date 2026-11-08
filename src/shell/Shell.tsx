import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useApi } from "../api/context";
import { subscribe } from "../api/subscribe";
import type { SceneOpen } from "../api/fixtures";
import { DEFAULT_PRODUCT_ID, PRODUCTS, productById } from "../products";
import { badgeOf, journeyOf, type Badge } from "../product/journey";
import { ProductPage, type TrayFocus, type Tab } from "../product/ProductPage";
import { progressStats } from "../product/stats";
import { useProducts } from "../product/store";
import { RestartingScreen } from "../selfupdate/RestartingScreen";
import { useSelfUpdate } from "../selfupdate/SelfUpdate";
import { Settings } from "../settings";
import { useToast } from "../ui/Toast";
import { QuitDialog } from "./QuitDialog";
import { Sidebar, type View } from "./Sidebar";
import { TitleBar } from "./TitleBar";

/** localStorage: the product the window last showed. */
export const LAST_PRODUCT_KEY = "launcher.lastProduct";

function initialProduct(preferred?: string): string {
  if (preferred && productById(preferred)) return preferred;
  const last = localStorage.getItem(LAST_PRODUCT_KEY);
  return last && productById(last) ? last : DEFAULT_PRODUCT_ID;
}

export interface ShellProps {
  /** Opens on this product instead of the last one (fixture scenes). */
  product?: string;
  open?: SceneOpen;
  /** Dev fixtures: open the first product page on this tab. */
  tab?: Tab;
  /** Shows "About <product>" (default on); App turns it off in live mode until #80. */
  showAbout?: boolean;
}

/** The window: seamless title bar, sidebar, and the product page or Settings in the main area. */
export function Shell({ product, open, tab, showAbout = true }: ShellProps) {
  const api = useApi();
  const { products, refresh, dismissBackups } = useProducts(api);
  const [view, setView] = useState<View>(() => ({ kind: "product", id: initialProduct(product) }));
  /** The Quit dialog: for the product a tray Quit asked about, or (null) the busy one. */
  const [quit, setQuit] = useState<{ productId: string | null } | null>(open === "quit" ? { productId: null } : null);
  // A scene's dialog or menu opens once, on the product the window opened on.
  const [sceneOpen, setSceneOpen] = useState(open);
  const [trayFocus, setTrayFocus] = useState<TrayFocus | null>(null);
  const traySeq = useRef(0);
  const toast = useToast();

  const select = useCallback((v: View) => {
    if (v.kind === "product") localStorage.setItem(LAST_PRODUCT_KEY, v.id);
    setSceneOpen(undefined);
    setTrayFocus(null);
    setView(v);
  }, []);

  // Tray requests: Rust has already shown the window (except for Quit while busy, which asks here).
  useEffect(() => {
    return subscribe(
      api.onTrayRequest((r) => {
        if (!productById(r.product_id)) return;
        if (r.action === "quit_while_busy") {
          setQuit({ productId: r.product_id });
          return;
        }
        select({ kind: "product", id: r.product_id });
        traySeq.current += 1;
        setTrayFocus({ product_id: r.product_id, action: r.action, seq: traySeq.current });
        if (r.error) toast.show({ tone: "error", text: r.error.message });
        void refresh();
      }),
    ).stop;
  }, [api, select, toast, refresh]);

  const badges = useMemo(() => {
    const out: Record<string, Badge | null> = {};
    for (const p of PRODUCTS) {
      const d = products[p.id];
      if (!d?.state) {
        out[p.id] = null;
        continue;
      }
      const j = journeyOf(d.state, d.samples.at(-1)?.event ?? null);
      out[p.id] = badgeOf(j, progressStats(d.samples).percent);
    }
    return out;
  }, [products]);

  // Opening Settings clears the launcher-update dot (and keeps it clear while Settings is open).
  const selfUpdate = useSelfUpdate();
  const clearDot = selfUpdate?.clearDot;
  const dot = selfUpdate?.dot;
  useEffect(() => {
    if (view.kind === "settings" && dot) clearDot?.();
  }, [view.kind, dot, clearDot]);

  const closeQuit = useCallback(() => setQuit(null), []);

  const current = view.kind === "product" ? productById(view.id)! : null;
  const busy =
    (quit?.productId ? productById(quit.productId) : undefined) ?? PRODUCTS.find((p) => products[p.id]?.state?.running);
  return (
    <div className="window">
      <TitleBar />
      <Sidebar view={view} badges={badges} onSelect={select} />
      <main className="main" aria-label={current ? current.name : "Settings"}>
        {current ? (
          <ProductPage
            key={current.id}
            product={current}
            data={products[current.id]}
            refresh={refresh}
            open={sceneOpen}
            tab={sceneOpen === open ? tab : undefined}
            trayFocus={trayFocus?.product_id === current.id ? trayFocus : undefined}
            onOpenSettings={() => select({ kind: "settings" })}
            onDismissBackups={() => dismissBackups(current.id)}
            showAbout={showAbout}
          />
        ) : (
          <Settings />
        )}
        <RestartingScreen />
        {quit && busy && (
          <QuitDialog
            product={busy}
            hasInstalled={Boolean(products[busy.id]?.state?.installed)}
            onClose={closeQuit}
          />
        )}
      </main>
    </div>
  );
}
