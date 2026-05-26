import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const siteRoot = path.resolve(scriptDir, "..");
const repoRoot = path.resolve(siteRoot, "..");
const carWasmRoot = path.resolve(repoRoot, "car-wasm");

const result = spawnSync(
  "wasm-pack",
  ["build", "--target", "bundler", "--out-dir", "pkg", "--no-opt"],
  {
    cwd: carWasmRoot,
    stdio: "inherit",
    env: process.env,
  },
);

if (result.error) {
  if (result.error.code === "ENOENT") {
    console.error("wasm-pack was not found in PATH; install wasm-pack first.");
  } else {
    console.error(result.error.message);
  }
  process.exit(1);
}

process.exit(result.status ?? 1);
