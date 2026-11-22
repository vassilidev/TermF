// A W3C WebDriver client on Node's fetch, for tauri-driver: just what the scenarios use.
const ELEMENT = "element-6066-11e4-a52e-4f735466cecc";

export const Key = { enter: "\uE007", escape: "\uE00C", ctrl: "\uE009", shift: "\uE008", end: "\uE010", tab: "\uE004" };

export class Session {
  constructor(base, id) {
    this.base = base;
    this.id = id;
  }

  static async create(base, capabilities) {
    const value = await call(base, "POST", "/session", { capabilities: { alwaysMatch: capabilities } });
    return new Session(base, value.sessionId);
  }

  cmd(method, path, body) {
    return call(this.base, method, `/session/${this.id}${path}`, body);
  }

  /** Runs `fn(...args)` in the page; `fn` must not close over anything. */
  exec(fn, ...args) {
    return this.cmd("POST", "/execute/sync", { script: `return (${fn}).apply(null, arguments)`, args });
  }

  /** Every element matching `css`. */
  async findAll(css) {
    const found = await this.cmd("POST", "/elements", { using: "css selector", value: css });
    return found.map((f) => f[ELEMENT]);
  }

  /** The first element matching `css`, or null. */
  async find(css) {
    const found = await this.cmd("POST", "/elements", { using: "css selector", value: css });
    return found.length ? found[0][ELEMENT] : null;
  }

  async must(css, what = css) {
    return (await this.wait(() => this.find(css), { what: `${what} to be there` }));
  }

  async click(css) {
    const el = await this.wait(() => this.exec(visibleOrNull, css).then((ok) => ok && this.find(css)), {
      what: `${css} to be visible`,
    });
    await this.cmd("POST", `/element/${el}/click`, {});
  }

  /** Types into the element, which takes the focus first; `text` may hold `Key` characters. */
  async type(css, text) {
    const el = await this.must(css);
    await this.cmd("POST", `/element/${el}/value`, { text, value: [...text] });
  }

  /** Types `text` (which may hold `Key` characters) on whatever has the focus, as a keyboard would. */
  async keys(text) {
    const actions = [...text].flatMap((value) => [{ type: "keyDown", value }, { type: "keyUp", value }]);
    await this.cmd("POST", "/actions", { actions: [{ type: "key", id: "keyboard", actions }] });
  }

  /** A chord such as Ctrl+K, pressed on whatever has the focus. */
  async press(...keys) {
    const down = keys.map((value) => ({ type: "keyDown", value }));
    const up = keys.toReversed().map((value) => ({ type: "keyUp", value }));
    await this.cmd("POST", "/actions", { actions: [{ type: "key", id: "keyboard", actions: [...down, ...up] }] });
    await this.cmd("DELETE", "/actions");
  }

  text(css) {
    return this.exec((s) => document.querySelector(s)?.innerText ?? null, css);
  }

  /** Polls `check` until it returns something truthy, which it returns; fails after `timeout` with `what`. */
  async wait(check, { timeout = 15_000, what = "the condition", every = 50 } = {}) {
    const deadline = Date.now() + timeout;
    let last;
    for (;;) {
      try {
        const value = await check();
        if (value) return value;
      } catch (e) {
        last = e;
      }
      if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}${last ? ` (last error: ${last.message})` : ""}`);
      await new Promise((r) => setTimeout(r, every));
    }
  }

  /** Waits for `css` to match a visible element, or for the predicate run in the page to hold. */
  waitFor(css, opts = {}) {
    return this.wait(() => this.exec(visibleOrNull, css), { what: `${css} to be visible`, ...opts });
  }

  waitGone(css, opts = {}) {
    return this.wait(async () => !(await this.exec(visibleOrNull, css)), { what: `${css} to go away`, ...opts });
  }

  async screenshot() {
    return Buffer.from(await this.cmd("GET", "/screenshot"), "base64");
  }

  source() {
    return this.cmd("GET", "/source");
  }

  async close() {
    await this.cmd("DELETE", "");
  }
}

function visibleOrNull(css) {
  const el = document.querySelector(css);
  return !!el && el.getClientRects().length > 0 && getComputedStyle(el).visibility !== "hidden";
}

async function call(base, method, path, body) {
  const res = await fetch(base + path, {
    method,
    headers: body ? { "content-type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(60_000),
  });
  const json = await res.json().catch(() => ({}));
  if (!res.ok || json.value?.error) {
    const v = json.value ?? {};
    throw new Error(`${method} ${path}: ${v.error ?? res.status} ${v.message ?? ""}`.trim());
  }
  return json.value;
}
