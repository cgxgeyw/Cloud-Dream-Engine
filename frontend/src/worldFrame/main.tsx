import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { WorldFrameApp } from "./WorldFrameApp";
import "../index.css";
import "./worldFrame.css";

const rootElement = document.getElementById("world-frame-root");

if (!rootElement) {
  throw new Error("未找到世界界面根节点。");
}

createRoot(rootElement).render(
  <StrictMode>
    <WorldFrameApp />
  </StrictMode>,
);
