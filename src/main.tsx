import React from "react";
import ReactDOM from "react-dom/client";
import { DesktopApp } from "./app/DesktopApp";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <DesktopApp />
  </React.StrictMode>,
);
