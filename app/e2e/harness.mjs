// Starts the real app through tauri-driver in a sandbox, and the helpers every scenario uses.
import { spawn, execFileSync } from "node:child_process";
import { appendFileSync, closeSync, existsSync, openSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Key, Session } from "./webdriver.mjs";
import { windows } from "./sandbox.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../..");

export const APP =
  process.env.TERMF_E2E_APP ?? join(repo, "target", "debug", windows ? "termf-app.exe" : "termf-app");

const i18n = readFileSync(join(here, "../src/lib/i18n.ts"), "utf8");
const block = (lang) => {
  const start = i18n.indexOf(`const ${lang} = {`);
  return i18n.slice(start, i18n.indexOf("\n};", start));
};

/** The camelCase keys of the interface texts: one of them shown as is means a text that was never translated. */
export const rawKeys = [...block("fr").matchAll(/^ {2}([A-Za-z_]\w*):/gm)].map((m) => m[1]).filter((k) => /^[a-z]+[A-Z0-9]/.test(k));

export function text(lang, key) {
  return block(lang).match(new RegExp(`^ {2}${key}: "(.*)",$`, "m"))?.[1];
}

export const viewIds = (() => {
  const src = readFileSync(join(here, "../src/lib/views.ts"), "utf8");
  return [...src.slice(src.indexOf("export const views")).matchAll(/^ {4}id: "(\w+)",$/gm)].map((m) => m[1]);
})();

function tauriDriver() {
  const name = windows ? "tauri-driver.exe" : "tauri-driver";
  const cargo = join(process.env.CARGO_HOME ?? join(process.env.HOME ?? process.env.USERPROFILE ?? "", ".cargo"), "bin", name);
  return existsSync(cargo) ? cargo : name;
}

/** Runs in the page before anything is tested: collects errors, and keeps xterm on its DOM renderer to read it. */
function prepare() {
  if (window.__e2e) return;
  const errors = [];
  window.__e2e = { errors, clicks: [], synthetic: 0 };
  document.addEventListener("click", (e) => window.__e2e.clicks.push(e.target), true);
  const push = (m) => errors.push(String(m).slice(0, 600));
  const error = console.error.bind(console);
  console.error = (...a) => (push(a.map((x) => x?.stack ?? x).join(" ")), error(...a));
  addEventListener("error", (e) => push(e.error?.stack ?? e.message));
  addEventListener("unhandledrejection", (e) => push(e.reason?.stack ?? e.reason));
  const getContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function (type, ...rest) {
    return /webgl/i.test(type) ? null : getContext.call(this, type, ...rest);
  };
}

function inspect(keys) {
  const set = new Set(keys);
  const raw = new Set();
  const visible = (el) => el.getClientRects().length > 0;
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const el = n.parentElement;
    if (!el || el.closest(".xterm, .cm-editor, pre, code, textarea, script, style") || !visible(el)) continue;
    for (const w of n.data.match(/[A-Za-z][A-Za-z0-9]*/g) ?? []) if (set.has(w)) raw.add(w);
  }
  for (const el of document.querySelectorAll("[aria-label], [title], [placeholder]")) {
    for (const a of ["aria-label", "title", "placeholder"]) {
      const v = el.getAttribute(a)?.trim();
      if (v && set.has(v)) raw.add(v);
    }
  }
  const alerts = [...document.querySelectorAll(".banner.error, .notice.error, [role=alert]")]
    .filter(visible)
    .map((e) => e.innerText.trim())
    .filter(Boolean);
  return { errors: window.__e2e?.errors ?? [], raw: [...raw], alerts };
}

function visible(css) {
  const el = document.querySelector(css);
  return !!el && el.getClientRects().length > 0;
}

function kill(driver) {
  if (!driver.pid || driver.exitCode !== null || driver.signalCode !== null) return;
  if (windows) {
    try {
      execFileSync("taskkill", ["/PID", String(driver.pid), "/T", "/F"], { stdio: "ignore" });
    } catch {
      // Already gone.
    }
  }
  else process.kill(-driver.pid, "SIGKILL");
}

/** tauri-driver on `port`, its native driver on `port + 1`. `args` go to the app on Linux, to WebView2 on Windows. */
export async function launch(box, port, { args = [] } = {}, signal) {
  const log = join(box.root, "app.log");
  const fd = openSync(log, "a");
  const driver = spawn(tauriDriver(), ["--port", String(port), "--native-port", String(port + 1)], {
    env: box.env,
    stdio: ["ignore", fd, fd],
    detached: !windows,
  });
  closeSync(fd);
  let failed = null;
  const exited = new Promise((resolve) => driver.once("exit", resolve).once("error", (e) => resolve((failed = e))));
  const base = `http://127.0.0.1:${port}`;
  try {
    await new Session(base, "", signal).wait(() => failed || fetch(`${base}/status`).then((r) => r.ok, () => false), {
      what: "tauri-driver to listen",
      timeout: 30_000,
    });
    if (failed) throw new Error(`cannot run tauri-driver: ${failed.message}`);
    const asked = Date.now();
    const s = await Session.create(base, { browserName: "wry", "tauri:options": { application: APP, args } }, signal);
    await s.exec(prepare);
    // WebKitGTK under xvfb drops the first click of a pointer that never entered the window.
    const enter = [{ type: "pointerMove", origin: "viewport", x: 1, y: 1, duration: 0 }];
    await s.cmd("POST", "/actions", { actions: [{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions: enter }] });
    return {
      s,
      log,
      asked,
      driver,
      async stop() {
        await s.close().catch(() => {});
        kill(driver);
        await exited;
      },
    };
  } catch (e) {
    if (windows) {
      // What runs when the session could not open: the app, its WebView2, the drivers.
      const list = "Get-Process termf-app,msedgewebview2,msedgedriver,tauri-driver -ErrorAction SilentlyContinue | Format-Table -AutoSize Id,ProcessName,StartTime | Out-String -Width 200";
      try {
        appendFileSync(log, execFileSync("powershell", ["-NoProfile", "-Command", list], { encoding: "utf8" }));
      } catch {
        // Diagnostics only.
      }
    }
    kill(driver);
    await exited;
    throw e;
  }
}

/** `port()`: a free pair of ports for each start of the app. `signal`: the scenario is past its time. */
export function helpers(box, port, signal) {
  let app = null;
  const t = {
    box,
    views: viewIds,
    linux: process.platform === "linux",
    windows,
    get s() {
      return app.s;
    },
    get app() {
      return app;
    },
    shown: (view) => `[data-panel="${view}"]:not([hidden])`,
    sel: (attr, value) => `[${attr}=${JSON.stringify(value)}]`,

    async start(options) {
      signal?.throwIfAborted();
      app = await launch(box, port(), options, signal);
      return app;
    },
    async stop() {
      await app?.stop();
    },
    async restart(options) {
      await t.stop();
      return t.start(options);
    },

    /** The tour shows once the workspace is read, at the first launch only. */
    async ready({ keepTour = false } = {}) {
      const s = t.s;
      await s.waitFor(".nav", { timeout: 60_000 });
      if (await s.exec(() => localStorage.getItem("termf.tour"))) return s.waitFor(t.shown("home"));
      await s.waitFor('[aria-labelledby="tour-title"]', { timeout: 60_000 });
      if (!keepTour) {
        await s.click('[aria-labelledby="tour-title"] footer .quiet');
        await s.waitGone('[aria-labelledby="tour-title"]');
      }
    },

    /** Ctrl+K; a chord WebKitGTK drops under xvfb is sent by script, counted like the clicks. */
    async palette() {
      await t.s.exec(() => document.activeElement instanceof HTMLElement && document.activeElement.blur());
      await t.s.press(Key.ctrl, "k");
      const opened = await t.s.waitFor("dialog.palette", { timeout: 1500 }).catch(() => false);
      if (opened) return;
      await t.s.exec(() => {
        const init = { key: "k", code: "KeyK", ctrlKey: true, bubbles: true, cancelable: true };
        document.body.dispatchEvent(new KeyboardEvent("keydown", init));
        window.__e2e.synthetic++;
      });
      await t.s.waitFor("dialog.palette");
    },

    /** `id`: as in `commands.ts`. */
    async command(id) {
      await t.palette();
      await t.s.click(t.sel("data-testid", `command-${id}`));
      await t.s.waitGone("dialog.palette");
    },

    async project(id = "acme") {
      await t.command(`project-${id}`);
      await t.s.waitFor(`.nav ${t.sel("data-project", id)}[aria-current="true"]`);
    },

    async view(id, { allow } = {}) {
      if (!(await t.s.exec(visible, t.shown(id)))) await t.command(`view-${id}`);
      await t.s.waitFor(t.shown(id));
      await t.settled(id);
      await t.clean({ allow });
    },

    settled(view) {
      const css = `${t.shown(view)} .spin`;
      return t.s.wait(() => t.s.exec((c) => ![...document.querySelectorAll(c)].some((e) => e.getClientRects().length), css), {
        what: `${view} to finish loading`,
        timeout: 30_000,
      });
    },

    async clean({ allow = [] } = {}) {
      const found = await t.s.exec(inspect, rawKeys);
      const ok = (text) => allow.some((re) => re.test(text));
      const problems = [
        ...found.errors.filter((e) => !ok(e)).map((e) => `error in the page: ${e}`),
        ...found.raw.map((k) => `untranslated text: ${k}`),
        ...found.alerts.filter((a) => !ok(a)).map((a) => `error shown: ${a}`),
      ];
      const log = readFileSync(app.log, "utf8");
      const panic = log.indexOf("panicked at");
      if (panic >= 0) problems.push(`the backend panicked: ${log.slice(panic, panic + 400)}`);
      if (problems.length) throw new Error(problems.join("\n"));
    },

    tile() {
      return t.s.wait(
        () => t.s.exec(() => document.querySelector(".tile[data-session]:not([hidden]) .xterm-rows")?.closest(".tile").dataset.session),
        { what: "a terminal tile" },
      );
    },

    /** Read from xterm's DOM renderer, which `prepare` keeps. */
    terminal(id) {
      return t.s.exec((sid) => document.querySelector(`.tile[data-session="${sid}"] .xterm-rows`)?.innerText ?? "", id);
    },

    /** The shell's prompt is drawn (`$ ` from the sandbox's profile, `PS …>` for PowerShell). */
    prompt(id) {
      return t.s.wait(
        async () => /(\$|>)\s*$/.test((await t.terminal(id)).trimEnd().split("\n").at(-1) ?? ""),
        { what: `the prompt in terminal ${id}`, timeout: 30_000 },
      );
    },

    async typeIn(id, text) {
      await t.s.click(`.tile[data-session="${id}"] .xterm-screen`);
      await t.s.exec((sid) => document.querySelector(`.tile[data-session="${sid}"] .xterm-helper-textarea`).focus(), id);
      await t.s.keys(text);
    },
  };
  return t;
}
