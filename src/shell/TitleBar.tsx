import { useApi } from "../api/context";

/** Seamless title bar: a drag region plus minimise and close floating top-right. There is no
 *  maximise: the window has a fixed maximum size, like the Riot Client (issue 135). */
export function TitleBar() {
  const api = useApi();
  return (
    <header className="titlebar">
      <div className="window-controls">
        <button type="button" className="win" aria-label="Minimise" onClick={() => void api.minimizeWindow()}>
          <span aria-hidden="true">–</span>
        </button>
        <button type="button" className="win close" aria-label="Close" onClick={() => void api.closeWindow()}>
          <span aria-hidden="true">✕</span>
        </button>
      </div>
      <div
        className="drag"
        data-testid="drag-region"
        onMouseDown={(e) => {
          if (e.button === 0 && e.detail === 1) void api.startWindowDrag();
        }}
      />
    </header>
  );
}
