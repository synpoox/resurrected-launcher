// The restart hold (spec: Launcher self-update → Held restart), derived from product state only.
import type { ProductMap } from "../product/store";
import { PRODUCTS } from "../products";

/** Why Restart to update waits. The update still installs on the next Quit. */
export interface RestartHold {
  reason: "product_running" | "product_downloading";
  product_id: string;
}

/**
 * A running game holds the restart first; otherwise any operation in progress (download, update,
 * repair, uninstall) holds it until it finishes. Products are checked in sidebar order.
 */
export function restartHold(products: ProductMap): RestartHold | null {
  const states = PRODUCTS.map((p) => products[p.id]?.state).filter((s) => s != null);
  const playing = states.find((s) => s.readiness.status === "running");
  if (playing) return { reason: "product_running", product_id: playing.product_id };
  const busy = states.find((s) => s.running);
  if (busy) return { reason: "product_downloading", product_id: busy.product_id };
  return null;
}
