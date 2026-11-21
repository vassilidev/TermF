// Builds the termf CLI for the bundle: Tauri's externalBin wants it at src-tauri/binaries/termf-<target triple>.
// shortcut: no universal-apple-darwin (it needs both arches and lipo), add it with the release CI.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

const app = join(import.meta.dirname, "..");
const triple =
  process.env.TAURI_ENV_TARGET_TRIPLE ??
  execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/host: (\S+)/)[1];
const profile = process.env.TAURI_ENV_DEBUG === "true" ? "debug" : "release";
const exe = triple.includes("windows") ? ".exe" : "";

execFileSync(
  "cargo",
  ["build", "-p", "termf-cli", "--target", triple, ...(profile === "release" ? ["--release"] : [])],
  { stdio: "inherit", cwd: app },
);
const target = process.env.CARGO_TARGET_DIR ?? join(app, "..", "target");
mkdirSync(join(app, "src-tauri", "binaries"), { recursive: true });
copyFileSync(
  join(target, triple, profile, `termf${exe}`),
  join(app, "src-tauri", "binaries", `termf-${triple}${exe}`),
);
