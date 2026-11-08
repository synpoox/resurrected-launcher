import { useEffect, useRef, useState } from "react";
import { useApi } from "../api/context";
import type { DiagnosticsPreview } from "../api/types";
import { Dialog } from "../ui/Dialog";
import { useToast } from "../ui/Toast";

/**
 * Review before saving: the full report text, what is never included, and an optional launcher
 * log that re-previews. Save is enabled only while the shown text matches the checkbox, so the
 * saved file is the text the player reviewed. Nothing is sent.
 */
export function DiagnosticsDialog({ onClose }: { onClose: () => void }) {
  const api = useApi();
  const toast = useToast();
  const [includeLog, setIncludeLog] = useState(false);
  const [preview, setPreview] = useState<{ log: boolean; data: DiagnosticsPreview } | null>(null);
  const [failed, setFailed] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState(false);
  const request = useRef(0);

  useEffect(() => {
    const id = ++request.current;
    api.previewDiagnostics(includeLog).then(
      (data) => {
        if (id !== request.current) return;
        setFailed(false);
        setPreview({ log: includeLog, data });
      },
      () => id === request.current && setFailed(true),
    );
  }, [api, includeLog]);

  const current = preview?.log === includeLog ? preview.data : null;

  const save = async () => {
    setSaving(true);
    setSaveError(false);
    try {
      if (await api.saveDiagnostics(includeLog)) {
        onClose();
        toast.show({
          tone: "info",
          text: "Diagnostics saved. Nothing was sent; share the file yourself if you want help.",
          actions: [{ label: "Show in folder", onSelect: () => void api.showDiagnosticsInFolder() }],
        });
        return;
      }
    } catch {
      setSaveError(true);
    }
    setSaving(false);
  };

  return (
    <Dialog
      title="Review diagnostics before saving"
      onClose={onClose}
      actions={
        <>
          <button type="button" className="btn primary" disabled={!current || saving} onClick={() => void save()}>
            Save as file…
          </button>
          <button type="button" className="btn ghost" onClick={onClose}>
            Cancel
          </button>
        </>
      }
    >
      <p>This is everything in the report. It stays on your PC until you share the file yourself.</p>
      {failed ? (
        <p className="inline-error" role="alert">
          The report couldn't be prepared. Close this and try again.
        </p>
      ) : (
        <pre className="report" tabIndex={0} aria-label="Report text" aria-busy={!current}>
          {(current ?? preview?.data)?.text ?? "Preparing the report…"}
        </pre>
      )}
      {preview && (
        <div className="fine">
          Never included:
          <ul className="never">
            {preview.data.never_included.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
          Your Windows user name is shown as <code>&lt;you&gt;</code>.
        </div>
      )}
      <label className="ack">
        <input
          type="checkbox"
          checked={includeLog}
          disabled={saving}
          onChange={(e) => setIncludeLog(e.target.checked)}
        />
        <span>Include the launcher log</span>
      </label>
      {saveError && (
        <p className="inline-error" role="alert">
          The report couldn't be saved. Try again, or choose another folder.
        </p>
      )}
    </Dialog>
  );
}
