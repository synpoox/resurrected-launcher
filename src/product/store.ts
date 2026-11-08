import { useCallback, useEffect, useState } from "react";
import type { LauncherApi, LauncherError, ProductState } from "../api/types";
import { isLauncherError } from "../api/types";
import { PRODUCTS } from "../products";
import type { Sample } from "./stats";

export interface ProductData {
  state: ProductState | null;
  error: LauncherError | null;
  samples: Sample[];
}

export type ProductMap = Record<string, ProductData>;

const empty: ProductData = { state: null, error: null, samples: [] };
const withRelease = PRODUCTS.filter((p) => p.hasRelease);

const asError = (e: unknown): LauncherError =>
  isLauncherError(e) ? e : { code: "internal", message: e instanceof Error ? e.message : String(e) };

/**
 * Product state and progress for every product with a release. Re-reads state on
 * `product-state-changed`, and collects progress events while an operation runs.
 */
export function useProducts(api: LauncherApi): { products: ProductMap; refresh: () => Promise<void> } {
  const [products, setProducts] = useState<ProductMap>(() =>
    Object.fromEntries(withRelease.map((p) => [p.id, empty])),
  );

  const refresh = useCallback(async () => {
    await Promise.all(
      withRelease.map(async (p) => {
        try {
          const state = await api.getProductState(p.id);
          setProducts((m) => ({
            ...m,
            [p.id]: { state, error: null, samples: state.running ? m[p.id].samples : [] },
          }));
        } catch (e) {
          setProducts((m) => ({ ...m, [p.id]: { ...m[p.id], error: asError(e) } }));
        }
      }),
    );
  }, [api]);

  useEffect(() => {
    let live = true;
    const stops: (() => void)[] = [];
    const keep = (p: Promise<() => void>) => p.then((un) => (live ? stops.push(un) : un()));
    keep(api.onStateChanged(() => void refresh()));
    keep(
      api.onProgress((event) =>
        setProducts((m) => {
          const cur = m[event.product_id];
          if (!cur) return m;
          const kept = cur.samples.filter((s) => s.event.kind === event.kind).slice(-30);
          return { ...m, [event.product_id]: { ...cur, samples: [...kept, { at: Date.now(), event }] } };
        }),
      ),
    );
    void refresh();
    return () => {
      live = false;
      stops.forEach((un) => un());
    };
  }, [api, refresh]);

  return { products, refresh };
}

export { asError };
