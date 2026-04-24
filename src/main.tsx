import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initAppearance } from "./lib/appearance";
import { initAccent } from "./lib/accent";
import "./styles/tokens.css";
import "./index.css";

initAppearance();
void initAccent();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
