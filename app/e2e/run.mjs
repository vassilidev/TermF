// `pnpm e2e [part of a scenario's name | perf]`: drives the real app (debug build) through tauri-driver on Linux,
// through the app's own bridge on macOS. Each scenario gets its own sandbox and app; a failed one runs once more,
// with the reason logged.
import { appendFileSync, copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { APP, helpers, viewIds } from "./harness.mjs";
import { measure, report } from "./perf.mjs";
import { assertSafe, createSandbox } from "./sandbox.mjs";
import { scenarios } from "./scenarios.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const outDir = join(here, "out");
const platform = process.platform === "win32" ? "windows" : process.platform === "darwin" ? "macos" : process.platform;
const filter = process.argv[2];
const log = (...a) => console.log(...a);

assertSafe();
if (!existsSync(APP)) {
  console.error(`No app at ${APP}: build it first (cd app && pnpm e2e:build).`);
  process.exit(1);
}

const missing = viewIds.filter((id) => !scenarios.some((s) => s.views.includes(id)));
if (missing.length) {
  console.error(`Views of views.ts with no scenario in e2e/scenarios.mjs: ${missing.join(", ")}`);
  process.exit(1);
}

// E2E_PORT: another base, so that two runs on one machine (two worktrees) never take each other's ports.
let nextPort = Number(process.env.E2E_PORT) || 4444;
const port = () => ((nextPort += 2), nextPort);
const slug = (name) => name.replace(/[^a-z0-9]+/gi, "-").replace(/^-|-$/g, "").toLowerCase().slice(0, 60);

async function artifacts(t, name, error) {
  const dir = join(outDir, slug(name));
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "error.txt"), String(error?.stack ?? error));
  for (const file of ["app.log", "calls.log"]) {
    if (existsSync(join(t.box.root, file))) copyFileSync(join(t.box.root, file), join(dir, file));
  }
  if (!t.app) return;
  // Without the scenario's signal, which is aborted when it ran out of time.
  const s = t.s.detached();
  const steps = [
    async () => writeFileSync(join(dir, "screenshot.png"), await s.screenshot()),
    async () => writeFileSync(join(dir, "page.html"), await s.source()),
    async () => writeFileSync(join(dir, "text.txt"), (await s.exec(() => document.body.innerText)) ?? ""),
    async () => writeFileSync(join(dir, "errors.json"), JSON.stringify(await s.exec(() => window.__e2e), null, 2)),
  ];
  for (const step of steps) await step().catch(() => {});
}

async function attempt(scenario) {
  const box = createSandbox(scenario.sandbox);
  const abort = new AbortController();
  const t = helpers(box, port, abort.signal);
  // Past its time, the scenario fails at its next command or wait; its app and sandbox go only once it has stopped.
  const timer = setTimeout(() => abort.abort(new Error("the scenario took over 3 minutes")), 180_000);
  try {
    await t.start(scenario.launch).catch((e) => {
      e.launch = true;
      e.log = existsSync(join(box.root, "app.log")) ? readFileSync(join(box.root, "app.log"), "utf8").split("\n").slice(-25).join("\n") : "";
      throw e;
    });
    await scenario.run(t);
    return null;
  } catch (e) {
    const error = abort.signal.aborted ? abort.signal.reason : e;
    await artifacts(t, scenario.name, error);
    return error;
  } finally {
    clearTimeout(timer);
    if (t.app) {
      const synthetic = await t.s.detached().exec(() => window.__e2e?.synthetic).catch(() => 0);
      if (synthetic) log(`  (${synthetic} input${synthetic > 1 ? "s" : ""} sent by script after WebKit dropped them)`);
    }
    await t.stop().catch(() => {});
    box.cleanup();
  }
}

rmSync(outDir, { recursive: true, force: true });
const chosen = scenarios.filter((s) => (!s.only || [s.only].flat().includes(platform)) && (!filter || s.name.includes(filter)));
if (filter && filter !== "perf" && !chosen.length) {
  console.error(`No scenario matches "${filter}" on ${platform}.`);
  process.exit(1);
}
const results = [];
let unusable = false;
if (filter !== "perf") {
  for (const scenario of chosen) {
    const started = Date.now();
    let error = await attempt(scenario);
    let retried = false;
    if (error) {
      log(`↻ retry "${scenario.name}": ${String(error.message).split("\n")[0]}`);
      retried = true;
      error = await attempt(scenario);
    }
    const seconds = ((Date.now() - started) / 1000).toFixed(1);
    results.push({ name: scenario.name, error, retried });
    log(`${error ? "✗" : "✓"} ${scenario.name} (${seconds} s${retried ? ", second try" : ""})`);
    if (error) log(`  ${String(error.message).replaceAll("\n", "\n  ")}`);
    if (error?.launch) log(`  app.log:\n${error.log ?? ""}`);
    // Every scenario starts the app: when it cannot be driven at all, the others would only wait as long.
    if (error?.launch && results.length === 1) {
      log("The app could not be started through the WebDriver: the other scenarios and the measures are skipped.");
      unusable = true;
      break;
    }
  }
}

let over = false;
if ((!filter || filter === "perf") && !unusable) {
  try {
    const values = await measure(port, (m) => log(`  ${m}`));
    const { table, over: past } = report(values, platform);
    over = past;
    log(`\n${table}\n`);
    mkdirSync(outDir, { recursive: true });
    writeFileSync(join(outDir, `perf-${platform}.json`), JSON.stringify(values, null, 2));
    if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${table}\n\n`);
  } catch (e) {
    over = true;
    log(`✗ performance measures: ${e.stack ?? e}`);
  }
}

const failed = results.filter((r) => r.error);
const flaky = results.filter((r) => r.retried && !r.error);
log(`\n${results.length - failed.length}/${results.length} scenarios passed${flaky.length ? `, ${flaky.length} on a second try` : ""}.`);
if (process.env.GITHUB_STEP_SUMMARY && results.length) {
  const lines = results.map((r) => `| ${r.error ? "✗" : "✓"} | ${r.name} | ${r.retried ? "second try" : ""} |`);
  appendFileSync(process.env.GITHUB_STEP_SUMMARY, `### End-to-end (${platform})\n\n| | Scenario | |\n|---|---|---|\n${lines.join("\n")}\n\n`);
}
process.exit(failed.length || over ? 1 : 0);
