import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles/tokens.css";
import "./styles/app.css";
import "./styles/quick.css";
import { QuickLauncher } from "./components/QuickLauncher";

// Fenêtre de la recherche rapide (`quick.rs`), ouverte par le raccourci global.
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QuickLauncher />
  </React.StrictMode>,
);
