import { createHash } from "node:crypto";
import { readFileSync, existsSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const native = resolve(root, "src-tauri");
const config = JSON.parse(readFileSync(resolve(native, "tauri.conf.json"), "utf8"));
const cli = resolve(root, "node_modules/@tauri-apps/cli");
const version = JSON.parse(readFileSync(resolve(cli, "package.json"), "utf8")).version;
const inputs = createHash("sha256")
  .update(readFileSync(resolve(root, "assets/app-icon.png")))
  .update(version)
  .update(JSON.stringify(config.bundle.icon))
  .digest("hex");
const marker = resolve(native, "icons/.input-hash");
const outputsExist = () => config.bundle.icon.every((icon) => existsSync(resolve(native, icon)));

if (existsSync(marker) && readFileSync(marker, "utf8") === inputs && outputsExist()) {
  console.log("Native app icons are up to date.");
} else {
  const result = spawnSync(process.execPath, [resolve(cli, "tauri.js"), "icon", "assets/app-icon.png"], {
    cwd: root,
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
  if (!outputsExist()) throw new Error("Icon generation did not create every configured icon.");
  writeFileSync(marker, inputs);
}
