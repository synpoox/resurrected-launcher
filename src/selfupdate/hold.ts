// The restart hold (spec: Launcher self-update → Held restart), derived from product state only.
import type { OperationKind } from "../api/types";
import type { ProductMap } from "../product/store";
import { PRODUCTS } from "../products";

/**
 * Why Restart to update waits. While a game runs, Quit closes the launcher without installing and the
 * update stays ready; while an operation runs, Quit itself is refused or asks first. Once the hold
 * clears, Restart to update or Quit installs it.
 */
export type RestartHold =
  | { reason: "product_running"; product_id: string }
  /** An operation is in progress; `operation` names it so the copy can say which. */
  | { reason: "product_busy"; product_id: string; operation: OperationKind };

/**
 * A running game holds the restart first; otherwise any operation in progress (download, update,
 * repair, uninstall) holds it until it finishes. Products are checked in sidebar order.
 */
export function restartHold(products: ProductMap): RestartHold | null {
  const states = PRODUCTS.map((p) => products[p.id]?.state).filter((s) => s != null);
  const playing = states.find((s) => s.readiness.status === "running");
  if (playing) return { reason: "product_running", product_id: playing.product_id };
  const busy = states.find((s) => s.running);
  if (busy?.running) return { reason: "product_busy", product_id: busy.product_id, operation: busy.running };
  return null;
}
