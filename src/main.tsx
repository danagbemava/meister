import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import SettingsWindow from "./components/SettingsWindow";
import { initAppearance } from "./lib/appearance";
import { initAccent } from "./lib/accent";
import { initReduceTransparency } from "./lib/reduceTransparency";
import "./styles/tokens.css";
import "./index.css";

initAppearance();
void initAccent();
void initReduceTransparency();

const label = getCurrentWindow().label;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {label === "settings" ? <SettingsWindow /> : <App />}
  </React.StrictMode>,
);
