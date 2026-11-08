import { useEffect, useRef, useState } from "react";
import { useApi } from "../api/context";
import type { GameLocation, Inspection, OutcomeReport } from "../api/types";
import { isLauncherError } from "../api/types";
import type { Product } from "../products";
import { Dialog } from "../ui/Dialog";

const STORE: Record<GameLocation["store"], string> = { battle_net: "Battle.net", unknown: "Store not recognised" };

/** `d2r-choose`: a radio list from detectGameLocations, chosen by index. Choosing moves nothing. */
export function LocationDialog({ product, onClose }: { product: Product; onClose: (chosen: boolean) => void }) {
  const api = useApi();
  const [locations, setLocations] = useState<GameLocation[] | null>(null);
  const [picked, setPicked] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const firstRadio = useRef<HTMLInputElement>(null);
  const hasLocations = Boolean(locations?.length);

  // Detection answers after the dialog opened: move focus to the selected installation.
  useEffect(() => {
    if (hasLocations) firstRadio.current?.focus();
  }, [hasLocations]);

  useEffect(() => {
    let live = true;
    api.detectGameLocations().then(
      (l) => live && setLocations(l),
      () => live && setLocations([]),
    );
    return () => {
      live = false;
    };
  }, [api]);

  const use = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.selectGameLocation(picked);
      onClose(true);
    } catch (e) {
      setError(isLauncherError(e) && e.code === "game_running" ? "Quit the game to change this." : "That installation can't be used any more. Pick another, or browse for one.");
      setBusy(false);
    }
  };

  const browse = async () => {
    setError(null);
    try {
      if (await api.chooseGameLocation()) onClose(true);
    } catch (e) {
      setError(
        isLauncherError(e) && e.code === "d2r_not_found"
          ? "That folder doesn't contain D2R.exe. Choose the folder Diablo II: Resurrected is installed in."
          : "That folder can't be used.",
      );
    }
  };

  const count = locations?.length ?? 0;
  return (
    <Dialog
      title="Choose your D2R installation"
      onClose={() => onClose(false)}
      actions={
        <>
          <button type="button" className="btn primary" disabled={count === 0 || busy} onClick={use}>
            Use this installation
          </button>
          <button type="button" className="btn ghost" onClick={() => onClose(false)}>
            Cancel
          </button>
        </>
      }
    >
      <p>
        {locations === null
          ? "Looking for installations…"
          : count === 0
            ? "No installations were found. Browse for the folder that contains D2R.exe."
            : `${count === 1 ? "One installation was" : `${count} installations were`} found. The launcher remembers the one you pick, and you can change it later in Settings. Nothing is moved or copied.`}
      </p>
      {count > 0 && (
        <div className="choices" role="radiogroup" aria-label="D2R installations">
          {locations!.map((l, i) => (
            <label key={l.path}>
              <input
                type="radio"
                name="d2r-location"
                checked={picked === i}
                onChange={() => setPicked(i)}
                ref={i === 0 ? firstRadio : undefined}
              />
              <span>
                <b className="path">{l.path}</b>
                <em>
                  {STORE[l.store]} · build {l.build} ·{" "}
                  {l.installed_products.includes(product.id) ? `${product.short} is installed here` : `${product.short} isn't installed here`}
                </em>
              </span>
            </label>
          ))}
        </div>
      )}
      <button type="button" className="link browse" onClick={browse}>
        Browse for another folder…
      </button>
      {error && (
        <p className="inline-error" role="alert">
          {error}
        </p>
      )}
    </Dialog>
  );
}

export interface SaveBreakingProps {
  product: Product;
  inspection: Inspection;
  installedVersion: string | null;
  onConfirm: () => void;
  onClose: () => void;
}

/** ADR 0001: an accent-topped warning with a required acknowledgement. Saves are never converted. */
export function SaveBreakingDialog({ product, inspection, installedVersion, onConfirm, onClose }: SaveBreakingProps) {
  const api = useApi();
  const [ack, setAck] = useState(false);
  const version = inspection.release.display_version;
  return (
    <Dialog
      title="This update can't use your current characters safely"
      tone="risk"
      onClose={onClose}
      actions={
        <>
          <button type="button" className="btn primary" disabled={!ack} aria-describedby="ack-why" onClick={onConfirm}>
            Update to {version}
          </button>
          <button type="button" className="btn ghost" onClick={onClose} data-autofocus="">
            Not now
          </button>
          {!ack && (
            <span id="ack-why" className="why">
              Tick the box above to update
            </span>
          )}
        </>
      }
    >
      {inspection.save_breaking_notice && (
        <p>
          <b>{inspection.save_breaking_notice}</b>
        </p>
      )}
      <p>
        Your characters stay in the same folder, and the launcher won't convert or delete them. {version} may refuse to load
        them, or change them the first time you play.
        {installedVersion && ` Going back to ${installedVersion} later restores the game files, but not your saves.`}
      </p>
      <p className="tip">
        To keep a copy, back up your {product.short} saves folder first.{" "}
        <button type="button" className="link" onClick={() => void api.openFolder(product.id, "saves")}>
          Open folder
        </button>
      </p>
      <label className="ack">
        <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} required />
        <span>I understand my current characters may not work after updating</span>
      </label>
    </Dialog>
  );
}

export interface UninstallProps {
  product: Product;
  inspection: Inspection;
  /** Set once the uninstall succeeded: the dialog then reports what was kept. */
  report: OutcomeReport | null;
  onConfirm: () => void;
  onClose: () => void;
}

/** Removes the product's files only. Kept: saves and settings. Not touched: D2R, D2RLoader, other products. */
export function UninstallDialog({ product, inspection, report, onConfirm, onClose }: UninstallProps) {
  const api = useApi();
  const kept = report ? report.preserved : inspection.preserved;
  return (
    <Dialog
      title={report ? `${product.short} was uninstalled` : `Uninstall ${product.name}?`}
      tone={report ? "info" : "risk"}
      onClose={onClose}
      actions={
        report ? (
          <button type="button" className="btn primary" onClick={onClose}>
            Done
          </button>
        ) : (
          <>
            <button type="button" className="btn primary" onClick={onConfirm}>
              Uninstall
            </button>
            <button type="button" className="btn ghost" onClick={onClose} data-autofocus="">
              Cancel
            </button>
          </>
        )
      }
    >
      <p>{report ? `${product.short}'s game files were removed.` : `This removes ${product.short}'s game files only.`}</p>
      <ul className="kept">
        <li>
          <b>Kept:</b> your saves and settings.{" "}
          <button type="button" className="link" onClick={() => void api.openFolder(product.id, "saves")}>
            Open saves
          </button>
          {kept.length > 0 && (
            <ul className="paths">
              {kept.map((p) => (
                <li key={p}>
                  <code>{p}</code>
                </li>
              ))}
            </ul>
          )}
        </li>
        {report && report.remaining.length > 0 && (
          <li>
            <b>Left in the {product.short} folder:</b> files the launcher didn't install.{" "}
            <button type="button" className="link" onClick={() => void api.openFolder(product.id, "settings")}>
              Open
            </button>
            <ul className="paths">
              {report.remaining.map((p) => (
                <li key={p}>
                  <code>{p}</code>
                </li>
              ))}
            </ul>
          </li>
        )}
        <li>
          <b>Not touched:</b> Diablo II: Resurrected, D2RLoader and other products.
        </li>
      </ul>
    </Dialog>
  );
}

export interface CleanInstallProps {
  product: Product;
  inspection: Inspection;
  onConfirm: () => void;
  onClose: () => void;
}

/** issue 126: the product folder holds files the offered release doesn't (an older version, or edited
 *  managed files), so Install would be refused. Clean install, with this consent, moves the folder
 *  aside as a labelled backup, installs the release and carries edited settings forward. */
export function CleanInstallDialog({ product, inspection, onConfirm, onClose }: CleanInstallProps) {
  const version = inspection.release.display_version;
  const kept = inspection.preserved;
  const backupOnly = inspection.backup_only.length;
  return (
    <Dialog
      title={`Replace the ${product.short} files in your game folder?`}
      tone="risk"
      onClose={onClose}
      actions={
        <>
          <button type="button" className="btn primary" onClick={onConfirm}>
            Clean install
          </button>
          <button type="button" className="btn ghost" onClick={onClose} data-autofocus="">
            Not now
          </button>
        </>
      }
    >
      <p>
        {product.short} is already in your game folder, but it doesn't match {version}: it's an older version, or some of
        its files were changed. It can't be updated in place.
      </p>
      <ul className="kept">
        <li>
          <b>Backup:</b> clean install moves the whole {product.short} folder aside, next to it in <code>mods</code>, and
          never deletes it. Then it installs {version}.
        </li>
        <li>
          <b>Carried forward:</b> {kept.length > 0 ? "your edited settings." : "nothing; no edited settings were found."}
          {kept.length > 0 && (
            <ul className="paths">
              {kept.map((p) => (
                <li key={p}>
                  <code>{p}</code>
                </li>
              ))}
            </ul>
          )}
        </li>
        {backupOnly > 0 && (
          <li>
            <b>Only in the backup:</b> {backupOnly} other {backupOnly === 1 ? "file stays" : "files stay"} only in the
            backup.
          </li>
        )}
        <li>
          <b>Not touched:</b> your saves, Diablo II: Resurrected, D2RLoader and other products.
        </li>
      </ul>
    </Dialog>
  );
}
