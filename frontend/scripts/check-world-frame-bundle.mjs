import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const frontendRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outputRoot = path.join(frontendRoot, "public", "world-frame");
const runtimePath = path.join(outputRoot, "runtime.iife.js");
const requiredFiles = ["frame.html", "frame.dev.html", "style.css", "runtime.iife.js"];

for (const filename of requiredFiles) {
  if (!fs.existsSync(path.join(outputRoot, filename))) {
    throw new Error(`World frame build is missing ${filename}.`);
  }
}

const runtimeSource = fs.readFileSync(runtimePath, "utf8");
if (/\bprocess\.env\b/.test(runtimeSource)) {
  throw new Error("World frame bundle contains an unresolved process.env reference.");
}
if (!runtimeSource.includes("world-frame/boot")) {
  throw new Error("World frame bundle does not contain the boot protocol.");
}

console.log("World frame bundle checks passed.");
