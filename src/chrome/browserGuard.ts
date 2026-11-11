// Release builds hide the WebView's browser behaviour (#185). The WebView2 settings
// in lifecycle/mod.rs turn off reload, print, find, save, view-source and zoom; this
// covers what WebView2 has no setting for without losing text editing.

/** Whether the default context menu is useful here: an editable field or selected text. */
export function wantsContextMenu(target: EventTarget | null, selection: string): boolean {
  if (selection.length > 0) return true;
  if (!(target instanceof Element)) return false;
  return target.closest("input, textarea, [contenteditable]:not([contenteditable='false'])") !== null;
}

/** Blocks the browser context menu and image/link drags. Returns a function that undoes it. */
export function installBrowserGuard(doc: Document = document): () => void {
  const onContextMenu = (e: MouseEvent) => {
    if (!wantsContextMenu(e.target, doc.getSelection()?.toString() ?? "")) e.preventDefault();
  };
  // Dragging an image or link out of the window would hand the URL to the shell.
  const onDragStart = (e: DragEvent) => {
    if (e.target instanceof Element && e.target.closest("img, a")) e.preventDefault();
  };
  doc.addEventListener("contextmenu", onContextMenu);
  doc.addEventListener("dragstart", onDragStart);
  return () => {
    doc.removeEventListener("contextmenu", onContextMenu);
    doc.removeEventListener("dragstart", onDragStart);
  };
}
