// The live LauncherApi: every member passes straight through to `src/ipc.ts`, the only module that
// imports `@tauri-apps/api`.
import * as ipc from "../ipc";
import type { LauncherApi } from "./types";

export function createLiveApi() {
  return {
    getProductState: ipc.getProductState,
    inspectOperation: ipc.inspectOperation,
    startOperation: ipc.startOperation,
    cancelOperation: ipc.cancelOperation,
    recoverProduct: ipc.recoverProduct,
    playProduct: ipc.playProduct,
    refreshCatalog: ipc.refreshCatalog,

    detectGameLocations: ipc.detectGameLocations,
    selectGameLocation: ipc.selectGameLocation,
    chooseGameLocation: ipc.chooseGameLocation,
    openLoaderDownloadPage: ipc.openLoaderDownloadPage,

    getReleaseNotes: ipc.getReleaseNotes,
    openFolder: ipc.openFolder,
    previewDiagnostics: ipc.previewDiagnostics,
    saveDiagnostics: ipc.saveDiagnostics,
    showDiagnosticsInFolder: ipc.showDiagnosticsInFolder,
    getStartWithWindows: ipc.getStartWithWindows,
    setStartWithWindows: ipc.setStartWithWindows,

    minimizeWindow: ipc.minimizeWindow,
    closeWindow: ipc.closeWindow,
    startWindowDrag: ipc.startWindowDrag,
    quitLauncher: ipc.quitLauncher,

    getNews: ipc.getNews,
    openNewsLink: ipc.openNewsLink,
    openProductPage: ipc.openProductPage,
    openCommunityPage: ipc.openCommunityPage,

    onStateChanged: ipc.onStateChanged,
    onProgress: ipc.onProgress,
    onOutcome: ipc.onOutcome,
    onTrayRequest: ipc.onTrayRequest,
  } satisfies LauncherApi;
}
