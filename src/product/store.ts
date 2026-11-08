import { useCallback, useEffect, useMemo, useState } from "react";
import type { LauncherApi, LauncherError, ProductState } from "../api/types";
import { isLauncherError } from "../api/types";
import { subscribe } from "../api/subscribe";
import { PRODUCTS } from "../products";
import type { Sample } from "./stats";

export interface ProductData {
  state: ProductState | null;
  error: LauncherError | null;
  samples: Sample[];
  /** Clean-install backup locations from the last outcome report (display only), until dismissed. */
  backups: string[];
}

export type ProductMap = Record<string, ProductData>;

const empty: ProductData = { state: null, error: null, samples: [], backups: [] };
const withRelease = PRODUCTS.filter((p) => p.hasRelease);

const asError = (e: unknown): LauncherError =>
  isLauncherError(e) ? e : { code: "internal", message: e instanceof Error ? e.message : String(e) };

const startups = new WeakMap<LauncherApi, Promise<void>>();

/**
 * Startup, once per API: recover every product before its state (and so Play) is shown, then
 * refresh the catalog in the background. A failed recovery still shows state: readiness says
 * `recovery_required` and Check again retries.
 */
export function startup(api: LauncherApi): Promise<void> {
  let done = startups.get(api);
  if (!done) {
    done = Promise.all(withRelease.map((p) => api.recoverProduct(p.id).catch(() => undefined))).then(() => {
      // A failed refresh is reported in state: every product's `catalog` says `failed`.
      api.refreshCatalog().catch(() => undefined);
    });
    startups.set(api, done);
  }
  return done;
}

/**
 * Coalesces state reads: at most one in flight and one queued, so reads never overlap and an older
 * reply can't overwrite a newer one. Every read waits for startup recovery first.
 */
function coalesce(read: () => Promise<void>, gate: () => Promise<void>): () => Promise<void> {
  let inFlight: Promise<void> | null = null;
  let queued: Promise<void> | null = null;
  const run = (): Promise<void> => {
    const p = gate()
      .then(read)
      .finally(() => {
        if (inFlight === p) inFlight = null;
      });
    inFlight = p;
    return p;
  };
  return () => {
    if (queued) return queued;
    if (inFlight) {
      queued = inFlight.then(() => {
        queued = null;
        return run();
      });
      return queued;
    }
    return run();
  };
}

export interface Products {
  products: ProductMap;
  /** Re-reads every product's state, after startup recovery and coalesced with event re-reads. */
  refresh: () => Promise<void>;
  dismissBackups: (productId: string) => void;
}

/**
 * Product state and progress for every product with a release. After startup recovery, re-reads
 * state on `product-state-changed`, `operation-progress` and `operation-outcome`, collects
 * progress events while an operation runs, and keeps clean-install backup locations until dismissed.
 */
export function useProducts(api: LauncherApi): Products {
  const [products, setProducts] = useState<ProductMap>(() =>
    Object.fromEntries(withRelease.map((p) => [p.id, empty])),
  );

  const refresh = useMemo(() => {
    const readAll = async () => {
      await Promise.all(
        withRelease.map(async (p) => {
          try {
            const state = await api.getProductState(p.id);
            setProducts((m) => ({
              ...m,
              [p.id]: { ...m[p.id], state, error: null, samples: state.running ? m[p.id].samples : [] },
            }));
          } catch (e) {
            setProducts((m) => ({ ...m, [p.id]: { ...m[p.id], error: asError(e) } }));
          }
        }),
      );
    };
    return coalesce(readAll, () => startup(api));
  }, [api]);

  const dismissBackups = useCallback(
    (id: string) => setProducts((m) => (m[id] ? { ...m, [id]: { ...m[id], backups: [] } } : m)),
    [],
  );

  useEffect(() => {
    const reread = () => void refresh();
    const sub = subscribe(
      api.onStateChanged(reread),
      api.onProgress((event) => {
        setProducts((m) => {
          const cur = m[event.product_id];
          if (!cur) return m;
          const kept = cur.samples.filter((s) => s.event.kind === event.kind).slice(-30);
          return { ...m, [event.product_id]: { ...cur, samples: [...kept, { at: Date.now(), event }] } };
        });
        reread();
      }),
      api.onOutcome((o) => {
        if (o.status === "succeeded" && o.report.clean_install_backups.length > 0) {
          const backups = o.report.clean_install_backups;
          setProducts((m) => (m[o.product_id] ? { ...m, [o.product_id]: { ...m[o.product_id], backups } } : m));
        }
        reread();
      }),
    );
    reread();
    return sub.stop;
  }, [api, refresh]);

  return { products, refresh, dismissBackups };
}

export { asError };
