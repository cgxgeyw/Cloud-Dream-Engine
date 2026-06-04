const fs = require("node:fs");
const path = require("node:path");

const devConfigPath = path.resolve(__dirname, "..", "dev.config.json");

const DEFAULT_DEV_CONFIG = Object.freeze({
  host: "127.0.0.1",
  frontendPort: 8850,
  backendPort: 8010,
});

function normalizePort(value, fallback) {
  const port = Number.parseInt(String(value ?? ""), 10);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    return fallback;
  }
  return port;
}

function readDevConfig() {
  let parsed = {};

  if (fs.existsSync(devConfigPath)) {
    parsed = JSON.parse(fs.readFileSync(devConfigPath, "utf8"));
  }

  const host =
    typeof parsed.host === "string" && parsed.host.trim()
      ? parsed.host.trim()
      : DEFAULT_DEV_CONFIG.host;

  return {
    host,
    frontendPort: normalizePort(parsed.frontendPort, DEFAULT_DEV_CONFIG.frontendPort),
    backendPort: normalizePort(parsed.backendPort, DEFAULT_DEV_CONFIG.backendPort),
  };
}

module.exports = {
  DEFAULT_DEV_CONFIG,
  devConfigPath,
  readDevConfig,
};
