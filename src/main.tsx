import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { installBrowserGuard } from "./chrome/browserGuard";
import { selfUpdateEnabled } from "./selfupdate/flag";
import "./tokens.css";
import "./styles.css";

// Dev keeps the browser menu and devtools (#185).
if (!import.meta.env.DEV) installBrowserGuard();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App search={location.search} dev={import.meta.env.DEV} selfUpdate={selfUpdateEnabled()} />
  </StrictMode>,
);
