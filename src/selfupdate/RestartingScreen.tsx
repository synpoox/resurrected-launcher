import { MARK_SRC } from "../art";
import { useSelfUpdate } from "./SelfUpdate";
import "./selfupdate.css";

/** Covers the main area while the launcher installs its update and restarts. */
export function RestartingScreen() {
  const su = useSelfUpdate();
  if (su?.status?.phase !== "restarting") return null;
  return (
    <div className="su-restarting" role="status">
      <img src={MARK_SRC} alt="" width={56} height={56} />
      <b>Updating the launcher…</b>
      <span>Back in a few seconds.</span>
    </div>
  );
}
