// The performance budgets of ARCHITECTURE §2.4, measured on the real app. Linux reads /proc; Windows asks CIM.
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import { createSandbox, windows } from "./sandbox.mjs";
import { helpers } from "./harness.mjs";
import { Key } from "./webdriver.mjs";

/**
 * `budget`: ARCHITECTURE §2.4. `limit`: what fails the CI job on Linux, the budget plus the margin a shared CI
 * runner and a debug build need, set from repeated measures (see docs/STATUS.md §4); null: reported only.
 */
export const measures = {
  coldStart: { label: "Cold start to a usable window", unit: "ms", budget: 400, limit: null },
  idleRam: { label: "RAM at rest, window open, no agent", unit: "MB", budget: 120, limit: null },
  hiddenTerminalRam: { label: "RAM per hidden terminal (10,000 lines)", unit: "MB", budget: 3, limit: null },
  idleCpu: { label: "CPU at rest, over 10 s", unit: "%", budget: 1, limit: null },
  keyEcho: { label: "Keystroke to echo (median of 30)", unit: "ms", budget: 16, limit: null },
};

const SESSIONS = 5;
const IDLE_SECONDS = 10;

// Process facts: `{pid, ppid, name, mem (bytes), cpu (seconds), started (epoch ms)}` for every process.
function processesLinux() {
  const tick = Number(execFileSync("getconf", ["CLK_TCK"], { encoding: "utf8" })) || 100;
  const uptime = Number(readFileSync("/proc/uptime", "utf8").split(" ")[0]);
  const now = Date.now();
  const list = [];
  for (const pid of readdirSync("/proc").filter((d) => /^\d+$/.test(d))) {
    try {
      const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
      const name = stat.slice(stat.indexOf("(") + 1, stat.lastIndexOf(")"));
      const f = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
      const pss = readFileSync(`/proc/${pid}/smaps_rollup`, "utf8").match(/^Pss:\s+(\d+) kB/m);
      list.push({
        pid: Number(pid),
        ppid: Number(f[1]),
        name,
        mem: pss ? Number(pss[1]) * 1024 : 0,
        cpu: (Number(f[11]) + Number(f[12])) / tick,
        started: now - (uptime - Number(f[19]) / tick) * 1000,
      });
    } catch {
      // Gone meanwhile, or not ours to read.
    }
  }
  return list;
}

function processesWindows() {
  const script =
    "Get-CimInstance Win32_Process | ForEach-Object { [pscustomobject]@{ pid = $_.ProcessId; ppid = $_.ParentProcessId; " +
    "name = $_.Name; mem = $_.PrivatePageCount; cpu = ($_.UserModeTime + $_.KernelModeTime) / 1e7; " +
    "started = ([DateTimeOffset]$_.CreationDate).ToUnixTimeMilliseconds() } } | ConvertTo-Json -Compress";
  return JSON.parse(execFileSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8", maxBuffer: 1 << 26 }));
}

const processes = () => (windows ? processesWindows() : processesLinux());

/** The app under `driver` and every process it started, with its own first. */
function appTree(driverPid, only = () => true) {
  const all = processes();
  const children = (pid) => all.filter((p) => p.ppid === pid);
  const below = (pid) => children(pid).flatMap((c) => [c, ...below(c.pid)]);
  const app = below(driverPid).find((p) => /^termf-app/.test(p.name));
  if (!app) throw new Error("the app's process is not found");
  return [app, ...below(app.pid).filter(only)];
}

const sum = (list, key) => list.reduce((n, p) => n + p[key], 0);
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const mb = (bytes) => bytes / 1024 / 1024;
// The shells of the terminals are the user's programs, not TermF's cost.
const ours = (p) => !/^(bash|sh|zsh|powershell|pwsh|conhost|cmd|seq)/i.test(p.name);

function onPage(sid) {
  const tile = document.querySelector(`.tile[data-session="${sid}"]`);
  const rows = tile.querySelector(".xterm-rows");
  const textarea = tile.querySelector(".xterm-helper-textarea");
  const echo = (window.__e2e.echo = []);
  let typed = null;
  textarea.addEventListener("keydown", () => (typed = { at: performance.now(), before: rows.textContent }), true);
  new MutationObserver(() => {
    // The cursor blinks: only a change of the text is the echo.
    if (typed && rows.textContent !== typed.before) echo.push(performance.now() - typed.at), (typed = null);
  }).observe(rows, { subtree: true, childList: true, characterData: true });
  return sid;
}

/** Runs every measure and returns `{key: value}`; `port()` as for the scenarios. */
export async function measure(port, log) {
  const out = {};
  const starts = [];
  let box;
  let t;
  for (let i = 0; i < 5; i++) {
    t?.app && (await t.stop(), box.cleanup());
    box = createSandbox();
    t = helpers(box, port);
    const app = await t.start();
    const shown = await t.s.wait(
      async () => (await t.s.exec(() => !!document.querySelector('.nav [data-project="acme"]'))) && Date.now(),
      { what: "the window to be usable", timeout: 60_000, every: 10 },
    );
    starts.push(shown - appTree(app.driver.pid)[0].started);
  }
  out.coldStart = median(starts);
  log(`cold starts: ${starts.map(Math.round).join(", ")} ms`);
  try {
    const driver = t.app.driver.pid;
    await t.ready();
    await t.settled("home");
    out.idleRam = mb(sum(appTree(driver, ours), "mem"));
    const before = sum(appTree(driver), "cpu");
    await sleep(IDLE_SECONDS * 1000);
    out.idleCpu = ((sum(appTree(driver), "cpu") - before) / IDLE_SECONDS) * 100;

    await t.project();
    const base = sum(appTree(driver, ours), "mem");
    const fill = windows ? "1..10000" : "seq 1 10000";
    for (let i = 0; i < SESSIONS; i++) {
      await t.command(`repo-shell-${box.acme}`);
      const id = await t.s.wait(() =>
        t.s.exec((n) => {
          const tiles = document.querySelectorAll(".tile[data-session]");
          return tiles.length === n && tiles[n - 1].dataset.session;
        }, i + 1),
      );
      await t.typeIn(id, fill + Key.enter);
      await t.s.wait(async () => (await t.terminal(id)).split("\n").some((l) => l.trim() === "10000"), { what: "10,000 lines", timeout: 60_000 });
    }
    await t.view("home");
    out.hiddenTerminalRam = mb(sum(appTree(driver, ours), "mem") - base) / SESSIONS;

    await t.command(`repo-shell-${box.acme}`);
    const id = await t.s.wait(() =>
      t.s.exec((n) => document.querySelectorAll(".tile[data-session]")[n]?.dataset.session, SESSIONS),
    );
    await t.s.waitFor(`.tile[data-session="${id}"] .xterm-rows`);
    await t.s.exec(onPage, id);
    await t.s.click(`.tile[data-session="${id}"] .xterm-screen`);
    for (let i = 1; i <= 30; i++) {
      await t.s.keys("x");
      await t.s.wait(async () => (await t.s.exec(() => window.__e2e.echo.length)) >= i, { what: `echo ${i}` });
    }
    const echoes = await t.s.exec(() => window.__e2e.echo);
    out.keyEcho = median(echoes);
    log(`echoes: ${echoes.map((x) => x.toFixed(1)).join(", ")} ms`);
  } finally {
    await t.stop();
    box.cleanup();
  }
  return out;
}

/** The table of the results, in Markdown, and whether one is past its limit. */
export function report(values, platform) {
  const rows = Object.entries(measures).map(([key, m]) => {
    const v = values[key];
    const over = m.limit !== null && platform === "linux" && v > m.limit;
    const shown = v === undefined ? "not measured" : `${v.toFixed(m.unit === "%" ? 2 : 1)} ${m.unit}`;
    const limit = m.limit === null ? "reported" : `${m.limit} ${m.unit}`;
    return { over, line: `| ${m.label} | ${shown} | ${m.budget} ${m.unit} | ${limit} | ${over ? "over" : "ok"} |` };
  });
  const table = [
    `### Performance (${platform}, debug build)`,
    "",
    "| Measure | Value | Budget (§2.4) | CI limit | |",
    "|---|---|---|---|---|",
    ...rows.map((r) => r.line),
  ].join("\n");
  return { table, over: rows.some((r) => r.over) };
}
