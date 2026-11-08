import { createContext, useContext } from "react";
import type { LauncherApi } from "./types";

export const ApiContext = createContext<LauncherApi | null>(null);

/** The LauncherApi the app was mounted with. */
export function useApi(): LauncherApi {
  const api = useContext(ApiContext);
  if (!api) throw new Error("useApi outside ApiContext");
  return api;
}
