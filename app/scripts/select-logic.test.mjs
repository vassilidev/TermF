import { test } from "node:test";
import assert from "node:assert/strict";
import { all, none, pick, within } from "../src/lib/select-logic.ts";

const shown = ["a", "b", "c", "d", "e"];

test("a click toggles one item and anchors the next range there", () => {
  let p = pick(none(), shown, "b", false);
  assert.deepEqual(p, { keys: ["b"], anchor: "b" });
  p = pick(p, shown, "d", false);
  assert.deepEqual(p, { keys: ["b", "d"], anchor: "d" });
  p = pick(p, shown, "b", false);
  assert.deepEqual(p, { keys: ["d"], anchor: "b" });
});

test("Shift adds everything from the anchor, either way, in the list's order", () => {
  const p = pick(pick(none(), shown, "d", false), shown, "b", true);
  assert.deepEqual(p, { keys: ["b", "c", "d"], anchor: "d" });
  assert.deepEqual(pick(p, shown, "e", true).keys, ["b", "c", "d", "e"]);
  // Without an anchor, Shift picks one.
  assert.deepEqual(pick(none(), shown, "c", true), { keys: ["c"], anchor: "c" });
  // An anchor no longer shown starts a new range.
  assert.deepEqual(pick({ keys: [], anchor: "z" }, shown, "c", true), { keys: ["c"], anchor: "c" });
});

test("only what is shown stays picked", () => {
  assert.deepEqual(within(["e", "x", "a"], shown), ["a", "e"]);
  // Picking after a filter drops what the filter hid.
  assert.deepEqual(pick({ keys: ["x", "a"], anchor: "a" }, shown, "b", false).keys, ["a", "b"]);
  // A key not shown changes nothing but the hidden ones.
  assert.deepEqual(pick({ keys: ["x", "a"], anchor: "a" }, shown, "z", false), { keys: ["a"], anchor: "a" });
});

test("select all, then none", () => {
  const every = all(pick(none(), shown, "b", false), shown);
  assert.deepEqual(every.keys, shown);
  assert.deepEqual(all(every, shown).keys, []);
  assert.deepEqual(all(none(), []).keys, []);
});

test("works with numbers", () => {
  assert.deepEqual(pick(pick(none(), [3, 1, 2], 3, false), [3, 1, 2], 2, true).keys, [3, 1, 2]);
});
