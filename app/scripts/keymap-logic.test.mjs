import { test } from "node:test";
import assert from "node:assert/strict";
import {
  actions,
  aria,
  bindings,
  conflict,
  display,
  format,
  match,
  parse,
  pressedCombo,
  refusal,
  setBinding,
} from "../src/lib/keymap-logic.ts";

const key = (key, code, mods = {}) => ({ key, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods });
const nine = { tile: 9, project: 9 };
const mac = bindings({}, true);
const other = bindings({}, false);
const byId = (id) => actions.find((a) => a.id === id);

test("every action has its own id, and defaults written as they are saved", () => {
  assert.equal(new Set(actions.map((a) => a.id)).size, actions.length);
  for (const a of actions) {
    for (const combo of [a.defaults.mac, a.defaults.other].filter(Boolean)) {
      assert.equal(format(parse(combo)), combo, `${a.id}: ${combo}`);
    }
  }
});

test("no two defaults clash, on macOS or elsewhere", () => {
  for (const [current, isMac] of [
    [mac, true],
    [other, false],
  ]) {
    for (const a of actions) {
      assert.equal(conflict(a.id, current[a.id], current, isMac), null, `${a.id} on ${isMac ? "macOS" : "Linux"}`);
    }
  }
});

test("combinations read from termf.toml, by TermF or by hand", () => {
  assert.deepEqual(parse("Cmd+Shift+L"), { ctrl: false, alt: false, cmd: true, shift: true, key: "L" });
  assert.equal(format(parse("shift+command+l")), "Cmd+Shift+L");
  assert.equal(format(parse("Control+Option+Super+enter")), "Ctrl+Alt+Cmd+Enter");
  assert.equal(format(parse("Ctrl+`")), "Ctrl+`");
  assert.equal(format(parse("Alt+Shift+Space")), "Alt+Shift+Space");
  assert.equal(format(parse("Cmd")), "Cmd");
  for (const bad of ["", "Ctrl+", "Ctrl+Nope", "K+Ctrl", "Ctrl+K+J"]) assert.equal(parse(bad), null, bad);
});

test("how a combination is shown and announced", () => {
  assert.equal(display(parse("Cmd+Shift+L"), true), "⌘⇧L");
  assert.equal(display(parse("Cmd+Enter"), true), "⌘↵");
  assert.equal(display(parse("Ctrl+`"), true), "⌃`");
  assert.equal(display(parse("Ctrl"), true, "3"), "⌃3");
  assert.equal(display(parse("Ctrl+Shift+T"), false), "Ctrl+Shift+T");
  assert.equal(display(parse("Alt"), false, "1…9"), "Alt+1…9");
  assert.equal(display(null, true), "");
  assert.equal(aria(parse("Cmd+K")), "Meta+K");
  assert.equal(aria(parse("Ctrl+Shift+Up")), "Control+Shift+ArrowUp");
  assert.equal(aria(parse("Alt"), "2"), "Alt+2");
});

test("a key press becomes a combination: letters as typed, digits and symbols by their place", () => {
  assert.equal(format(pressedCombo(key("k", "KeyK", { metaKey: true }))), "Cmd+K");
  // AZERTY: the A key sits where QWERTY has Q.
  assert.equal(format(pressedCombo(key("a", "KeyQ", { ctrlKey: true }))), "Ctrl+A");
  // Option on macOS types a symbol: the place of the key says which letter.
  assert.equal(format(pressedCombo(key("˚", "KeyK", { altKey: true }))), "Alt+K");
  assert.equal(format(pressedCombo(key("&", "Digit1", { metaKey: true }))), "Cmd+1");
  assert.equal(format(pressedCombo(key("`", "Backquote", { ctrlKey: true }))), "Ctrl+`");
  assert.equal(format(pressedCombo(key("F5", "F5"))), "F5");
  assert.equal(pressedCombo(key("Meta", "MetaLeft", { metaKey: true })), null);
  // A system-wide shortcut is registered by place: AZERTY's A is QWERTY's Q.
  assert.equal(format(pressedCombo(key("a", "KeyQ", { ctrlKey: true, altKey: true }), true)), "Ctrl+Alt+Q");
});

test("CmdOrCtrl written by hand is ⌘ on macOS, Ctrl elsewhere", () => {
  assert.equal(bindings({ palette: "CmdOrCtrl+Shift+P" }, true).palette, "Cmd+Shift+P");
  assert.equal(bindings({ palette: "commandorcontrol+Shift+P" }, false).palette, "Ctrl+Shift+P");
});

test("shortcuts on macOS", () => {
  const press = (k, inTerminal = false, view = "terminals") => match(k, mac, { mac: true, inTerminal, view, limits: nine });
  assert.deepEqual(press(key("1", "Digit1", { metaKey: true })), { id: "tile", n: 1 });
  assert.deepEqual(press(key("3", "Digit3", { ctrlKey: true }), true), { id: "project", n: 3 });
  // AZERTY: the digit row types & é " without Shift; the code still says which digit.
  assert.deepEqual(press(key("&", "Digit1", { ctrlKey: true })), { id: "project", n: 1 });
  assert.equal(press(key("0", "Digit0", { ctrlKey: true })), null);
  assert.deepEqual(press(key("o", "KeyO", { metaKey: true }), true), { id: "switch_project" });
  assert.deepEqual(press(key("t", "KeyT", { metaKey: true })), { id: "new_session" });
  assert.equal(press(key("t", "KeyT", { metaKey: true, shiftKey: true })), null);
  assert.deepEqual(press(key("k", "KeyK", { metaKey: true }), true), { id: "palette" });
  assert.equal(press(key("k", "KeyK", { ctrlKey: true })), null);
  assert.equal(press(key("1", "Digit1", { metaKey: true, altKey: true })), null);
  assert.deepEqual(press(key("`", "Backquote", { ctrlKey: true }), true), { id: "quick_terminal" });
  assert.deepEqual(press(key("Enter", "Enter", { metaKey: true, shiftKey: true })), { id: "broadcast" });
});

test("view shortcuts only in their view", () => {
  const press = (k, view) => match(k, mac, { mac: true, inTerminal: false, view, limits: nine });
  assert.deepEqual(press(key("Enter", "Enter", { metaKey: true }), "terminals"), { id: "maximize" });
  assert.equal(press(key("Enter", "Enter", { metaKey: true }), "files"), null);
  assert.deepEqual(press(key("p", "KeyP", { metaKey: true }), "files"), { id: "find_file" });
  assert.deepEqual(press(key("f", "KeyF", { metaKey: true, shiftKey: true }), "files"), { id: "search_project" });
  assert.equal(press(key("p", "KeyP", { metaKey: true }), "terminals"), null);
});

test("a digit with no tile or project behind it is left to the terminal", () => {
  const press = (k, isMac) =>
    match(k, isMac ? mac : other, { mac: isMac, inTerminal: true, view: "terminals", limits: { tile: 4, project: 3 } });
  assert.deepEqual(press(key("4", "Digit4", { metaKey: true }), true), { id: "tile", n: 4 });
  assert.equal(press(key("5", "Digit5", { metaKey: true }), true), null);
  assert.deepEqual(press(key("3", "Digit3", { ctrlKey: true }), true), { id: "project", n: 3 });
  assert.equal(press(key("4", "Digit4", { ctrlKey: true }), true), null);
  assert.equal(press(key("4", "Digit4", { altKey: true }), false), null);
  assert.equal(match(key("1", "Digit1", { ctrlKey: true }), other, { mac: false, inTerminal: false, view: "terminals", limits: { tile: 0, project: 0 } }), null);
});

test("shortcuts on Linux and Windows", () => {
  const press = (k, inTerminal = false) => match(k, other, { mac: false, inTerminal, view: "terminals", limits: nine });
  assert.deepEqual(press(key("1", "Digit1", { ctrlKey: true })), { id: "tile", n: 1 });
  assert.deepEqual(press(key("2", "Digit2", { altKey: true })), { id: "project", n: 2 });
  // AltGr is Ctrl+Alt on Windows: it types characters, it is no shortcut.
  assert.equal(press(key("@", "Digit0", { ctrlKey: true, altKey: true })), null);
  assert.equal(press(key("2", "Digit2", { ctrlKey: true, altKey: true })), null);
  assert.deepEqual(press(key("T", "KeyT", { ctrlKey: true, shiftKey: true }), true), { id: "new_session" });
  assert.equal(press(key("t", "KeyT", { ctrlKey: true })), null);
  // Ctrl+K, J, O and Alt+digit (readline's digit argument) belong to the shell in a terminal, unless Shift is held.
  assert.equal(press(key("o", "KeyO", { ctrlKey: true }), true), null);
  assert.deepEqual(press(key("O", "KeyO", { ctrlKey: true, shiftKey: true }), true), { id: "switch_project" });
  assert.deepEqual(press(key("o", "KeyO", { ctrlKey: true })), { id: "switch_project" });
  assert.deepEqual(press(key("O", "KeyO", { ctrlKey: true, shiftKey: true })), { id: "switch_project" });
  assert.equal(press(key("j", "KeyJ", { ctrlKey: true }), true), null);
  assert.equal(press(key("2", "Digit2", { altKey: true }), true), null);
  assert.deepEqual(press(key("@", "Digit2", { altKey: true, shiftKey: true }), true), { id: "project", n: 2 });
  assert.deepEqual(press(key("l", "KeyL", { ctrlKey: true, shiftKey: true }), true), { id: "launch_ticket" });
  // No terminal in Files: Shift is not taken there.
  const files = { mac: false, inTerminal: false, view: "files", limits: nine };
  assert.deepEqual(match(key("p", "KeyP", { ctrlKey: true }), other, files), { id: "find_file" });
  assert.equal(match(key("P", "KeyP", { ctrlKey: true, shiftKey: true }), other, files), null);
  // The rule follows the combination, not the action: a palette moved to Ctrl+Y needs Shift in a terminal too.
  const moved = bindings(setBinding({}, "palette", "Ctrl+Y", false), false);
  const ctx = { mac: false, inTerminal: true, view: "terminals" };
  assert.equal(match(key("y", "KeyY", { ctrlKey: true }), moved, ctx), null);
  assert.deepEqual(match(key("Y", "KeyY", { ctrlKey: true, shiftKey: true }), moved, ctx), { id: "palette" });
});

test("a changed shortcut is used, a removed one is gone", () => {
  const changed = bindings(setBinding(setBinding({}, "maximize", "Cmd+Shift+M", true), "tile", "", true), true);
  const ctx = { mac: true, inTerminal: false, view: "terminals", limits: nine };
  assert.deepEqual(match(key("m", "KeyM", { metaKey: true, shiftKey: true }), changed, ctx), { id: "maximize" });
  assert.equal(match(key("Enter", "Enter", { metaKey: true }), changed, ctx), null);
  assert.equal(match(key("1", "Digit1", { metaKey: true }), changed, ctx), null);
  const digits = bindings(setBinding({}, "tile", "Ctrl+Shift", false), false);
  assert.deepEqual(match(key("!", "Digit1", { ctrlKey: true, shiftKey: true }), digits, { ...ctx, mac: false }), {
    id: "tile",
    n: 1,
  });
  assert.equal(match(key("1", "Digit1", { ctrlKey: true }), digits, { ...ctx, mac: false }), null);
});

test("conflicts: same combination where both actions work", () => {
  assert.equal(conflict("switch_project", "Cmd+J", mac, true), "next_waiting");
  // A 1-to-9 action takes every digit.
  assert.equal(conflict("palette", "Cmd+4", mac, true), "tile");
  assert.equal(conflict("project", "Cmd", mac, true), "tile");
  // Elsewhere, Alt+digit also takes Alt+Shift+digit, which the terminals need.
  assert.equal(conflict("tile", "Alt+Shift", other, false), "project");
  // Files and Terminals never show at once.
  assert.equal(conflict("find_file", "Cmd+Enter", mac, true), null);
  assert.equal(conflict("find_file", "Cmd+K", mac, true), "palette");
  // System-wide covers the app too, but the quick terminal may use the same keys in and out of it.
  assert.equal(conflict("quick_terminal_global", "Cmd+K", mac, true), "palette");
  assert.equal(conflict("quick_terminal_global", "Ctrl+`", mac, true), null);
  // Elsewhere, Ctrl+K also takes Ctrl+Shift+K (the terminal's way to reach it).
  assert.equal(conflict("launch_ticket", "Ctrl+Shift+K", other, false), "palette");
  assert.equal(conflict("launch_ticket", "Ctrl+Shift+K", mac, true), null);
  // …but Files has no terminal: its Ctrl+P does not take Ctrl+Shift+P.
  assert.equal(conflict("search_project", "Ctrl+Shift+P", other, false), null);
  // A removed shortcut clashes with nothing.
  assert.equal(conflict("maximize", "", mac, true), null);
  const removed = bindings(setBinding({}, "palette", "", true), true);
  assert.equal(conflict("switch_project", "Cmd+K", removed, true), null);
});

test("refused combinations say why", () => {
  const why = (id, combo, isMac) => refusal(byId(id), parse(combo), isMac);
  assert.equal(why("palette", "K", true), "settingsKeysNeedsModifier");
  assert.equal(why("palette", "Shift+K", false), "settingsKeysNeedsModifier");
  assert.equal(why("palette", "F6", true), null);
  assert.equal(why("tile", "Cmd+K", true), "settingsKeysNeedsDigit");
  assert.equal(why("tile", "Alt+Shift+1", false), null);
  for (const combo of ["Cmd+Q", "Cmd+W", "Cmd+H", "Cmd+M", "Cmd+,", "Cmd+`", "Cmd+Space", "Cmd+Tab"]) {
    assert.equal(why("palette", combo, true), "settingsKeysReservedSystem", combo);
  }
  for (const combo of ["Cmd+C", "Cmd+V", "Cmd+X", "Cmd+A", "Cmd+Z", "Cmd+Shift+Z"]) {
    assert.equal(why("palette", combo, true), "settingsKeysReservedEdit", combo);
  }
  assert.equal(why("palette", "Cmd+Shift+C", true), null);
  assert.equal(why("palette", "Ctrl+C", true), null);
  for (const combo of ["Ctrl+C", "Ctrl+D", "Ctrl+Z", "Ctrl+L"]) {
    assert.equal(why("palette", combo, false), "settingsKeysReservedTerminal", combo);
  }
  assert.equal(why("palette", "Ctrl+V", false), "settingsKeysReservedEdit");
  assert.equal(why("palette", "Ctrl+Alt+K", false), "settingsKeysReservedAltGr");
  assert.equal(why("palette", "Alt+F4", false), "settingsKeysReservedSystem");
  assert.equal(why("palette", "Ctrl+Shift+C", false), null);
  assert.equal(why("quick_terminal_global", "Alt+Shift+Space", false), null);
});

test("[keys] keeps only what differs from the defaults, and ids it does not know", () => {
  let overrides = { someday: "Cmd+Y" };
  overrides = setBinding(overrides, "maximize", "Cmd+Shift+M", true);
  overrides = setBinding(overrides, "quick_terminal_global", "Alt+Shift+Space", true);
  overrides = setBinding(overrides, "tile", "", true);
  assert.deepEqual(overrides, {
    someday: "Cmd+Y",
    maximize: "Cmd+Shift+M",
    quick_terminal_global: "Alt+Shift+Space",
    tile: "",
  });
  overrides = setBinding(overrides, "maximize", "Cmd+Enter", true);
  overrides = setBinding(overrides, "quick_terminal_global", "", true);
  assert.deepEqual(overrides, { someday: "Cmd+Y", tile: "" });
  // The same override is a change on one system and the default on the other.
  assert.deepEqual(setBinding({}, "palette", "Ctrl+K", true), { palette: "Ctrl+K" });
  assert.deepEqual(setBinding({}, "palette", "Ctrl+K", false), {});
  const current = bindings({ someday: "Cmd+Y", palette: "Cmd+P" }, true);
  assert.equal(current.palette, "Cmd+P");
  assert.equal(current.next_waiting, "Cmd+J");
  assert.equal(current.someday, undefined);
});
