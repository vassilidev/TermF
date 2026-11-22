// Starts the real app through tauri-driver in a sandbox, and the helpers every scenario uses.
import { spawn, execFileSync } from "node:child_process";
import { once } from "node:events";
import { closeSync, existsSync, openSync, readFileSync } from "node:fs";
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

/** The interface text of `key` in `lang`, for the one-line texts. */
export function text(lang, key) {
  return block(lang).match(new RegExp(`^ {2}${key}: "(.*)",$`, "m"))?.[1];
}

/** The ids of `views.ts`: every view the navigation and the palette know. */
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
  window.__e2e = { errors };
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

/** What is wrong on screen: errors caught in the page, untranslated texts, visible error notices. */
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
  if (driver.exitCode !== null || driver.signalCode !== null) return;
  if (windows) execFileSync("taskkill", ["/PID", String(driver.pid), "/T", "/F"], { stdio: "ignore" });
  else process.kill(-driver.pid, "SIGKILL");
}

/**
 * The app started in `box`, driven through tauri-driver on `port` (its native driver on `port + 1`). `args` go to
 * the app on Linux, to WebView2 on Windows.
 */
export async function launch(box, port, { args = [] } = {}) {
  const log = join(box.root, "app.log");
  const fd = openSync(log, "a");
  const driver = spawn(tauriDriver(), ["--port", String(port), "--native-port", String(port + 1)], {
    env: box.env,
    stdio: ["ignore", fd, fd],
    detached: !windows,
  });
  closeSync(fd);
  const exited = once(driver, "exit");
  const base = `http://127.0.0.1:${port}`;
  try {
    await new Session(base, "").wait(() => fetch(`${base}/status`).then((r) => r.ok, () => false), {
      what: "tauri-driver to listen",
      timeout: 30_000,
    });
    const asked = Date.now();
    const s = await Session.create(base, { browserName: "wry", "tauri:options": { application: APP, args } });
    await s.exec(prepare);
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
    kill(driver);
    await exited;
    throw e;
  }
}

/** The helpers a scenario gets as `t`; `port()` gives a free pair of ports for each start of the app. */
export function helpers(box, port) {
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
      app = await launch(box, port(), options);
      return app;
    },
    async stop() {
      await app?.stop();
    },
    async restart(options) {
      await t.stop();
      return t.start(options);
    },

    /** The window is up and the workspace read. At the first launch the tour shows: skipped unless `keepTour`. */
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

    async palette() {
      await t.s.exec(() => document.activeElement instanceof HTMLElement && document.activeElement.blur());
      await t.s.press(Key.ctrl, "k");
      await t.s.waitFor("dialog.palette");
    },

    /** Runs a palette command by its id (`commands.ts`). */
    async command(id) {
      await t.palette();
      await t.s.click(t.sel("data-testid", `command-${id}`));
      await t.s.waitGone("dialog.palette");
    },

    async project(id = "acme") {
      await t.command(`project-${id}`);
      await t.s.waitFor(`.nav ${t.sel("data-project", id)}[aria-current="true"]`);
    },

    /** Shows a view from the palette, waits for it to settle, and checks it is clean. */
    async view(id, { allow } = {}) {
      if (!(await t.s.exec(visible, t.shown(id)))) await t.command(`view-${id}`);
      await t.s.waitFor(t.shown(id));
      await t.settled(id);
      await t.clean({ allow });
    },

    /** No spinner left in the view's panel. */
    settled(view) {
      const css = `${t.shown(view)} .spin`;
      return t.s.wait(() => t.s.exec((c) => ![...document.querySelectorAll(c)].some((e) => e.getClientRects().length), css), {
        what: `${view} to finish loading`,
        timeout: 30_000,
      });
    },

    /** Fails on an error in the page, an untranslated text, an error shown, or a panic of the backend. */
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

    /** The id of the session whose terminal shows, once it shows. */
    tile() {
      return t.s.wait(
        () => t.s.exec(() => document.querySelector(".tile[data-session]:not([hidden]) .xterm-rows")?.closest(".tile").dataset.session),
        { what: "a terminal tile" },
      );
    },

    /** The text of the terminal in the tile of session `id` (xterm's DOM renderer). */
    terminal(id) {
      return t.s.exec((sid) => document.querySelector(`.tile[data-session="${sid}"] .xterm-rows`)?.innerText ?? "", id);
    },

    /** Clicks into a tile's terminal, then types `text` there. */
    async typeIn(id, text) {
      await t.s.click(`.tile[data-session="${id}"] .xterm-screen`);
      await t.s.keys(text);
    },
  };
  return t;
}
