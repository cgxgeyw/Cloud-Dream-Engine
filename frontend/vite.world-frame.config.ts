import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { createWorldFrameDocument } from "./src/worldFrame/frameDocument";

export default defineConfig({
  define: {
    "process.env.NODE_ENV": JSON.stringify("production"),
  },
  plugins: [
    react(),
    {
      name: "emit-world-frame-document",
      generateBundle() {
        this.emitFile({
          type: "asset",
          fileName: "frame.html",
          source: createWorldFrameDocument(),
        });
        this.emitFile({
          type: "asset",
          fileName: "frame.dev.html",
          source: createWorldFrameDocument(
            "./runtime.iife.js",
            "./style.css",
            true,
            "'self' http://127.0.0.1:* http://localhost:*",
          ),
        });
      },
    },
  ],
  publicDir: false,
  build: {
    outDir: path.resolve(__dirname, "public", "world-frame"),
    emptyOutDir: true,
    cssCodeSplit: false,
    // 沙箱 iframe 的 CSP 是 font-src data: blob:（见 frameDocument.ts），
    // KaTeX 等 CSS 引用的字体必须内联成 base64，单独产物文件会被 CSP 拦截。
    assetsInlineLimit: 100_000_000,
    lib: {
      entry: path.resolve(__dirname, "src", "worldFrame", "main.tsx"),
      name: "CloudDreamWorldFrame",
      formats: ["iife"],
      fileName: "runtime",
      cssFileName: "style",
    },
  },
});
