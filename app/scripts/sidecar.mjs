// Builds the termf CLI for the bundle: Tauri's externalBin wants it at src-tauri/binaries/termf-<target triple>.
// A universal macOS build gets both architectures joined by lipo.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

const app = join(import.meta.dirname, "..");
const triple =
  process.env.TAURI_ENV_TARGET_TRIPLE ??
  execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/host: (\S+)/)[1];
const profile = process.env.TAURI_ENV_DEBUG === "true" ? "debug" : "release";
const exe = triple.includes("windows") ? ".exe" : "";
const triples = triple === "universal-apple-darwin" ? ["aarch64-apple-darwin", "x86_64-apple-darwin"] : [triple];
const target = process.env.CARGO_TARGET_DIR ?? join(app, "..", "target");

for (const t of triples) {
  execFileSync("cargo", ["build", "-p", "termf-cli", "--target", t, ...(profile === "release" ? ["--release"] : [])], {
    stdio: "inherit",
    cwd: app,
  });
}
const built = triples.map((t) => join(target, t, profile, `termf${exe}`));
const out = join(app, "src-tauri", "binaries", `termf-${triple}${exe}`);
mkdirSync(join(app, "src-tauri", "binaries"), { recursive: true });
if (built.length > 1) execFileSync("lipo", ["-create", "-output", out, ...built], { stdio: "inherit" });
else copyFileSync(built[0], out);
