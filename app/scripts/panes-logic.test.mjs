import { test } from "node:test";
import assert from "node:assert/strict";
import { beside, clamp, close, decode, encode, go, leaves, slots } from "../src/lib/panes-logic.ts";

const one = { view: "terminals", other: null, active: 0, size: 0.5 };
const known = ["terminals", "tickets", "files"];

test("one panel: going to a view replaces it", () => {
  assert.deepEqual(go(one, "tickets"), { ...one, view: "tickets" });
  assert.equal(go(one, "terminals"), one);
  assert.deepEqual(slots(one), ["terminals", null]);
});

test("opening beside puts the view on the right and the keyboard there", () => {
  const two = beside(one, "tickets");
  assert.deepEqual(two, { view: "tickets", other: "terminals", active: 1, size: 0.5 });
  assert.deepEqual(slots(two), ["terminals", "tickets"]);
  // The view already shown stays where it is.
  assert.equal(beside(one, "terminals"), one);
  // From the left panel, the right one gets the view; from the right one, the left one.
  const left = { ...two, view: "terminals", other: "tickets", active: 0 };
  assert.deepEqual(slots(beside(left, "files")), ["terminals", "files"]);
  assert.deepEqual(slots(beside(two, "files")), ["files", "tickets"]);
  assert.equal(beside(two, "files").view, "files");
  // Beside the view of the other panel: the keyboard goes there, nothing moves.
  assert.deepEqual(slots(beside(two, "terminals")), ["terminals", "tickets"]);
  assert.equal(beside(two, "terminals").active, 0);
});

test("two panels: the active one changes view, the other's view moves the keyboard there", () => {
  const two = beside(one, "tickets");
  const files = go(two, "files");
  assert.deepEqual(slots(files), ["terminals", "files"]);
  assert.equal(files.active, 1);
  const back = go(two, "terminals");
  assert.deepEqual(slots(back), ["terminals", "tickets"]);
  assert.equal(back.view, "terminals");
  assert.equal(back.active, 0);
});

test("closing a panel leaves the other one alone", () => {
  const two = beside(one, "tickets");
  assert.deepEqual(close(two, "tickets"), { ...one, view: "terminals" });
  assert.deepEqual(close(two, "terminals"), { ...one, view: "tickets" });
  assert.equal(close(two, "files"), two);
  assert.equal(close(one, "terminals"), one);
});

test("the width stays between a quarter and three quarters", () => {
  assert.equal(clamp(0.1), 0.25);
  assert.equal(clamp(0.9), 0.75);
  assert.equal(clamp(0.4), 0.4);
  assert.equal(clamp(Number.NaN), 0.5);
});

test("kept from one launch to the next: two known views, else nothing", () => {
  const two = { ...beside(one, "tickets"), size: 0.61234 };
  const text = encode(two, known);
  assert.deepEqual(decode(text, known), { left: "terminals", right: "tickets", size: 0.612 });
  assert.equal(encode(one, known), null);
  // A page outside the registry (a project's page) is not kept.
  assert.equal(encode(beside(one, "project"), known), null);
  assert.equal(decode(null, known), null);
  assert.equal(decode("{", known), null);
  assert.equal(decode("null", known), null);
  assert.equal(decode(JSON.stringify({ left: "terminals", right: "terminals" }), known), null);
  assert.equal(decode(JSON.stringify({ left: "terminals", right: "gone" }), known), null);
  assert.deepEqual(decode(JSON.stringify({ left: "files", right: "tickets", size: 3 }), known), {
    left: "files",
    right: "tickets",
    size: 0.75,
  });
  assert.equal(decode(JSON.stringify({ left: "files", right: "tickets", size: "x" }), known).size, 0.5);
});

test("a page with unsaved changes is asked about only when a change takes it off screen", () => {
  const page = "project";
  const shown = { view: "project", other: null, active: 0, size: 0.5 };
  // Another view in its panel, or the page closed beside another view: off screen.
  assert.equal(leaves(page, shown, go(shown, "library")), true);
  const two = { view: "project", other: "terminals", active: 0, size: 0.5 };
  assert.equal(leaves(page, two, close(two, "project")), true);
  // The keyboard to the other panel, or another view beside it: still shown.
  assert.equal(leaves(page, two, go(two, "terminals")), false);
  assert.equal(leaves(page, shown, beside(shown, "files")), false);
  assert.equal(leaves(page, two, close(two, "terminals")), false);
  // Beside, replaced by another view opened beside the active one.
  const aside = { view: "terminals", other: "project", active: 0, size: 0.5 };
  assert.equal(leaves(page, aside, beside(aside, "files")), true);
  // Not shown at all: nothing to ask.
  assert.equal(leaves(page, one, go(one, "tickets")), false);
});
