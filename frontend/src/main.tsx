import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { configureDesktopRuntime } from "./data/desktopRuntime";
import { applyMode, applyStyle, resolveInitialMode, resolveInitialStyle } from "./data/theme";
import { applyLanguage, resolveInitialLanguage } from "./data/language";
import "./index.css";

const rootElement = document.getElementById("root");

if (!rootElement) {
  throw new Error("Root element #root was not found.");
}

applyMode(resolveInitialMode());
applyStyle(resolveInitialStyle());
applyLanguage(resolveInitialLanguage());

configureDesktopRuntime()
  .catch((err) => {
    console.error("API 适配层初始化失败:", err);
  })
  .finally(() => {
    createRoot(rootElement).render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
  });
