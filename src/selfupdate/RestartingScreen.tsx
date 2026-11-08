import { MARK_SRC } from "../art";
import { useSelfUpdate } from "./SelfUpdate";
import "./selfupdate.css";

/**
 * Covers the main area while the launcher installs its update: Restart to update comes back in a few
 * seconds; Quit closes the launcher and doesn't relaunch it.
 */
export function RestartingScreen() {
  const su = useSelfUpdate();
  const phase = su?.status?.phase;
  if (phase !== "restarting" && phase !== "quitting") return null;
  return (
    <div className="su-restarting" role="status">
      <img src={MARK_SRC} alt="" width={56} height={56} />
      <b>Updating the launcher…</b>
      <span>{phase === "restarting" ? "Back in a few seconds." : "The launcher closes now. Start it again whenever you like."}</span>
    </div>
  );
}
