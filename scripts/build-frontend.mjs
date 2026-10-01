import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const marker = resolve(root, "node_modules/.cache/rbx-tools/frontend-build.json");

function files(path) {
  if (!existsSync(path)) return [];
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    const child = resolve(path, entry.name);
    return entry.isDirectory() ? files(child) : entry.isFile() ? [child] : [];
  });
}

function fingerprint(paths) {
  const hash = createHash("sha256");
  for (const path of paths.sort()) {
    hash.update(relative(root, path)).update("\0");
    hash.update(readFileSync(path)).update("\0");
  }
  return hash.digest("hex");
}

// Include imported assets/config, dependency versions, build tools, and Vite's
// production environment. Hash contents so staging timestamps do not matter.
const inputs = fingerprint([
  ...["src", "assets", "public"].flatMap((path) => files(resolve(root, path))),
  ...["package.json", "package-lock.json", "tsconfig.json", "vite.config.ts", "index.html",
    "src-tauri/tauri.conf.json", "scripts/build-frontend.mjs",
    ".env", ".env.local", ".env.production", ".env.production.local"]
    .map((path) => resolve(root, path)).filter(existsSync),
]);
const environment = JSON.stringify([
  process.version, process.platform, process.arch,
  Object.entries(process.env)
    .filter(([key]) => key.startsWith("VITE_") || key.startsWith("TAURI_ENV_") || key === "NODE_ENV")
    .sort(([left], [right]) => left.localeCompare(right)),
]);
let previous;
try { previous = JSON.parse(readFileSync(marker, "utf8")); } catch { /* First or interrupted build. */ }

if (!process.argv.includes("--force") && previous?.inputs === inputs && previous.environment === environment
  && existsSync(resolve(root, "dist/index.html"))
  && previous.outputs === fingerprint(files(resolve(root, "dist")))) {
  console.log("Frontend is up to date; preserving output files for Cargo.");
} else {
  for (const [command, args] of [
    ["node_modules/typescript/bin/tsc", ["-b"]],
    ["node_modules/vite/bin/vite.js", ["build"]],
  ]) {
    const result = spawnSync(process.execPath, [resolve(root, command), ...args], { cwd: root, stdio: "inherit" });
    if (result.error) throw result.error;
    if (result.status !== 0) process.exit(result.status ?? 1);
  }
  mkdirSync(dirname(marker), { recursive: true });
  writeFileSync(marker, JSON.stringify({ inputs, environment, outputs: fingerprint(files(resolve(root, "dist"))) }));
}
