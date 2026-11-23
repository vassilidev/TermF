// No imports but types, so `pnpm test` runs it in plain Node.
import type { t } from "$lib/i18n";

type Label = Parameters<typeof t>[0];

/** Where a shortcut works: everywhere in the app, in one view, or system-wide (even with TermF in the background). */
export type Scope = "app" | "terminals" | "files" | "global";
export type KeyGroup = "general" | "terminals" | "files" | "tickets";
export type KeyAction = {
  id: string;
  label: Label;
  help?: Label;
  group: KeyGroup;
  scope: Scope;
  /** Combinations as saved in `[keys]` of termf.toml; "" is none. */
  defaults: { mac: string; other: string };
  /** Followed by 1 to 9, so only the modifiers are set. */
  digits?: boolean;
  /** A character that also asks for it, typed outside text fields and terminals; not editable. */
  typed?: string;
  /** Matched on the character typed, as browsers zoom: layouts move - and = (and + needs Shift on many). */
  byCharacter?: boolean;
};

/** Every shortcut of the app. A key press handled anywhere else is a bug. */
export const actions: KeyAction[] = [
  { id: "palette", label: "palette", group: "general", scope: "app", defaults: { mac: "Cmd+K", other: "Ctrl+K" } },
  { id: "help", label: "t6bHelp", help: "t6bHelpTyped", group: "general", scope: "app", typed: "?", defaults: { mac: "", other: "" } },
  {
    id: "switch_project",
    label: "navSwitchProject",
    group: "general",
    scope: "app",
    defaults: { mac: "Cmd+O", other: "Ctrl+O" },
  },
  { id: "talk_cto", label: "ctoTalk", group: "general", scope: "app", defaults: { mac: "Cmd+Shift+H", other: "Ctrl+Shift+H" } },
  {
    id: "project",
    label: "settingsKeysProject",
    group: "general",
    scope: "app",
    digits: true,
    defaults: { mac: "Ctrl", other: "Alt" },
  },
  // Ctrl+`, not ⌘`, which macOS uses to cycle windows; in the app only, so VS Code keeps it in the background.
  {
    id: "quick_terminal",
    label: "quickTerminal",
    help: "settingsKeysQuickHelp",
    group: "general",
    scope: "app",
    defaults: { mac: "Ctrl+`", other: "Ctrl+`" },
  },
  {
    id: "split_view",
    label: "t6bSplitView",
    group: "general",
    scope: "app",
    defaults: { mac: "Cmd+\\", other: "Ctrl+Shift+\\" },
  },
  {
    id: "other_panel",
    label: "t6bOtherPanel",
    group: "general",
    scope: "app",
    defaults: { mac: "Alt+Cmd+\\", other: "Alt+Shift+\\" },
  },
  {
    id: "quick_terminal_global",
    label: "settingsKeysQuickGlobal",
    help: "settingsKeysQuickGlobalHelp",
    group: "general",
    scope: "global",
    defaults: { mac: "", other: "" },
  },
  { id: "toggle_nav", label: "wsToggleNav", group: "general", scope: "app", defaults: { mac: "Cmd+B", other: "Ctrl+B" } },
  {
    id: "next_waiting",
    label: "nextWaiting",
    group: "terminals",
    scope: "app",
    defaults: { mac: "Cmd+J", other: "Ctrl+J" },
  },
  {
    id: "new_session",
    label: "navNewSession",
    group: "terminals",
    scope: "app",
    defaults: { mac: "Cmd+T", other: "Ctrl+Shift+T" },
  },
  // Ctrl+Shift+T is the new session outside macOS, where Ctrl+T stays the shell's.
  { id: "reopen_tab", label: "wsReopenTab", group: "terminals", scope: "app", defaults: { mac: "Cmd+Shift+T", other: "Alt+Shift+T" } },
  {
    id: "tile",
    label: "t6aTab",
    group: "terminals",
    scope: "app",
    digits: true,
    defaults: { mac: "Cmd", other: "Ctrl" },
  },
  {
    id: "prev_tab",
    label: "t6aPrevTab",
    group: "terminals",
    scope: "terminals",
    defaults: { mac: "Cmd+Shift+[", other: "Ctrl+Shift+Tab" },
  },
  {
    id: "next_tab",
    label: "t6aNextTab",
    group: "terminals",
    scope: "terminals",
    defaults: { mac: "Cmd+Shift+]", other: "Ctrl+Tab" },
  },
  {
    id: "split_right",
    label: "t6aSplitRight",
    group: "terminals",
    scope: "terminals",
    defaults: { mac: "Cmd+D", other: "Ctrl+Shift+D" },
  },
  {
    id: "split_down",
    label: "t6aSplitDown",
    group: "terminals",
    scope: "terminals",
    defaults: { mac: "Cmd+Shift+D", other: "Ctrl+Shift+E" },
  },
  {
    id: "close_split",
    label: "t6aCloseSplit",
    group: "terminals",
    scope: "terminals",
    defaults: { mac: "Cmd+W", other: "Ctrl+Shift+W" },
  },
  // Ctrl+Alt is AltGr outside macOS: Alt+arrows there, as in Windows Terminal.
  { id: "focus_left", label: "t6aFocusLeft", group: "terminals", scope: "terminals", defaults: { mac: "Alt+Cmd+Left", other: "Alt+Left" } },
  { id: "focus_right", label: "t6aFocusRight", group: "terminals", scope: "terminals", defaults: { mac: "Alt+Cmd+Right", other: "Alt+Right" } },
  { id: "focus_up", label: "t6aFocusUp", group: "terminals", scope: "terminals", defaults: { mac: "Alt+Cmd+Up", other: "Alt+Up" } },
  { id: "focus_down", label: "t6aFocusDown", group: "terminals", scope: "terminals", defaults: { mac: "Alt+Cmd+Down", other: "Alt+Down" } },
  { id: "resize_left", label: "t6aResizeLeft", group: "terminals", scope: "terminals", defaults: { mac: "Ctrl+Cmd+Left", other: "Alt+Shift+Left" } },
  { id: "resize_right", label: "t6aResizeRight", group: "terminals", scope: "terminals", defaults: { mac: "Ctrl+Cmd+Right", other: "Alt+Shift+Right" } },
  { id: "resize_up", label: "t6aResizeUp", group: "terminals", scope: "terminals", defaults: { mac: "Ctrl+Cmd+Up", other: "Alt+Shift+Up" } },
  { id: "resize_down", label: "t6aResizeDown", group: "terminals", scope: "terminals", defaults: { mac: "Ctrl+Cmd+Down", other: "Alt+Shift+Down" } },
  {
    id: "equalize_splits",
    label: "t6aEqualize",
    group: "terminals",
    scope: "terminals",
    defaults: { mac: "Ctrl+Cmd+=", other: "Ctrl+Shift+=" },
  },
  {
    id: "maximize",
    label: "maximizeActive",
    group: "terminals",
    scope: "terminals",
    defaults: { mac: "Cmd+Enter", other: "Ctrl+Enter" },
  },
  {
    id: "broadcast",
    label: "settingsKeysBroadcast",
    group: "terminals",
    scope: "terminals",
    defaults: { mac: "Cmd+Shift+Enter", other: "Ctrl+Shift+Enter" },
  },
  // After the splits' Ctrl+Shift+= outside macOS: the focused terminal's text, else the whole interface.
  { id: "zoom_in", label: "wsZoomIn", group: "general", scope: "app", byCharacter: true, defaults: { mac: "Cmd+=", other: "Ctrl+=" } },
  { id: "zoom_out", label: "wsZoomOut", group: "general", scope: "app", byCharacter: true, defaults: { mac: "Cmd+-", other: "Ctrl+-" } },
  { id: "zoom_reset", label: "wsZoomReset", group: "general", scope: "app", defaults: { mac: "Cmd+0", other: "Ctrl+0" } },
  { id: "find_file", label: "filesFind", group: "files", scope: "files", defaults: { mac: "Cmd+P", other: "Ctrl+P" } },
  {
    id: "search_project",
    label: "filesSearch",
    group: "files",
    scope: "files",
    defaults: { mac: "Cmd+Shift+F", other: "Ctrl+Shift+F" },
  },
  {
    id: "launch_ticket",
    label: "ticketLaunchLong",
    group: "tickets",
    scope: "app",
    defaults: { mac: "Cmd+Shift+L", other: "Ctrl+Shift+L" },
  },
];

export const action = (id: string) => actions.find((a) => a.id === id);

export type Combo = { ctrl: boolean; alt: boolean; cmd: boolean; shift: boolean; key: string };

const MODS = { ctrl: "Ctrl", alt: "Alt", cmd: "Cmd", shift: "Shift" } as const;
const ALIASES: Record<string, keyof typeof MODS> = {
  ctrl: "ctrl",
  control: "ctrl",
  alt: "alt",
  option: "alt",
  cmd: "cmd",
  command: "cmd",
  super: "cmd",
  meta: "cmd",
  shift: "shift",
};
const NAMED = ["Enter", "Space", "Tab", "Backspace", "Delete", "Escape", "Up", "Down", "Left", "Right", "Home", "End", "PageUp", "PageDown", "Insert"];
const CODES: Record<string, string> = {
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  NumpadEnter: "Enter",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  ...Object.fromEntries(NAMED.map((k) => [k, k])),
};
const isFn = (key: string) => /^F([1-9]|1[0-2])$/.test(key);
const isLetter = (key: string) => /^[A-Z]$/.test(key);
const isDigit = (key: string) => /^[0-9]$/.test(key);

/** A saved combination, as written in `[keys]` (by TermF or by hand); null when it cannot be one. */
export function parse(text: string): Combo | null {
  const tokens = text.split("+").map((s) => s.trim());
  if (!text.trim() || tokens.some((s) => !s)) return null;
  const combo: Combo = { ctrl: false, alt: false, cmd: false, shift: false, key: "" };
  for (const token of tokens) {
    const mod = ALIASES[token.toLowerCase()];
    if (mod && !combo.key) combo[mod] = true;
    else if (combo.key) return null;
    else {
      const upper = token.toUpperCase();
      const named = NAMED.find((k) => k.toUpperCase() === upper);
      if (/^[A-Z0-9]$/.test(upper) || isFn(upper)) combo.key = upper;
      else if (named) combo.key = named;
      else if (Object.values(CODES).includes(token)) combo.key = token;
      else return null;
    }
  }
  return combo;
}

/** As saved in `[keys]`, and as the system-wide shortcut takes it: `Ctrl+Alt+Cmd+Shift+K`. */
export function format(c: Combo): string {
  const mods = (Object.keys(MODS) as (keyof typeof MODS)[]).filter((m) => c[m]).map((m) => MODS[m]);
  return [...mods, ...(c.key ? [c.key] : [])].join("+");
}

/**
 * The key of a press: letters as the layout types them (AZERTY's A is A), digits and symbols by their place.
 * `physical`: letters by their place too, as the system registers a system-wide shortcut.
 */
export function keyOf(e: { key: string; code: string }, physical = false): string {
  if (/^Digit[0-9]$/.test(e.code)) return e.code.slice(5);
  if (!physical && /^[a-z]$/i.test(e.key)) return e.key.toUpperCase();
  if (/^Key[A-Z]$/.test(e.code)) return e.code.slice(3);
  if (isFn(e.code)) return e.code;
  return CODES[e.code] ?? "";
}

type Press = { key: string; code: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean };

/** The combination pressed; null for a modifier alone. */
export function pressedCombo(e: Press, physical = false): Combo | null {
  const key = keyOf(e, physical);
  return key ? { ctrl: e.ctrlKey, alt: e.altKey, cmd: e.metaKey, shift: e.shiftKey, key } : null;
}

/** - or = as typed (+ counts as =), whichever key the layout puts them on; null for any other character. */
export const typedSymbol = (e: { key: string }): "=" | "-" | null =>
  e.key === "=" || e.key === "+" ? "=" : e.key === "-" ? "-" : null;

/** What the shortcut editor records for `a`: the symbol typed for an action matched on the character, else the key. */
export function recordedCombo(e: Press, a: KeyAction): Combo | null {
  const symbol = a.byCharacter ? typedSymbol(e) : null;
  if (!symbol) return pressedCombo(e, a.scope === "global");
  return { ctrl: e.ctrlKey, alt: e.altKey, cmd: e.metaKey, shift: e.shiftKey && e.key !== "+", key: symbol };
}

const GLYPHS: Record<string, string> = { Enter: "↵", Up: "↑", Down: "↓", Left: "←", Right: "→", Backspace: "⌫", Delete: "⌦", Tab: "⇥" };

/** One label per key, as shown in `<kbd>`: `⌘` `⇧` `L` on macOS, `Ctrl` `Shift` `L` elsewhere. */
export function tokens(c: Combo, mac: boolean, digit = ""): string[] {
  const mods = mac
    ? [c.ctrl && "⌃", c.alt && "⌥", c.cmd && "⌘", c.shift && "⇧"]
    : [c.ctrl && "Ctrl", c.alt && "Alt", c.cmd && "Super", c.shift && "Shift"];
  const key = digit || c.key;
  return [...mods.filter((m): m is string => !!m), ...(key ? [mac ? (GLYPHS[key] ?? key) : key] : [])];
}

export function display(c: Combo | null, mac: boolean, digit = ""): string {
  return c ? tokens(c, mac, digit).join(mac ? "" : "+") : "";
}

/** For `aria-keyshortcuts`. */
export function aria(c: Combo | null, digit = ""): string | undefined {
  if (!c) return undefined;
  const names: Record<string, string> = { Up: "ArrowUp", Down: "ArrowDown", Left: "ArrowLeft", Right: "ArrowRight" };
  const key = digit || c.key;
  const mods = [c.ctrl && "Control", c.alt && "Alt", c.cmd && "Meta", c.shift && "Shift"].filter(Boolean);
  return [...mods, names[key] ?? key].join("+");
}

/** The combination of each action: its default for this system unless `overrides` changes it. */
export function bindings(overrides: Record<string, string>, mac: boolean): Record<string, string> {
  // Written by hand, as the system-wide shortcut also takes it.
  const main = (combo: string) => combo.replace(/\b(?:cmd|command)or(?:ctrl|control)\b/gi, mac ? "Cmd" : "Ctrl");
  return Object.fromEntries(
    actions.map((a) => [a.id, a.id in overrides ? main(overrides[a.id]) : mac ? a.defaults.mac : a.defaults.other]),
  );
}

/** `overrides` with `id` set to `combo`; only what differs from the default is kept. Unknown ids stay as they are. */
export function setBinding(overrides: Record<string, string>, id: string, combo: string, mac: boolean) {
  const a = action(id);
  const fallback = a ? (mac ? a.defaults.mac : a.defaults.other) : null;
  const next = { ...overrides };
  if (combo === fallback) delete next[id];
  else next[id] = combo;
  return next;
}

/**
 * On Linux and Windows, Ctrl or Alt with a letter, and Alt with a digit, are readline's in a terminal: there the
 * action needs Shift as well. Shift is then accepted outside the terminals too, so the same keys work everywhere.
 */
export function shellKeys(c: Combo, mac: boolean): boolean {
  return !mac && !c.shift && !c.cmd && (c.ctrl || c.alt) && (isLetter(c.key) || (c.alt && isDigit(c.key)));
}

/** Only the actions a terminal can have the focus for take Shift as well. */
const reachable = (a: KeyAction) => a.scope === "app" || a.scope === "terminals";

/** The full combinations an action takes, so two actions can be compared. */
function claims(a: KeyAction, combo: string, mac: boolean): string[] {
  const c = parse(combo);
  if (!c || (!a.digits && !c.key)) return [];
  const keys = a.digits ? ["1", "2", "3", "4", "5", "6", "7", "8", "9"] : [c.key];
  return keys.flatMap((key) => {
    const one = { ...c, key };
    return reachable(a) && shellKeys(one, mac) ? [format(one), format({ ...one, shift: true })] : [format(one)];
  });
}

const overlap = (a: Scope, b: Scope) => a === b || [a, b].some((s) => s === "app" || s === "global");
// The same panel inside and outside the app: one combination for both is fine.
const QUICK = ["quick_terminal", "quick_terminal_global"];

/** The other action that already takes `combo` where `id` works, if any. */
export function conflict(id: string, combo: string, current: Record<string, string>, mac: boolean): string | null {
  const self = action(id);
  if (!self) return null;
  const mine = new Set(claims(self, combo, mac));
  for (const other of actions) {
    if (other.id === id || !overlap(self.scope, other.scope)) continue;
    if (QUICK.includes(id) && QUICK.includes(other.id)) continue;
    if (claims(other, current[other.id] ?? "", mac).some((c) => mine.has(c))) return other.id;
  }
  return null;
}

/** Why a press cannot become `a`'s shortcut, as an i18n key; null when it can. */
export function refusal(a: KeyAction, c: Combo, mac: boolean): Label | null {
  if (a.digits && !/^[1-9]$/.test(c.key)) return "settingsKeysNeedsDigit";
  if (!c.ctrl && !c.alt && !c.cmd && !isFn(c.key)) return "settingsKeysNeedsModifier";
  const only = (ctrl: boolean, alt: boolean, cmd: boolean) => c.ctrl === ctrl && c.alt === alt && c.cmd === cmd;
  if (mac && only(false, false, true)) {
    if (["Tab", "Space", "`"].includes(c.key) || (!c.shift && ["Q", "H", "M", ","].includes(c.key))) {
      return "settingsKeysReservedSystem";
    }
    if ((!c.shift && ["C", "V", "X", "A", "Z"].includes(c.key)) || (c.shift && c.key === "Z")) return "settingsKeysReservedEdit";
  }
  if (!mac) {
    // AltGr arrives as Ctrl+Alt on Windows and types characters.
    if (c.ctrl && c.alt) return "settingsKeysReservedAltGr";
    if (only(false, true, false) && !c.shift && c.key === "F4") return "settingsKeysReservedSystem";
    if (only(true, false, false) && !c.shift) {
      if (["C", "D", "Z", "L"].includes(c.key)) return "settingsKeysReservedTerminal";
      if (["V", "X", "A"].includes(c.key)) return "settingsKeysReservedEdit";
    }
  }
  return null;
}

export type Hit = { id: string; n?: number };
export type Context = {
  mac: boolean;
  /** The focus is in a terminal. */
  inTerminal: boolean;
  /** The focus is where text is typed: a field, an editor. */
  inField?: boolean;
  view: string;
  /** How many tiles or projects a digit can reach: a digit with nothing behind it is left to the terminal. */
  limits?: Record<string, number>;
};

/** The action a key press asks for in the app, null to leave the key alone. */
export function match(e: Press, current: Record<string, string>, ctx: Context): Hit | null {
  if (!e.ctrlKey && !e.metaKey && !e.altKey && !ctx.inField && !ctx.inTerminal) {
    const typed = actions.find((a) => a.typed === e.key && (a.scope === "app" || a.scope === ctx.view));
    if (typed) return { id: typed.id };
  }
  const got = pressedCombo(e);
  const symbol = typedSymbol(e);
  for (const a of actions) {
    if (a.scope === "global" || (a.scope !== "app" && a.scope !== ctx.view)) continue;
    const want = parse(current[a.id] ?? "");
    if (!want) continue;
    if (a.byCharacter && (want.key === "=" || want.key === "-")) {
      // The symbol typed, + needing Shift on many layouts; a key typing neither (Ctrl+_ is readline's undo), by its place.
      const hit = symbol
        ? symbol === want.key && (e.key === "+" || e.shiftKey === want.shift)
        : got?.key === want.key && got.shift === want.shift;
      if (hit && e.ctrlKey === want.ctrl && e.altKey === want.alt && e.metaKey === want.cmd) return { id: a.id };
      continue;
    }
    if (!got) continue;
    if (a.digits ? !/^[1-9]$/.test(got.key) : want.key !== got.key) continue;
    want.key = got.key;
    if (got.ctrl !== want.ctrl || got.alt !== want.alt || got.cmd !== want.cmd) continue;
    const shell = reachable(a) && shellKeys(want, ctx.mac);
    if (shell ? ctx.inTerminal && !got.shift : got.shift !== want.shift) continue;
    const n = a.digits ? Number(got.key) : undefined;
    if (n !== undefined && n > (ctx.limits?.[a.id] ?? 9)) continue;
    return n === undefined ? { id: a.id } : { id: a.id, n };
  }
  return null;
}

export const groups: [KeyGroup, Label][] = [
  ["general", "settingsKeysGeneral"],
  ["terminals", "viewTerminals"],
  ["files", "viewFiles"],
  ["tickets", "viewTickets"],
];

export function shortcut(a: KeyAction, combo: string, mac: boolean): string[] {
  return [display(parse(combo), mac, a.digits ? "1…9" : ""), a.typed ?? ""].filter(Boolean);
}

export type Sheet = { group: KeyGroup; label: Label; rows: { id: string; label: Label; keys: string[] }[] }[];

export function sheet(current: Record<string, string>, mac: boolean, view: string): Sheet {
  const first = groups.filter(([g]) => g === view);
  return [...first, ...groups.filter(([g]) => g !== view)]
    .map(([group, label]) => ({
      group,
      label,
      rows: actions
        .filter((a) => a.group === group)
        .map((a) => ({ id: a.id, label: a.label, keys: shortcut(a, current[a.id] ?? "", mac) }))
        .filter((row) => row.keys.length),
    }))
    .filter((g) => g.rows.length);
}
