// The live LauncherApi: every member passes straight through to `src/ipc.ts`, the only module that
// imports `@tauri-apps/api`. News has no native commands until #80, so it comes from `news`.
import * as ipc from "../ipc";
import type { LauncherApi } from "./types";

export type NewsApi = Pick<LauncherApi, "getNews" | "openNewsLink" | "openProductPage">;

export function createLiveApi(news: NewsApi) {
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
    toggleMaximizeWindow: ipc.toggleMaximizeWindow,
    closeWindow: ipc.closeWindow,
    startWindowDrag: ipc.startWindowDrag,
    quitLauncher: ipc.quitLauncher,

    getNews: (productId: string) => news.getNews(productId),
    openNewsLink: (productId: string, index: number) => news.openNewsLink(productId, index),
    openProductPage: (productId: string) => news.openProductPage(productId),

    onStateChanged: ipc.onStateChanged,
    onProgress: ipc.onProgress,
    onOutcome: ipc.onOutcome,
    onTrayRequest: ipc.onTrayRequest,
  } satisfies LauncherApi;
}
