import React from "react";
import ReactDOM from "react-dom/client";
import { DesktopApp } from "./app/DesktopApp";

if (import.meta.env.VITE_E2E === "true") {
  if (!window.sessionStorage.getItem("everyfile-e2e-started")) {
    window.localStorage.clear();
    window.sessionStorage.setItem("everyfile-e2e-started", "true");
  }
  void import("@wdio/tauri-plugin");
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <DesktopApp />
  </React.StrictMode>,
);
