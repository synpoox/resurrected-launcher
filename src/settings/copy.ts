// Settings copy that depends on product state, kept pure so it can be tested on its own.
import type { Folder, GameLocation, LauncherError, ProductState } from "../api/types";
import type { Product } from "../products";

export const STORE_NAME: Record<GameLocation["store"], string> = {
  battle_net: "Battle.net",
  unknown: "Store not recognised",
};

export const GAME_RUNNING_REASON = "Quit the game to change this";
export const STATE_FAILED_REASON = "Couldn't read the launcher's state. Reopen Settings to try again";

/** What the recheck after a location change found, for the Game panel's notice. */
export function recheckText(state: ProductState, product: Product): string {
  const version = state.installed ? `alpha ${state.installed.display_version}` : "";
  const r = state.readiness;
  switch (r.status) {
    case "ready":
      return `${product.short} ${version} was found there and is ready to play.`;
    case "update_available":
      return `${product.short} ${version} was found there, and an update is available.`;
    case "not_installed":
      return `${product.short} isn't installed at this location, so its page now offers Install.`;
    case "build_unsupported":
      return `D2R build ${r.found} isn't supported by this ${product.short} release. ${product.short}'s page explains what to do.`;
    case "loader_missing":
    case "loader_unsupported":
      return `D2RLoader isn't ready at this location. ${product.short}'s page explains what to do.`;
    case "d2r_missing":
      return "D2R still wasn't found there.";
    default:
      return `${product.short}'s page shows what to do next.`;
  }
}

/** The folder rows' names, used in toasts. */
export function folderName(folder: Folder, product: Product | null): string {
  switch (folder) {
    case "saves":
      return `${product?.short} saves folder`;
    case "settings":
      return `${product?.short} settings and filters folder`;
    case "product_plugins":
      return `${product?.short} plugins folder`;
    case "global_plugins":
      return "global plugins folder";
  }
}

/** The red toast after openFolder fails. It never mentions administrator rights. */
export function folderFailureText(error: LauncherError | null, name: string, product: Product | null): string {
  if (error?.code === "product_not_installed") {
    return `Couldn't open the ${name}: ${product?.short ?? "the product"} isn't installed at this D2R location.`;
  }
  if (error?.code === "access_denied") {
    return `Couldn't open the ${name}: access was denied. Create the folder yourself in File Explorer, or check the D2R location in Settings → Game.`;
  }
  return `Couldn't open the ${name}. Check the D2R location in Settings → Game, then try again.`;
}
