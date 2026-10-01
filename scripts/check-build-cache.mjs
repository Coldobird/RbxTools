import assert from "node:assert/strict";
import { test } from "node:test";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

function fixture(t, script) {
  const temp = realpathSync(tmpdir());
  const root = mkdtempSync(join(temp, "rbx-build-cache-"));
  t.after(() => {
    assert.equal(dirname(realpathSync(root)), temp);
    rmSync(root, { recursive: true, force: true });
  });
  const write = (path, value) => {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), value);
  };
  write("scripts/placeholder", "");
  copyFileSync(join(dirname(fileURLToPath(import.meta.url)), script), join(root, "scripts", script));
  write("src/app.ts", "first");
  write("assets/app-icon.png", "image");
  for (const path of ["package.json", "package-lock.json", "tsconfig.json", "vite.config.ts", "index.html"]) write(path, "{}");
  write("src-tauri/tauri.conf.json", JSON.stringify({ bundle: { icon: ["icons/icon.ico"] } }));
  const run = (env = {}, args = []) => spawnSync(process.execPath, [resolve(root, "scripts", script), ...args], {
    cwd: root, env: { ...process.env, ...env }, encoding: "utf8",
  });
  const count = () => readFileSync(join(root, "runs"), "utf8").length;
  return { root, write, run, count };
}

const recordRun = `const fs = require('node:fs'); fs.appendFileSync('runs', 'x');`;

test("frontend cache preserves outputs and invalidates changed inputs, environment, and output contents", (t) => {
  const { root, write, run, count } = fixture(t, "build-frontend.mjs");
  write("node_modules/typescript/bin/tsc", recordRun + `if (process.env.RBX_FAIL) process.exit(17);`);
  write("node_modules/vite/bin/vite.js", recordRun + `fs.mkdirSync('dist', {recursive:true}); fs.writeFileSync('dist/index.html', fs.readFileSync('src/app.ts'));`);
  const success = (env, args) => assert.equal(run(env, args).status, 0);
  success();
  const output = join(root, "dist/index.html");
  const timestamp = statSync(output).mtimeMs;
  success();
  assert.equal(count(), 2);
  assert.equal(statSync(output).mtimeMs, timestamp);
  write("src/app.ts", "changed");
  success();
  assert.equal(readFileSync(output, "utf8"), "changed");
  write("dist/index.html", "corrupted");
  success();
  assert.equal(readFileSync(output, "utf8"), "changed");
  success({ VITE_CACHE_PROBE: "changed" });
  assert.equal(count(), 8);
  success({ VITE_CACHE_PROBE: "changed" }, ["--force"]);
  assert.equal(count(), 10);
  write("assets/app-icon.png", "changed image");
  assert.equal(run({ VITE_CACHE_PROBE: "changed", RBX_FAIL: "1" }).status, 17);
  assert.equal(count(), 11);
  success({ VITE_CACHE_PROBE: "changed" });
  assert.equal(count(), 13);
  rmSync(output);
  success({ VITE_CACHE_PROBE: "changed" });
  assert.equal(count(), 15);
  assert.ok(existsSync(output));
});

test("icon cache detects missing outputs, artwork changes, and CLI upgrades", (t) => {
  const { root, write, run, count } = fixture(t, "ensure-icons.mjs");
  write("node_modules/@tauri-apps/cli/package.json", JSON.stringify({ version: "1" }));
  write("node_modules/@tauri-apps/cli/tauri.js", recordRun + `fs.mkdirSync('src-tauri/icons', {recursive:true}); fs.writeFileSync('src-tauri/icons/icon.ico', 'icon');`);
  assert.equal(run().status, 0);
  const output = join(root, "src-tauri/icons/icon.ico");
  const timestamp = statSync(output).mtimeMs;
  assert.equal(run().status, 0);
  assert.equal(count(), 1);
  assert.equal(statSync(output).mtimeMs, timestamp);
  rmSync(output);
  assert.equal(run().status, 0);
  write("assets/app-icon.png", "new image");
  assert.equal(run().status, 0);
  write("node_modules/@tauri-apps/cli/package.json", JSON.stringify({ version: "2" }));
  assert.equal(run().status, 0);
  assert.equal(count(), 4);
});
