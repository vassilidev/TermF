// The performance budgets of ARCHITECTURE §2.4, measured on the real app. Linux reads /proc; Windows asks CIM.
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import { createSandbox, windows } from "./sandbox.mjs";
import { APP, helpers } from "./harness.mjs";
import { Key } from "./webdriver.mjs";

/** The build measured: `TERMF_E2E_APP` pointing into `target/release` is the product, anything else the debug build. */
export const build = /target[\\/]release[\\/]/.test(APP) ? "release" : "debug";

/**
 * `budget`: ARCHITECTURE §2.4. `limit`: what fails the Linux job, per build: the budget with a margin for a shared
 * runner, or, where the app is still past its budget (docs/STATUS.md §4), the level measured on the CI runner plus a
 * margin, so that a regression still fails. Windows: reported only.
 */
export const measures = {
  coldStart: { label: "Cold start to a usable window", unit: "ms", budget: 400, limit: { debug: 900, release: 800 } },
  idleRam: { label: "RAM at rest, window open, no agent", unit: "MB", budget: 120, limit: { debug: 650, release: 600 } },
  hiddenTerminalRam: { label: "RAM per hidden terminal (10,000 lines)", unit: "MB", budget: 3, limit: { debug: 35, release: 20 } },
  idleCpu: { label: "CPU at rest, over 10 s", unit: "%", budget: 1, limit: { debug: 2, release: 2 } },
  keyEcho: { label: "Keystroke to echo (median of 30)", unit: "ms", budget: 16, limit: { debug: 24, release: 24 } },
};

const SESSIONS = 5;
const IDLE_SECONDS = 10;
// The web process gives back what hidden terminals let go of on its own schedule, not at once.
const SETTLE_SECONDS = 10;

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
    const started = appTree(app.root)[0].started;
    starts.push(shown - started);
    // Where the time goes: the backend and the webview until the page starts loading, then the page.
    const page = await t.s.exec(() => performance.timeOrigin);
    log(`cold start ${i + 1}: ${Math.round(shown - started)} ms (page from ${Math.round(page - started)} ms)`);
  }
  out.coldStart = median(starts);
  try {
    const driver = t.app.root;
    await t.ready();
    await t.settled("home");
    // What the app does once at startup (the update check 5 s in) is not rest.
    await sleep(SETTLE_SECONDS * 1000);
    out.idleRam = mb(sum(appTree(driver, ours), "mem"));
    const before = sum(appTree(driver), "cpu");
    await sleep(IDLE_SECONDS * 1000);
    out.idleCpu = ((sum(appTree(driver), "cpu") - before) / IDLE_SECONDS) * 100;

    await t.project();
    const open = async (n) => {
      await t.command(`repo-shell-${box.acme}`);
      const id = await t.s.wait(() =>
        t.s.exec((k) => {
          const tiles = document.querySelectorAll(".tile[data-session]");
          return tiles.length === k && tiles[k - 1].dataset.session;
        }, n),
      );
      await t.prompt(id);
      return id;
    };
    // A first terminal loads xterm and the view, once: not what each hidden terminal costs.
    await open(1);
    await t.view("home");
    await sleep(SETTLE_SECONDS * 1000);
    const base = sum(appTree(driver, ours), "mem");
    const fill = windows ? "1..10000" : "seq 1 10000";
    for (let i = 0; i < SESSIONS; i++) {
      const id = await open(i + 2);
      await t.typeIn(id, fill + Key.enter);
      await t.s.wait(async () => (await t.terminal(id)).split("\n").some((l) => l.trim() === "10000"), { what: "10,000 lines", timeout: 60_000 });
    }
    await t.view("home");
    await sleep(SETTLE_SECONDS * 1000);
    out.hiddenTerminalRam = mb(sum(appTree(driver, ours), "mem") - base) / SESSIONS;

    const id = await open(SESSIONS + 2);
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

export function report(values, platform) {
  const rows = Object.entries(measures).map(([key, m]) => {
    const v = values[key];
    const over = platform === "linux" && v > m.limit[build];
    const shown = v === undefined ? "not measured" : `${v.toFixed(m.unit === "%" ? 2 : 1)} ${m.unit}`;
    const limit = platform === "linux" ? `${m.limit[build]} ${m.unit}` : "reported";
    return { over, line: `| ${m.label} | ${shown} | ${m.budget} ${m.unit} | ${limit} | ${over ? "over" : "ok"} |` };
  });
  const table = [
    `### Performance (${platform}, ${build} build)`,
    "",
    "| Measure | Value | Budget (§2.4) | CI limit | |",
    "|---|---|---|---|---|",
    ...rows.map((r) => r.line),
  ].join("\n");
  return { table, over: rows.some((r) => r.over) };
}
