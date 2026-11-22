// The second transport, for the app's `e2e` build (src-tauri/src/e2e.rs): the same interface as the WebDriver
// session, but each command is JavaScript the real window runs. Input is made in the page: pointer, mouse and key
// events as a browser fires them, the text through `execCommand("insertText")`, and the default actions a script's
// events do not get (submitting a form on Enter, closing a dialog on Escape) done as the browser would.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Session } from "./webdriver.mjs";

export class Bridge extends Session {
  /** `base`: the bridge's address; `token`: the one the app was started with. */
  constructor(base, token, signal) {
    super(base, token, signal);
  }

  async request(method, path, body, signal = this.signal) {
    const timeout = AbortSignal.timeout(60_000);
    const res = await fetch(this.base + path, {
      method,
      headers: { "x-termf-e2e-token": this.id },
      body,
      signal: signal ? AbortSignal.any([signal, timeout]) : timeout,
    });
    const json = await res.json().catch(() => null);
    if (!res.ok) throw new Error(`${method} ${path}: ${res.status} ${JSON.stringify(json)}`);
    return json;
  }

  async exec(fn, ...args) {
    const reply = await this.request("POST", "/eval", `(${fn}).apply(null, ${JSON.stringify(args)})`);
    if (!reply.ok) throw new Error(`in the page: ${reply.value}`);
    return reply.value ?? null;
  }

  async click(css, index = 0) {
    const { x, y } = await this.point(css, index);
    await this.exec(clickAt, css, index, x, y);
  }

  keys(text) {
    return this.exec(typeKeys, [...text], false);
  }

  press(...keys) {
    return this.exec(typeKeys, keys, true);
  }

  /** The window as drawn, where the system lets `screencapture` read it (macOS, with the screen recording right). */
  async screenshot() {
    const id = await this.request("GET", "/status", undefined, null);
    if (process.platform !== "darwin" || !id) throw new Error("no screenshot of the window here: see page.html");
    const dir = mkdtempSync(join(tmpdir(), "termf-e2e-shot-"));
    try {
      execFileSync("screencapture", ["-x", "-o", `-l${id}`, join(dir, "w.png")], { stdio: "ignore" });
      return readFileSync(join(dir, "w.png"));
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }

  source() {
    return this.exec(() => document.documentElement.outerHTML);
  }

  /** The app quits as from its menu, so its agents and teams shut down as they would. */
  async close() {
    await this.request("POST", "/quit", "", null);
  }
}

function clickAt(css, index, x, y) {
  const el = document.querySelectorAll(css)[index];
  const top = document.elementFromPoint(x, y);
  const target = top && el.contains(top) ? top : el;
  const at = { bubbles: true, cancelable: true, composed: true, clientX: x, clientY: y, button: 0, view: window, detail: 1 };
  const pointer = (type, buttons) =>
    target.dispatchEvent(new PointerEvent(type, { ...at, buttons, pointerId: 1, pointerType: "mouse", isPrimary: true }));
  pointer("pointerdown", 1);
  // Focus is mousedown's default action: a cancelled pointerdown does not stop it.
  if (target.dispatchEvent(new MouseEvent("mousedown", { ...at, buttons: 1 }))) {
    const focusable = target.closest("input, textarea, select, button, a[href], [tabindex], [contenteditable=''], [contenteditable=true]");
    if (focusable) focusable.focus();
    else if (document.activeElement instanceof HTMLElement && !document.activeElement.contains(target)) document.activeElement.blur();
  }
  pointer("pointerup", 0);
  target.dispatchEvent(new MouseEvent("mouseup", at));
  target.dispatchEvent(new MouseEvent("click", at));
}

/** `keys`: characters and WebDriver key codes; `chord`: modifiers held over the other keys, else each key alone. */
async function typeKeys(keys, chord) {
  const named = {
    "": ["Backspace", 8],
    "": ["Tab", 9],
    "": ["Enter", 13],
    "": ["Escape", 27],
    "": [" ", 32, "Space"],
    "": ["End", 35],
    "": ["Home", 36],
    "": ["ArrowLeft", 37],
    "": ["ArrowUp", 38],
    "": ["ArrowRight", 39],
    "": ["ArrowDown", 40],
    "": ["Delete", 46],
  };
  const modifiers = { "": "shiftKey", "": "ctrlKey", "": "altKey", "": "metaKey" };
  const held = {};
  if (chord) for (const k of keys) if (modifiers[k]) held[modifiers[k]] = true;
  const fire = (type, key, code, keyCode, charCode = 0) => {
    const e = new KeyboardEvent(type, { key, code, bubbles: true, cancelable: true, composed: true, ...held });
    for (const [name, value] of Object.entries({ keyCode, charCode, which: charCode || keyCode })) {
      Object.defineProperty(e, name, { get: () => value });
    }
    (document.activeElement ?? document.body).dispatchEvent(e);
    return !e.defaultPrevented;
  };
  // HTML's implicit submission: the form's default button is clicked (nothing if it is disabled); without one, the
  // form is submitted only when it has a single field that blocks it. Never from a checkbox, radio, range or colour.
  const blocking = ["text", "search", "url", "tel", "email", "password", "date", "month", "week", "time", "datetime-local", "number"];
  const submitImplicitly = (input) => {
    const form = input.form;
    if (!form || !blocking.includes(input.type)) return;
    const submitter = [...form.elements].find((e) => (e instanceof HTMLButtonElement || e instanceof HTMLInputElement) && ["submit", "image"].includes(e.type));
    if (submitter) {
      if (!submitter.disabled) submitter.click();
    } else if ([...form.elements].filter((e) => e instanceof HTMLInputElement && blocking.includes(e.type)).length === 1) {
      form.requestSubmit();
    }
  };
  const command = held.ctrlKey || held.metaKey || held.altKey;
  for (const k of keys) {
    if (modifiers[k]) continue;
    // One key per task, as from a keyboard: the page updates (a submit button enabled by the text) in between.
    await new Promise((r) => setTimeout(r));
    const el = document.activeElement;
    if (named[k]) {
      const [key, keyCode, code = key] = named[k];
      if (fire("keydown", key, code, keyCode) && !command) {
        if (key === "Enter" && fire("keypress", key, code, keyCode, 13)) {
          if (el instanceof HTMLInputElement) submitImplicitly(el);
          else if (el instanceof HTMLButtonElement) el.click();
          else document.execCommand("insertLineBreak");
        } else if (key === "Escape") {
          // As the browser does: only a modal dialog closes on Escape, the one on top, after its cancellable `cancel`.
          const modals = [...document.querySelectorAll("dialog:modal")];
          const dialog = modals.find((d) => d.contains(el)) ?? modals.at(-1);
          if (dialog && dialog.dispatchEvent(new Event("cancel", { cancelable: true }))) dialog.close();
        } else if (key === "Backspace") document.execCommand("delete");
        else if (key === "Delete") document.execCommand("forwardDelete");
        else if (key === " " && fire("keypress", key, code, keyCode, 32)) {
          if (el instanceof HTMLButtonElement) el.click();
          else document.execCommand("insertText", false, " ");
        }
      }
      fire("keyup", key, code, keyCode);
      continue;
    }
    const letter = /^[a-z]$/i.test(k);
    const keyCode = letter ? k.toUpperCase().charCodeAt(0) : /^[0-9]$/.test(k) ? k.charCodeAt(0) : 0;
    const code = letter ? `Key${k.toUpperCase()}` : /^[0-9]$/.test(k) ? `Digit${k}` : "";
    const key = held.shiftKey && letter ? k.toUpperCase() : k;
    if (fire("keydown", key, code, keyCode) && !command && fire("keypress", key, code, keyCode, key.charCodeAt(0))) {
      document.execCommand("insertText", false, key);
    }
    fire("keyup", key, code, keyCode);
  }
}
