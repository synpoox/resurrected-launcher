import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { selfUpdateEnabled } from "./selfupdate/flag";
import "./tokens.css";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App search={location.search} dev={import.meta.env.DEV} selfUpdate={selfUpdateEnabled()} />
  </StrictMode>,
);
