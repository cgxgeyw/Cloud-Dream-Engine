const http = require("node:http");
const fs = require("node:fs");
const path = require("node:path");
const { spawn } = require("node:child_process");
const { readDevConfig } = require("./dev-config");

const rootDir = path.resolve(__dirname, "..");
const tauriDevOverridePath = path.resolve(rootDir, "src-tauri", "tauri.dev.override.json");
const { host, frontendPort } = readDevConfig();
const frontendUrl = `http://${host}:${frontendPort}`;

function quoteWindowsArg(value) {
  const text = String(value);
  return /[\s"&^|<>]/.test(text) ? `"${text.replace(/"/g, '\\"')}"` : text;
}

function spawnCommand(command, args, options) {
  if (process.platform === "win32") {
    const commandLine = [command, ...args].map(quoteWindowsArg).join(" ");
    return spawn("cmd.exe", ["/d", "/s", "/c", commandLine], options);
  }
  return spawn(command, args, options);
}

function waitForHttpReady(url, timeoutMs = 30000) {
  const startedAt = Date.now();

  return new Promise((resolve, reject) => {
    const tryConnect = () => {
      const request = http.get(url, (response) => {
        response.resume();
        resolve();
      });

      request.on("error", () => {
        if (Date.now() - startedAt >= timeoutMs) {
          reject(new Error(`Timed out waiting for dev server at ${url}`));
          return;
        }
        setTimeout(tryConnect, 300);
      });
    };

    tryConnect();
  });
}

async function isHttpReady(url) {
  try {
    await waitForHttpReady(url, 800);
    return true;
  } catch {
    return false;
  }
}

function terminateChild(child) {
  if (!child || child.killed) {
    return;
  }
  child.kill("SIGTERM");
}

function writeTauriDevOverride() {
  fs.writeFileSync(
    tauriDevOverridePath,
    `${JSON.stringify(
      {
        build: {
          beforeDevCommand: null,
          devUrl: frontendUrl,
        },
      },
      null,
      2,
    )}\n`,
    "utf8",
  );
}

function removeTauriDevOverride() {
  try {
    fs.unlinkSync(tauriDevOverridePath);
  } catch {}
}

async function main() {
  let frontendDev = null;
  const frontendAlreadyRunning = await isHttpReady(frontendUrl);

  if (!frontendAlreadyRunning) {
    frontendDev = spawnCommand(
      "npm",
      [
        "--prefix",
        "frontend",
        "run",
        "dev",
        "--",
        "--host",
        host,
        "--port",
        String(frontendPort),
        "--strictPort",
      ],
      {
        cwd: rootDir,
        stdio: "inherit",
        env: process.env,
      },
    );

    frontendDev.on("exit", (code) => {
      if (code && code !== 0) {
        process.exit(code);
      }
    });

    try {
      await waitForHttpReady(frontendUrl);
    } catch (error) {
      terminateChild(frontendDev);
      throw error;
    }
  }

  writeTauriDevOverride();

  const tauriDev = spawnCommand("tauri", ["dev", "--config", tauriDevOverridePath], {
    cwd: rootDir,
    stdio: "inherit",
    env: process.env,
  });

  const shutdown = () => {
    terminateChild(tauriDev);
    terminateChild(frontendDev);
    removeTauriDevOverride();
  };

  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);

  tauriDev.on("exit", (code, signal) => {
    terminateChild(frontendDev);
    removeTauriDevOverride();
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 0);
  });
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
