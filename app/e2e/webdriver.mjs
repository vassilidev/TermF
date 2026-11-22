// A W3C WebDriver client on Node's fetch, for tauri-driver: just what the scenarios use.
export const Key = { enter: "\uE007", escape: "\uE00C", ctrl: "\uE009", shift: "\uE008", end: "\uE010", tab: "\uE004", meta: "\uE03D", backspace: "\uE003" };
/** The app's shortcuts: \u2318 on macOS, Ctrl elsewhere. */
Key.mod = process.platform === "darwin" ? Key.meta : Key.ctrl;

/** `signal`: once aborted (a scenario past its time), every command and every wait fails at once. */
export class Session {
  constructor(base, id, signal) {
    this.base = base;
    this.id = id;
    this.signal = signal;
  }

  static async create(base, capabilities, signal) {
    const value = await call(base, "POST", "/session", { capabilities: { alwaysMatch: capabilities } }, signal);
    return new Session(base, value.sessionId, signal);
  }

  /** The same session without the scenario's signal: to collect what a failed or late scenario left. */
  detached() {
    return new this.constructor(this.base, this.id);
  }

  cmd(method, path, body) {
    return call(this.base, method, `/session/${this.id}${path}`, body, this.signal);
  }

  /** Runs `fn(...args)` in the page; `fn` must not close over anything. */
  exec(fn, ...args) {
    return this.cmd("POST", "/execute/sync", { script: `return (${fn}).apply(null, arguments)`, args });
  }

  /** Where a click on the `index`-th match of `css` lands, once it shows, stands still and nothing covers it. */
  point(css, index = 0) {
    const find = (c, i) => {
      const el = document.querySelectorAll(c)[i];
      if (!el || !el.getClientRects().length || getComputedStyle(el).visibility === "hidden") return null;
      el.scrollIntoView({ block: "nearest", inline: "nearest" });
      const r = el.getBoundingClientRect();
      const [x, y] = [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)];
      const top = document.elementFromPoint(x, y);
      return top && (top === el || el.contains(top)) ? { x, y } : null;
    };
    let before = null;
    // Twice at the same place: a card sliding into position would take the click elsewhere.
    const still = async () => {
      const at = await this.exec(find, css, index);
      const same = at && before && at.x === before.x && at.y === before.y;
      before = at;
      return same && at;
    };
    return this.wait(still, { what: `${css} to be visible, still and uncovered` });
  }

  /**
   * Pointer actions first, so the page gets trusted events as from a mouse. Under xvfb WebKitGTK drops some of them:
   * a click the page did not see on the element is then made by script, and counted in `window.__e2e.synthetic`.
   */
  async click(css, index = 0) {
    const { x, y } = await this.point(css, index);
    const before = await this.exec(() => window.__e2e?.clicks.length ?? 0);
    const actions = [
      { type: "pointerMove", origin: "viewport", x, y, duration: 0 },
      { type: "pointerDown", button: 0 },
      { type: "pointerUp", button: 0 },
    ];
    await this.cmd("POST", "/actions", { actions: [{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions }] });
    const seen = (c, i, n) => {
      const el = document.querySelectorAll(c)[i];
      const clicks = window.__e2e?.clicks ?? [];
      return clicks.length > n && (!el || clicks.slice(n).some((target) => el.contains(target)));
    };
    const landed = await this.wait(() => this.exec(seen, css, index, before), { timeout: 1000, every: 25 }).catch(() => false);
    if (!landed) {
      await this.exec(
        (c, i) => {
          const el = document.querySelectorAll(c)[i];
          el.focus?.();
          el.click();
          if (window.__e2e) window.__e2e.synthetic++;
        },
        css,
        index,
      );
    }
  }


  async type(css, text, index = 0) {
    await this.click(css, index);
    await this.keys(text);
  }

  /** On whatever has the focus; `text` may hold `Key` characters. */
  async keys(text) {
    const actions = [...text].flatMap((value) => [{ type: "keyDown", value }, { type: "keyUp", value }]);
    await this.cmd("POST", "/actions", { actions: [{ type: "key", id: "keyboard", actions }] });
  }

  async press(...keys) {
    const down = keys.map((value) => ({ type: "keyDown", value }));
    const up = keys.toReversed().map((value) => ({ type: "keyUp", value }));
    await this.cmd("POST", "/actions", { actions: [{ type: "key", id: "keyboard", actions: [...down, ...up] }] });
    await this.cmd("DELETE", "/actions");
  }

  text(css) {
    return this.exec((s) => document.querySelector(s)?.innerText ?? null, css);
  }

  async wait(check, { timeout = 15_000, what = "the condition", every = 50 } = {}) {
    const deadline = Date.now() + timeout;
    let last;
    for (;;) {
      this.signal?.throwIfAborted();
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

  /** Even past the scenario's time: the app is asked to close before its process is killed. */
  async close() {
    await call(this.base, "DELETE", `/session/${this.id}`);
  }
}

function visibleOrNull(css) {
  const el = document.querySelector(css);
  return !!el && el.getClientRects().length > 0 && getComputedStyle(el).visibility !== "hidden";
}

async function call(base, method, path, body, signal) {
  const timeout = AbortSignal.timeout(60_000);
  const res = await fetch(base + path, {
    method,
    headers: body ? { "content-type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
    signal: signal ? AbortSignal.any([signal, timeout]) : timeout,
  });
  const json = await res.json().catch(() => ({}));
  if (!res.ok) {
    const v = json.value ?? {};
    throw new Error(`${method} ${path}: ${v.error ?? res.status} ${v.message ?? ""}`.trim());
  }
  return json.value;
}
