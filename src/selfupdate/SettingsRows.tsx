// Settings → Launcher: Launcher version and Download launcher updates automatically (spec: Settings, #28).
import { useId, useState } from "react";
import { Row } from "../settings/Row";
import { Switch } from "../ui/Switch";
import { AUTO_DOWNLOAD_OFF_HINT, AUTO_DOWNLOAD_ON_HINT, holdText, versionStatus } from "./copy";
import { useSelfUpdate, type SelfUpdate } from "./SelfUpdate";

/** Renders nothing when the self-update flag is off. */
export function SelfUpdateRows() {
  const su = useSelfUpdate();
  if (!su) return null;
  return (
    <>
      <VersionRow su={su} />
      <Row label="Download launcher updates automatically" hint={su.autoDownload === false ? AUTO_DOWNLOAD_OFF_HINT : AUTO_DOWNLOAD_ON_HINT}>
        <Switch
          label="Download launcher updates automatically"
          checked={su.autoDownload ?? true}
          onChange={su.setAutoDownload}
        />
      </Row>
    </>
  );
}

function VersionRow({ su }: { su: SelfUpdate }) {
  const [checking, setChecking] = useState(false);
  const hintId = useId();
  const whyId = useId();
  const s = su.status;

  const check = async () => {
    setChecking(true);
    try {
      await su.check();
    } finally {
      setChecking(false);
    }
  };

  const hint = s ? `Launcher ${s.current} · ${checking ? "Checking…" : versionStatus(s)}` : "Checking…";
  let control;
  if (!s) control = null;
  else if (s.phase === "available")
    control = (
      <button type="button" className="btn ghost small" onClick={su.download}>
        Download
      </button>
    );
  else if (s.phase === "ready" || s.phase === "restarting") {
    const why = s.phase === "ready" && su.hold ? holdText(su.hold) : null;
    control = (
      <>
        <button
          type="button"
          className="btn primary small"
          disabled={s.phase === "restarting" || why !== null}
          aria-describedby={why ? whyId : undefined}
          onClick={su.restart}
        >
          Restart to update
        </button>
        {why && (
          <span id={whyId} className="why">
            {why}
          </span>
        )}
      </>
    );
  } else if (s.phase === "download_failed" || s.phase === "signature_rejected")
    control = (
      <button type="button" className="btn ghost small" onClick={su.download}>
        Try again
      </button>
    );
  else
    control = (
      <button
        type="button"
        className="btn ghost small"
        // While downloading, the hint says why there is nothing to check.
        disabled={checking || s.phase === "downloading"}
        aria-describedby={s.phase === "downloading" ? hintId : undefined}
        onClick={() => void check()}
      >
        Check for updates
      </button>
    );

  return (
    <Row label="Launcher version" hint={<span id={hintId}>{hint}</span>}>
      {control}
    </Row>
  );
}
