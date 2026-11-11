import { useEffect, useId, useRef } from "react";
import { Spinner } from "../ui/pending";

export interface ProgressProps {
  label: string;
  percent: number | null;
  detail: string;
  cancelLabel: string;
  /** Omit to disable Cancel; `detail` then says why. */
  onCancel?: () => void;
  /** The striped bar of a safe stop. */
  striped?: boolean;
  /** Patch Notes: a 250px bar with Cancel and no detail line. */
  compact?: boolean;
  /** Cancel was chosen and hasn't answered yet (issue 138): a spinner, and no second request. */
  busy?: boolean;
}

/** Replaces Play while an operation runs: label, %, bar, detail and Cancel. */
export function Progress({ label, percent, detail, cancelLabel, onCancel, striped, compact, busy }: ProgressProps) {
  const detailId = useId();
  const fill = useRef<HTMLDivElement>(null);
  const value = percent ?? 0;
  useEffect(() => {
    // CSSOM, not a style attribute: the CSP has `style-src 'self'`.
    // A scale, not a width: it eases on the compositor without relayout.
    fill.current?.style.setProperty("transform", `scaleX(${value / 100})`);
  }, [value]);
  return (
    <div className={`progress${compact ? " compact" : ""}`}>
      <div className="progress-top">
        <b>{label}</b>
        {percent !== null && <span className="pct">{percent}%</span>}
      </div>
      <div
        className="track"
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent ?? undefined}
      >
        <div ref={fill} className={`fill${striped ? " striped" : ""}`} />
      </div>
      <div className="progress-bottom">
        <span id={detailId} className="detail">
          {detail}
        </span>
        <button
          type="button"
          className="btn ghost small"
          disabled={!onCancel}
          aria-describedby={onCancel ? undefined : detailId}
          aria-busy={busy || undefined}
          aria-disabled={busy || undefined}
          onClick={busy ? undefined : onCancel}
        >
          {busy && <Spinner />}
          {cancelLabel}
        </button>
      </div>
    </div>
  );
}
