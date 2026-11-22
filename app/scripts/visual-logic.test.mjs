import { test } from "node:test";
import assert from "node:assert/strict";
import { clamp, deck, diffStats, fit, outcome, percent, settles, swipeFirst, swipeKind, toImage, zoomAt } from "../src/lib/visual-logic.ts";

const screen = (id, decision = "pending") => ({ id, decision });
const version = (screens, state = "ready") => ({ state, screens });
const ids = (list) => list.map((s) => s.id).join(" ");

test("the quick mode comes first only when several screens wait", () => {
  assert.equal(swipeFirst(version([screen("a"), screen("b")])), true);
  assert.equal(swipeFirst(version([screen("a"), screen("b", "approved")])), false);
  assert.equal(swipeFirst(version([screen("a"), screen("b")], "done")), false);
  assert.equal(swipeFirst(undefined), false);
});

test("the deck keeps the version's order and puts the screens passed over last", () => {
  const v = version([screen("a"), screen("b", "approved"), screen("c"), screen("d")]);
  assert.equal(ids(deck(v, [])), "a c d");
  assert.equal(ids(deck(v, ["c", "a"])), "d c a");
  assert.equal(ids(deck(v, ["b"])), "a c d", "a decided screen passed over is gone");
});

test("a version settles as the engine does, and only with its last pending screen", () => {
  assert.equal(outcome([screen("a", "approved"), screen("b", "changes")]), "changes");
  assert.equal(outcome([screen("a", "changes"), screen("b", "rejected")]), "rejected");
  assert.equal(outcome([screen("a", "approved")]), "approved");
  const v = version([screen("a", "approved"), screen("b"), screen("c")]);
  assert.equal(settles(v, "b", "rejected"), null);
  const last = version([screen("a", "approved"), screen("b")]);
  assert.equal(settles(last, "b", "rejected"), "rejected");
  assert.equal(settles(last, "b", "approved"), "approved");
});

test("a swipe needs distance or speed, along its main axis", () => {
  assert.equal(swipeKind(200, 10, 0, 0, 800, 600), "approved");
  assert.equal(swipeKind(-200, 10, 0, 0, 800, 600), "rejected");
  assert.equal(swipeKind(10, -140, 0, 0, 800, 600), "changes");
  assert.equal(swipeKind(10, 140, 0, 0, 800, 600), "skip");
  assert.equal(swipeKind(60, 0, 0, 0, 800, 600), null);
  assert.equal(swipeKind(60, 0, 0.8, 0, 800, 600), "approved", "a flick");
  assert.equal(swipeKind(30, 0, 2, 0, 800, 600), null, "too short even when fast");
});

test("zoom keeps the point under the pointer, and clicks map to image pixels", () => {
  const view = fit(700, 500, 1400, 3000);
  assert.deepEqual(view, { scale: 0.5, x: 0, y: 0 });
  assert.deepEqual(fit(700, 500, 1400, 1000, true), { scale: 0.5, x: 0, y: 0 });
  assert.deepEqual(fit(1000, 500, 400, 200), { scale: 1, x: 300, y: 0 }, "never enlarged to fit");
  const zoomed = zoomAt(view, 100, 100, 2);
  assert.equal(zoomed.scale, 1);
  assert.deepEqual(toImage(view, 100, 100, 1400, 3000), toImage(zoomed, 100, 100, 1400, 3000));
  assert.deepEqual(toImage(view, 350, 50, 1400, 3000), { x: 700, y: 100 });
  assert.equal(toImage(view, -1, 50, 1400, 3000), null);
  assert.equal(zoomAt(view, 0, 0, 1000).scale, 8);
});

test("panning never leaves an empty band, and a small image stays centered", () => {
  assert.deepEqual(clamp({ scale: 1, x: 50, y: 30 }, 700, 500, 1400, 3000), { scale: 1, x: 0, y: 0 });
  assert.deepEqual(clamp({ scale: 1, x: -2000, y: -9000 }, 700, 500, 1400, 3000), { scale: 1, x: -700, y: -2500 });
  assert.deepEqual(clamp({ scale: 0.1, x: 0, y: 0 }, 700, 500, 1400, 3000), { scale: 0.1, x: 280, y: 0 });
  assert.deepEqual(clamp({ scale: 0.1, x: 0, y: 0 }, 700, 500, 1400, 3000, true), { scale: 0.1, x: 280, y: 100 });
});

test("a diff counts its lines, not its headers; a tiny change never reads zero", () => {
  const diff = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,2 +1,3 @@\n a\n-b\n+c\n+d\n";
  assert.deepEqual(diffStats(diff), { added: 2, removed: 1 });
  assert.equal(percent(0.00001, "en"), "0.1%");
  assert.equal(percent(0.25, "en"), "25%");
  assert.equal(percent(0.034, "en"), "3.4%");
});
