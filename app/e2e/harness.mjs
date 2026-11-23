// Starts the real app in a sandbox, through tauri-driver (Linux) or the app's own bridge (macOS, Windows), and the
// helpers every scenario uses.
import { spawn, execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { closeSync, existsSync, openSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Key, Session } from "./webdriver.mjs";
import { Bridge } from "./bridge.mjs";
import { windows } from "./sandbox.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../..");

export const APP =
  process.env.TERMF_E2E_APP ?? join(repo, "target", "debug", windows ? "termf-app.exe" : "termf-app");

const i18n = readFileSync(join(here, "../src/lib/i18n.ts"), "utf8");
// `const en: Record<Key, string> = {` as well as `const fr = {`.
const block = (lang) => {
  const start = i18n.search(new RegExp(`^const ${lang}\\b[^=]*= \\{`, "m"));
  if (start < 0) throw new Error(`no "${lang}" block in i18n.ts`);
  return i18n.slice(start, i18n.indexOf("\n};", start));
};

/** The camelCase keys of the interface texts: one of them shown as is means a text that was never translated. */
export const rawKeys = [...block("fr").matchAll(/^ {2}([A-Za-z_]\w*):/gm)].map((m) => m[1]).filter((k) => /^[a-z]+[A-Z0-9]/.test(k));

const strings = (lang) => {
  const out = new Map();
  for (const m of block(lang).matchAll(/^ {2}(\w+): "(.*)",$/gm)) {
    try {
      out.set(m[1], JSON.parse(`"${m[2]}"`));
    } catch {
      // A value written across lines or with a quote of its own: not compared.
    }
  }
  return out;
};

/** The English texts of several words whose French differs: one of them in the French interface is untranslated. */
export const englishOnly = (() => {
  const fr = strings("fr");
  return [...strings("en")].filter(([k, v]) => fr.has(k) && fr.get(k) !== v && /\w+ \w+/.test(v) && !/[{<]/.test(v)).map(([, v]) => v);
})();

export function text(lang, key) {
  return block(lang).match(new RegExp(`^ {2}${key}: "(.*)",$`, "m"))?.[1];
}

/** `TERMF_E2E_TRANSPORT=bridge` drives Linux through the bridge too; macOS has no WebDriver for its webview. */
export const transport = process.env.TERMF_E2E_TRANSPORT ?? (process.platform === "linux" ? "webdriver" : "bridge");

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
  window.__e2e = { errors, aim: null, hit: false, synthetic: 0 };
  // Checked as the click comes in: once the page has handled it, what was clicked may be gone or replaced.
  document.addEventListener("click", (e) => window.__e2e.aim?.contains(e.target) && (window.__e2e.hit = true), true);
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

/** `english`: checked only when the page is in French. */
function inspect(keys, english) {
  const set = new Set(keys);
  const en = new Set(document.documentElement.lang === "fr" ? english : []);
  const raw = new Set();
  const untranslated = new Set();
  const visible = (el) => el.getClientRects().length > 0;
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const el = n.parentElement;
    if (!el || el.closest(".xterm, .cm-editor, pre, code, textarea, script, style") || !visible(el)) continue;
    for (const w of n.data.match(/[A-Za-z][A-Za-z0-9]*/g) ?? []) if (set.has(w)) raw.add(w);
    if (en.has(n.data.trim())) untranslated.add(n.data.trim());
  }
  for (const el of document.querySelectorAll("[aria-label], [title], [placeholder]")) {
    for (const a of ["aria-label", "title", "placeholder"]) {
      const v = el.getAttribute(a)?.trim();
      if (v && set.has(v)) raw.add(v);
      if (v && en.has(v)) untranslated.add(v);
    }
  }
  const alerts = [...document.querySelectorAll(".banner.error, .notice.error, [role=alert]")]
    .filter(visible)
    .map((e) => e.innerText.trim())
    .filter(Boolean);
  return { errors: window.__e2e?.errors ?? [], raw: [...raw], english: [...untranslated], alerts };
}

function visible(css) {
  const el = document.querySelector(css);
  return !!el && el.getClientRects().length > 0;
}

function kill(child) {
  if (!child.pid || child.exitCode !== null || child.signalCode !== null) return;
  if (!windows) return process.kill(-child.pid, "SIGKILL");
  try {
    execFileSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    // Already gone.
  }
}

function watch(child) {
  const state = { failed: null };
  state.exited = new Promise((resolve) => child.once("exit", resolve).once("error", (e) => resolve((state.failed = e))));
  return state;
}

/**
 * Linux: tauri-driver on `port`, WebKitWebDriver on `port + 1`, `args` to the app. Windows: Edge WebDriver launching
 * the app leaves its session hanging (the app does start), so the app is started here with WebView2's debugging
 * port open (`port + 1`, `args` added to WebView2's), and Edge WebDriver on `port` attaches to it, as Microsoft
 * documents for WebView2 apps; that port does not open yet (docs/STATUS.md §4).
 */
export async function launch(box, port, { args = [] } = {}, signal) {
  if (transport === "bridge") return launchBridge(box, port, args, signal);
  const log = join(box.root, "app.log");
  const fd = openSync(log, "a");
  const io = { stdio: ["ignore", fd, fd], detached: !windows };
  const debug = port + 1;
  const driver = windows
    ? spawn("msedgedriver", [`--port=${port}`], { env: box.env, ...io })
    : spawn(tauriDriver(), ["--port", String(port), "--native-port", String(debug)], { env: box.env, ...io });
  const app = windows
    ? spawn(APP, [], {
        env: { ...box.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: [`--remote-debugging-port=${debug}`, ...args].join(" ") },
        ...io,
      })
    : null;
  closeSync(fd);
  const children = [driver, app].filter(Boolean).map((child) => ({ child, ...watch(child) }));
  const failed = () => children.find((c) => c.failed)?.failed;
  const stop = async () => {
    for (const { child } of children) kill(child);
    await Promise.all(children.map((c) => c.exited));
  };
  const base = `http://127.0.0.1:${port}`;
  const probe = new Session(base, "", signal);
  try {
    await probe.wait(() => failed() || fetch(`${base}/status`).then((r) => r.ok, () => false), { what: "the WebDriver to listen", timeout: 30_000 });
    if (windows) {
      await probe.wait(() => failed() || fetch(`http://127.0.0.1:${debug}/json/version`).then((r) => r.ok, () => false), {
        what: "WebView2 to open its debugging port",
        timeout: 60_000,
      });
    }
    if (failed()) throw new Error(`cannot run the driver or the app: ${failed().message}`);
    const asked = Date.now();
    const capabilities = windows
      ? { browserName: "webview2", "ms:edgeOptions": { debuggerAddress: `127.0.0.1:${debug}` } }
      : { browserName: "wry", "tauri:options": { application: APP, args } };
    const s = await Session.create(base, capabilities, signal);
    await s.exec(prepare);
    return {
      s,
      log,
      asked,
      // The process the app runs under, for the measures.
      root: windows ? process.pid : driver.pid,
      async stop() {
        await s.close().catch(() => {});
        await stop();
      },
    };
  } catch (e) {
    await stop();
    throw e;
  }
}

/** The app's `e2e` build, its bridge on `port` with a fresh token; `prepare` runs before the page's own scripts. */
async function launchBridge(box, port, args, signal) {
  const log = join(box.root, "app.log");
  const fd = openSync(log, "a");
  const token = randomBytes(32).toString("hex");
  const env = { ...box.env, TERMF_E2E_PORT: String(port), TERMF_E2E_TOKEN: token, TERMF_E2E_INIT: `(${prepare})();` };
  // On Windows the arguments are WebView2's.
  if (windows) env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = args.join(" ");
  const asked = Date.now();
  const child = spawn(APP, windows ? [] : args, { env, stdio: ["ignore", fd, fd], detached: !windows });
  closeSync(fd);
  const state = watch(child);
  const stop = async () => {
    kill(child);
    await state.exited;
  };
  const s = new Bridge(`http://127.0.0.1:${port}`, token, signal);
  try {
    const up = () => s.exec(() => document.readyState === "complete" && !!window.__TAURI_INTERNALS__ && !!window.__e2e);
    const gone = () => state.failed || child.exitCode !== null || child.signalCode !== null;
    await s.wait(async () => gone() || (await up()), { what: "the app's bridge to answer", timeout: 60_000, every: 100 });
    if (gone()) throw new Error(`the app stopped (${state.failed?.message ?? child.exitCode ?? child.signalCode})`);
    return {
      s,
      log,
      asked,
      root: child.pid,
      async stop() {
        await s.close().catch(() => {});
        const quit = await Promise.race([state.exited.then(() => true), new Promise((r) => setTimeout(r, 5000, false))]);
        if (!quit) await stop();
      },
    };
  } catch (e) {
    await stop();
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
    mac: process.platform === "darwin",
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

    /** The introduction shows once the workspace is read, at the first launch only; later, the view last shown. */
    async ready({ keepIntro = false } = {}) {
      const s = t.s;
      await s.waitFor(".nav", { timeout: 60_000 });
      if (await s.exec(() => localStorage.getItem("termf.tour"))) return s.waitFor("[data-panel]:not([hidden])");
      await s.waitFor("dialog.intro[open]", { timeout: 60_000 });
      if (!keepIntro) {
        await s.click("dialog.intro .skip");
        await s.waitGone("dialog.intro");
      }
    },

    /** A Tauri event, as the backend sends it (the native menu, a file dropped from the Finder). */
    emit(event, payload) {
      return t.s.exec((e, p) => window.__TAURI_INTERNALS__.invoke("plugin:event|emit", { event: e, payload: p }), event, payload);
    },

    /** Ctrl+K; a chord WebKitGTK drops under xvfb is sent by script, counted like the clicks. */
    async palette() {
      await t.s.exec(() => document.activeElement instanceof HTMLElement && document.activeElement.blur());
      await t.s.press(Key.mod, "k");
      const opened = await t.s.waitFor("dialog.palette", { timeout: 1500 }).catch(() => false);
      if (opened) return;
      await t.s.exec(() => {
        const mod = navigator.platform.startsWith("Mac") ? "metaKey" : "ctrlKey";
        const init = { key: "k", code: "KeyK", [mod]: true, bubbles: true, cancelable: true };
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

    /** A `<select>`'s option, as a pick in its list would. */
    async choose(css, value) {
      const found = await t.s.exec(
        (c, v) => {
          const el = document.querySelector(c);
          if (!el || ![...el.options].some((o) => o.value === v && !o.disabled)) return false;
          el.value = v;
          el.dispatchEvent(new Event("input", { bubbles: true }));
          el.dispatchEvent(new Event("change", { bubbles: true }));
          return true;
        },
        css,
        value,
      );
      if (!found) throw new Error(`no option ${JSON.stringify(value)} in ${css}`);
    },

    async view(id, { allow } = {}) {
      if (!(await t.s.exec(visible, t.shown(id)))) await t.command(`view-${id}`);
      await t.s.waitFor(t.shown(id));
      await t.settled(id);
      await t.clean({ allow });
    },

    settled(view) {
      const css = `${t.shown(view)} .spin, ${t.shown(view)} [data-skeleton]`;
      return t.s.wait(() => t.s.exec((c) => ![...document.querySelectorAll(c)].some((e) => e.getClientRects().length), css), {
        what: `${view} to finish loading`,
        timeout: 30_000,
      });
    },

    async clean({ allow = [] } = {}) {
      const found = await t.s.exec(inspect, rawKeys, englishOnly);
      const ok = (text) => allow.some((re) => re.test(text));
      const problems = [
        ...found.errors.filter((e) => !ok(e)).map((e) => `error in the page: ${e}`),
        ...found.raw.map((k) => `untranslated text: ${k}`),
        ...found.english.map((k) => `English in the French interface: ${k}`),
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
