import { useEffect, useRef, useState } from "react";
import { useApi } from "../api/context";
import { subscribe } from "../api/subscribe";
import type { Product } from "../products";
import { Dialog } from "../ui/Dialog";
import { useToast } from "../ui/Toast";
import { asError } from "../product/store";

export interface QuitDialogProps {
  product: Product;
  hasInstalled: boolean;
  onClose: () => void;
}

/** Quit while busy: Cancel download and quit (waits for the safe stop) or Keep downloading. */
export function QuitDialog({ product, hasInstalled, onClose }: QuitDialogProps) {
  const api = useApi();
  const toast = useToast();
  const [stopping, setStopping] = useState(false);
  // Read through a ref, so a parent re-render (live progress) doesn't restart the stop below.
  const close = useRef(onClose);
  useEffect(() => {
    close.current = onClose;
  }, [onClose]);

  // Once per stop: listen for the outcome, then cancel; quit after the cancelled (or finished) outcome.
  useEffect(() => {
    if (!stopping) return;
    const fail = (e: unknown) => toast.show({ tone: "error", text: asError(e).message });
    // A refused quit (for example while recovery runs) shows why and lets the player choose again.
    const quit = () =>
      void api.quitLauncher().catch((e) => {
        fail(e);
        setStopping(false);
      });
    const sub = subscribe(
      api.onOutcome((o) => {
        if (o.product_id !== product.id) return;
        if (o.status === "cancelled" || o.status === "succeeded") quit();
        else {
          fail(o.error);
          close.current();
        }
      }),
    );
    sub.ready
      .then(() => api.cancelOperation(product.id))
      .catch(async (e) => {
        // Refused because nothing is running: the operation already ended, so it's safe to quit.
        const state = await api.getProductState(product.id).catch(() => null);
        if (state && !state.running) return quit();
        fail(e);
        setStopping(false);
      });
    return sub.stop;
  }, [stopping, api, product.id, toast]);

  return (
    <Dialog
      title={`Quit while ${product.short} is downloading?`}
      onClose={stopping ? undefined : onClose}
      actions={
        <>
          <button type="button" className="btn primary" disabled={stopping} aria-describedby="quit-why" onClick={() => setStopping(true)}>
            {stopping ? "Stopping safely…" : "Cancel download and quit"}
          </button>
          <button type="button" className="btn ghost" disabled={stopping} onClick={onClose} data-autofocus="">
            Keep downloading
          </button>
        </>
      }
    >
      <p id="quit-why">
        {stopping
          ? "Quitting waits until it's safe to stop. Don't close the launcher."
          : hasInstalled
            ? "The download stops and the installed release stays playable."
            : "The download stops. Nothing is installed until a download finishes."}
      </p>
    </Dialog>
  );
}
