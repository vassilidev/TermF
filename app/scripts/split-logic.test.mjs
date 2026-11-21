import { test } from "node:test";
import assert from "node:assert/strict";
import {
  decode,
  dividers,
  encode,
  equalize,
  focus,
  grid,
  insert,
  leaves,
  move,
  neighbour,
  rects,
  relabel,
  remove,
  resize,
  resizeAt,
} from "../src/lib/split-logic.ts";

const close = (a, b) => assert.ok(Math.abs(a - b) < 1e-9, `${a} ≠ ${b}`);
const sizes = (p, expected) => p.sizes.forEach((s, i) => close(s, expected[i]));

// 1 | 2
//   | 3
const three = () => insert(insert(1, 1, 2, "right"), 2, 3, "down");

test("splitting a pane gives the new one half of its room, on the side asked", () => {
  assert.deepEqual(insert(1, 1, 2, "right"), { dir: "row", sizes: [0.5, 0.5], children: [1, 2] });
  assert.deepEqual(insert(1, 1, 2, "up"), { dir: "col", sizes: [0.5, 0.5], children: [2, 1] });
  assert.deepEqual(three(), {
    dir: "row",
    sizes: [0.5, 0.5],
    children: [1, { dir: "col", sizes: [0.5, 0.5], children: [2, 3] }],
  });
  // The same direction adds a sibling instead of nesting.
  const row = insert(insert(1, 1, 2, "right"), 1, 3, "right");
  assert.deepEqual(row.children, [1, 3, 2]);
  sizes(row, [0.25, 0.25, 0.5]);
  assert.deepEqual(leaves(three()), [1, 2, 3]);
  assert.equal(insert(1, 9, 2, "right"), 1, "an unknown pane changes nothing");
});

test("closing a pane gives its room back and never leaves a split of one", () => {
  assert.equal(remove(1, 1), null);
  assert.deepEqual(remove(three(), 3), { dir: "row", sizes: [0.5, 0.5], children: [1, 2] });
  assert.deepEqual(remove(remove(three(), 3), 1), 2);
  // A nested split of the same direction as its new parent merges into it.
  const nested = { dir: "row", sizes: [0.5, 0.5], children: [1, { dir: "col", sizes: [0.5, 0.5], children: [2, { dir: "row", sizes: [0.5, 0.5], children: [3, 4] }] }] };
  const merged = remove(nested, 2);
  assert.deepEqual(merged.children, [1, 3, 4]);
  sizes(merged, [0.5, 0.25, 0.25]);
  assert.deepEqual(relabel(three(), (id) => (id === 2 ? null : id * 10)), { dir: "row", sizes: [0.5, 0.5], children: [10, 30] });
});

test("where each pane sits, and the borders between them", () => {
  const r = rects(three());
  assert.deepEqual(r.get(1), { x: 0, y: 0, w: 0.5, h: 1 });
  assert.deepEqual(r.get(3), { x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
  const d = dividers(three());
  assert.equal(d.length, 2);
  assert.deepEqual(d[0], { path: [], i: 0, dir: "row", at: 0.5, box: { x: 0, y: 0, w: 1, h: 1 } });
  assert.deepEqual(d[1], { path: [1], i: 0, dir: "col", at: 0.5, box: { x: 0.5, y: 0, w: 0.5, h: 1 } });
  assert.deepEqual(rects(7).get(7), { x: 0, y: 0, w: 1, h: 1 });
});

test("moving between panes goes to the one sharing the most border, never past the edge", () => {
  const t = three();
  assert.equal(neighbour(t, 1, "right"), 2);
  assert.equal(neighbour(t, 3, "left"), 1);
  assert.equal(neighbour(t, 2, "down"), 3);
  assert.equal(neighbour(t, 3, "up"), 2);
  assert.equal(neighbour(t, 1, "left"), null);
  assert.equal(neighbour(t, 2, "right"), null);
  assert.equal(neighbour(t, 9, "right"), null);
  // 2 over a wide 1: from 1 upwards, the pane above it.
  const below = { dir: "col", sizes: [0.3, 0.7], children: [{ dir: "row", sizes: [0.2, 0.8], children: [2, 4] }, 1] };
  assert.equal(neighbour(below, 1, "up"), 4);
});

test("resizing by mouse or keyboard keeps every pane at a tenth of its split at least", () => {
  sizes(resizeAt(three(), [], 0, 0.2), [0.7, 0.3]);
  sizes(resizeAt(three(), [], 0, 0.9), [0.9, 0.1]);
  sizes(resizeAt(three(), [1], 0, -0.1).children[1], [0.4, 0.6]);
  // The right border of 1 moves right: it grows.
  sizes(resize(three(), 1, "right", 0.1), [0.6, 0.4]);
  // 2 has no border on its right: its left one moves right, so it shrinks.
  sizes(resize(three(), 2, "right", 0.1), [0.6, 0.4]);
  sizes(resize(three(), 2, "left", 0.1), [0.4, 0.6]);
  sizes(resize(three(), 3, "up", 0.1).children[1], [0.4, 0.6]);
  // In a half-width split, a tenth of the tab is a fifth of the split.
  sizes(resize(insert(three(), 3, 4, "right"), 3, "right", 0.1).children[1].children[1], [0.7, 0.3]);
  assert.deepEqual(resize(three(), 1, "up", 0.1), three(), "no border that way");
  assert.deepEqual(resize(5, 5, "left", 0.1), 5);
});

test("layouts: even sizes, grids and focus", () => {
  const lopsided = resizeAt(resizeAt(three(), [], 0, 0.3), [1], 0, 0.2);
  assert.deepEqual(equalize(lopsided), three());
  assert.equal(grid([]), null);
  assert.equal(grid([4]), 4);
  assert.deepEqual(grid([1, 2], 2), { dir: "row", sizes: [0.5, 0.5], children: [1, 2] });
  const auto = grid([1, 2, 3, 4, 5]);
  assert.equal(auto.dir, "col");
  assert.deepEqual(auto.children.map((r) => (typeof r === "number" ? r : r.children)), [[1, 2, 3], [4, 5]]);
  assert.deepEqual(grid([1, 2, 3], 2).children[1], 3);
  assert.deepEqual(focus([1, 2, 3], 2), { dir: "row", sizes: [0.7, 0.3], children: [2, { dir: "col", sizes: [0.5, 0.5], children: [1, 3] }] });
  assert.deepEqual(focus([1, 2], 2), { dir: "row", sizes: [0.7, 0.3], children: [2, 1] });
  assert.equal(focus([1], 1), 1);
});

test("a saved tree reads back over the same sessions, and anything else is refused", () => {
  const t = three();
  const text = encode(t, [3, 1, 2]);
  assert.deepEqual(decode(text, 3), { dir: "row", sizes: [0.5, 0.5], children: [1, { dir: "col", sizes: [0.5, 0.5], children: [2, 0] }] });
  assert.equal(decode("2", 3), 2);
  assert.deepEqual(decode('{"dir":"col","sizes":[2,6],"children":[0,1]}', 2).sizes, [0.25, 0.75]);
  for (const bad of [
    "",
    "nope",
    "3",
    "-1",
    "1.5",
    "null",
    '{"dir":"row","sizes":[1],"children":[0]}',
    '{"dir":"diagonal","sizes":[1,1],"children":[0,1]}',
    '{"dir":"row","sizes":[1,0],"children":[0,1]}',
    '{"dir":"row","sizes":[1,1],"children":[0,0]}',
    '{"dir":"row","sizes":[1,1,1],"children":[0,1]}',
  ]) {
    assert.equal(decode(bad, 2), null, bad);
  }
});

test("tabs move by dragging", () => {
  assert.deepEqual(move(["a", "b", "c"], 0, 2), ["b", "c", "a"]);
  assert.deepEqual(move(["a", "b", "c"], 2, 0), ["c", "a", "b"]);
});
