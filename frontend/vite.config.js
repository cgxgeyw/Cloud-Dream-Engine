import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import fs from "node:fs";
import path from "node:path";
const devConfigPath = path.resolve(__dirname, "..", "dev.config.json");
function readDevConfig() {
    const defaults = {
        host: "127.0.0.1",
        frontendPort: 8850,
        backendPort: 8010,
    };
    if (!fs.existsSync(devConfigPath)) {
        return defaults;
    }
    const parsed = JSON.parse(fs.readFileSync(devConfigPath, "utf8"));
    const host = typeof parsed.host === "string" && parsed.host.trim() ? parsed.host.trim() : defaults.host;
    const frontendPort = Number.isInteger(parsed.frontendPort) ? Number(parsed.frontendPort) : defaults.frontendPort;
    const backendPort = Number.isInteger(parsed.backendPort) ? Number(parsed.backendPort) : defaults.backendPort;
    return { host, frontendPort, backendPort };
}
const devConfig = readDevConfig();
const backendTarget = `http://${devConfig.host}:${devConfig.backendPort}`;
export default defineConfig({
    base: "./",
    plugins: [react(), tailwindcss()],
    build: {
        chunkSizeWarningLimit: 1700,
    },
    server: {
        host: devConfig.host,
        port: devConfig.frontendPort,
        strictPort: true,
        proxy: {
            "/api": {
                target: backendTarget,
                changeOrigin: true,
            },
            "/assets": {
                target: backendTarget,
                changeOrigin: true,
            },
        },
    },
});
