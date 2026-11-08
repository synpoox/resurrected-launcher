import { useEffect, useRef, useState } from "react";
import { useApi } from "../api/context";
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
    let live = true;
    let stop: (() => void) | undefined;
    api
      .onOutcome((o) => {
        if (o.product_id !== product.id) return;
        if (o.status === "cancelled" || o.status === "succeeded") void api.quitLauncher().catch((e) => toast.show({ tone: "error", text: asError(e).message }));
        else {
          toast.show({ tone: "error", text: asError(o.error).message });
          close.current();
        }
      })
      .then((un) => (live ? (stop = un) : un()))
      .then(() => api.cancelOperation(product.id))
      .catch((e) => {
        toast.show({ tone: "error", text: asError(e).message });
        setStopping(false);
      });
    return () => {
      live = false;
      stop?.();
    };
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
