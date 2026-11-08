import { useSelfUpdate } from "./SelfUpdate";
import "./selfupdate.css";

/** The red dot on the Settings sidebar item, after Later on a launcher-update toast. */
export function SettingsDot() {
  const su = useSelfUpdate();
  if (!su?.dot) return null;
  return <span className="su-dot" role="img" aria-label="launcher update" />;
}
