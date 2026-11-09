// Settings → Launcher: Launcher version and Download launcher updates automatically (spec: Settings, #28).
import { useId, useState } from "react";
import type { SelfUpdateStatus } from "./api";
import { Row } from "../settings/Row";
import { BusyButton } from "../ui/pending";
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
  const control = s ? versionControl(s) : null;

  // Controls that turn off while focused use aria-disabled, so focus stays on them.
  function versionControl(s: SelfUpdateStatus) {
    switch (s.phase) {
      case "available":
        return (
          <BusyButton className="ghost small" busy={su.requesting === "download"} blocked={su.requesting !== null} onClick={su.download}>
            Download
          </BusyButton>
        );
      case "ready":
      case "restarting":
      case "quitting": {
        const why = s.phase === "ready" && su.hold ? holdText(su.hold) : null;
        const off = s.phase !== "ready" || why !== null;
        return (
          <>
            <BusyButton
              className="primary small"
              busy={su.requesting === "restart"}
              blocked={off || su.requesting !== null}
              aria-describedby={why ? whyId : undefined}
              onClick={su.restart}
            >
              Restart to update
            </BusyButton>
            {why && (
              <span id={whyId} className="why">
                {why}
              </span>
            )}
          </>
        );
      }
      case "download_failed":
      case "signature_rejected":
        return (
          <BusyButton className="ghost small" busy={su.requesting === "download"} blocked={su.requesting !== null} onClick={su.download}>
            Try again
          </BusyButton>
        );
      case "up_to_date":
      case "updated":
      case "downloading": {
        // While downloading, the hint says why there is nothing to check.
        const off = checking || s.phase === "downloading";
        return (
          <button
            type="button"
            className="btn ghost small"
            aria-disabled={off || undefined}
            aria-describedby={s.phase === "downloading" ? hintId : undefined}
            onClick={() => !off && void check()}
          >
            Check for updates
          </button>
        );
      }
    }
  }

  return (
    <Row label="Launcher version" hint={<span id={hintId}>{hint}</span>}>
      {control}
    </Row>
  );
}
