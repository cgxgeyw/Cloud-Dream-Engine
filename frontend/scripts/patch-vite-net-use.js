#!/usr/bin/env node
/**
 * Patch Vite's Windows realpath optimization so it doesn't execute `net use`,
 * which fails in our restricted environment (Node cannot spawn external
 * processes such as `net.exe` or `cmd.exe`). We wrap the `exec` call with a
 * try/catch and fall back to the native `realpath` implementation.
 */
const fs = require("node:fs");
const path = require("node:path");

const viteChunkPath = path.join(__dirname, "..", "node_modules", "vite", "dist", "node", "chunks", "node.js");

if (!fs.existsSync(viteChunkPath)) {
  console.warn(`[patch-vite-net-use] Skipping because ${viteChunkPath} does not exist yet.`);
  process.exit(0);
}

const fileContents = fs.readFileSync(viteChunkPath, "utf8");
const patchedMarker = '\ttry {\n\texec("net use", (error, stdout) => {';

if (fileContents.includes(patchedMarker)) {
  console.log("[patch-vite-net-use] Patch already applied.");
  process.exit(0);
}

const netUseSnippet =
  '\texec("net use", (error, stdout) => {\n' +
  '\t\tif (error) return;\n' +
  '\t\tconst lines = stdout.split("\\n");\n' +
  "\t\tfor (const line of lines) {\n" +
  "\t\t\tconst m = parseNetUseRE.exec(line);\n" +
  "\t\t\tif (m) windowsNetworkMap.set(m[2], m[1]);\n" +
  "\t\t}\n" +
  "\t\tif (windowsNetworkMap.size === 0) safeRealpathSync = fs.realpathSync.native;\n" +
  "\t\telse safeRealpathSync = windowsMappedRealpathSync;\n" +
  "\t});";

if (!fileContents.includes(netUseSnippet)) {
  console.error("[patch-vite-net-use] Failed to locate the Vite net use snippet. Patch aborted.");
  process.exit(1);
}

const replacement =
  "\ttry {\n" +
  netUseSnippet +
  "\n" +
  "\t} catch (error) {\n" +
  "\t\tsafeRealpathSync = fs.realpathSync.native;\n" +
  "\t}";

const updatedContents = fileContents.replace(netUseSnippet, replacement);
fs.writeFileSync(viteChunkPath, updatedContents, "utf8");
console.log("[patch-vite-net-use] Applied Vite net use patch.");
