const http = require("node:http");
const fs = require("node:fs");
const path = require("node:path");
const { spawn, execFileSync } = require("node:child_process");
const { readDevConfig } = require("./dev-config");

const rootDir = path.resolve(__dirname, "..");
const tauriDevOverridePath = path.resolve(rootDir, "src-tauri", "tauri.dev.override.json");
const { host, frontendPort } = readDevConfig();
const frontendUrl = `http://${host}:${frontendPort}`;

/**
 * 在普通 PowerShell 里 `npm run tauri:dev` 默认拿不到 VS 的 link.exe / Windows SDK
 * 的 rc.exe，Tauri 的 winres 构建会直接 panic。这里自动从 vswhere 找 Build Tools，
 * 把 MSVC 和 SDK 的 bin 目录补进当前 Node 进程，再传给 tauri 子进程。
 */
function ensureWindowsToolchain() {
  if (process.platform !== "win32") {
    return;
  }
  const vswhere = "C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe";
  if (!fs.existsSync(vswhere)) {
    return;
  }

  let installPath = "";
  try {
    installPath = execFileSync(
      vswhere,
      [
        "-latest",
        "-products",
        "*",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-property",
        "installationPath",
      ],
      { encoding: "utf8" },
    ).trim();
  } catch {
    return;
  }
  if (!installPath) {
    return;
  }

  const msvcRoot = path.join(installPath, "VC", "Tools", "MSVC");
  let msvcBin = "";
  try {
    const versions = fs
      .readdirSync(msvcRoot)
      .filter((version) =>
        fs.existsSync(path.join(msvcRoot, version, "bin", "Hostx64", "x64", "link.exe")),
      )
      .sort()
      .reverse();
    if (versions.length > 0) {
      msvcBin = path.join(msvcRoot, versions[0], "bin", "Hostx64", "x64");
    }
  } catch {
    return;
  }

  const sdkBinRoot = "C:\\Program Files (x86)\\Windows Kits\\10\\bin";
  let sdkBin = "";
  try {
    const versions = fs
      .readdirSync(sdkBinRoot)
      .filter((version) => fs.existsSync(path.join(sdkBinRoot, version, "x64", "rc.exe")))
      .sort()
      .reverse();
    if (versions.length > 0) {
      sdkBin = path.join(sdkBinRoot, versions[0], "x64");
    }
  } catch {
    return;
  }

  const additions = [msvcBin, sdkBin].filter(Boolean);
  if (additions.length > 0) {
    process.env.PATH = `${additions.join(";")};${process.env.PATH || ""}`;
  }
  if (sdkBin) {
    process.env.RC = path.join(sdkBin, "rc.exe");
  }
}

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
  ensureWindowsToolchain();
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
