import { test } from "node:test";
import assert from "node:assert/strict";
import { NAV, dragNav, fontStep, navColumn, pushRecent, readNav, shellPaths, toggleNav, zoomStep } from "../src/lib/workspace-logic.ts";

test("the navigation kept reads back, anything else gives the default", () => {
  assert.deepEqual(readNav(null), { width: 232, mode: "full", fold: "rail" });
  assert.deepEqual(readNav("{not json"), { width: 232, mode: "full", fold: "rail" });
  assert.deepEqual(readNav('{"width":300,"mode":"rail","fold":"hidden"}'), { width: 300, mode: "rail", fold: "hidden" });
  assert.deepEqual(readNav('{"width":9000,"mode":"wide"}'), { width: NAV.max, mode: "full", fold: "rail" });
  assert.equal(readNav('{"width":"x"}').width, NAV.width);
});

test("⌘B folds to icons, or hides when that is the option, and comes back", () => {
  const full = readNav(null);
  assert.equal(toggleNav(full).mode, "rail");
  assert.equal(toggleNav(toggleNav(full)).mode, "full");
  assert.equal(toggleNav({ ...full, fold: "hidden" }).mode, "hidden");
  assert.equal(toggleNav({ ...full, mode: "hidden" }).mode, "full");
});

test("dragging the edge sizes it within bounds, and folds it under the snap", () => {
  const full = readNav(null);
  assert.deepEqual(dragNav(full, 300), { ...full, width: 300 });
  assert.equal(dragNav(full, 150).width, NAV.min);
  assert.equal(dragNav(full, 2000).width, NAV.max);
  assert.deepEqual(dragNav(full, 80), { ...full, mode: "rail" });
  assert.equal(dragNav({ ...full, mode: "rail" }, 260).mode, "full");
});

test("the column: the width, icons when folded or the window narrow, nothing when hidden", () => {
  const nav = { width: 280, mode: "full", fold: "rail" };
  assert.equal(navColumn(nav, false), 280);
  assert.equal(navColumn(nav, true), NAV.rail);
  assert.equal(navColumn({ ...nav, mode: "rail" }, false), NAV.rail);
  assert.equal(navColumn({ ...nav, mode: "hidden" }, true), 0);
});

test("zoom steps like a browser, and stops at the ends", () => {
  assert.equal(zoomStep(1, 1), 1.1);
  assert.equal(zoomStep(1, -1), 0.9);
  assert.equal(zoomStep(1.25, 0), 1);
  assert.equal(zoomStep(2, 1), 2);
  assert.equal(zoomStep(0.67, -1), 0.67);
  assert.equal(zoomStep(1.05, 1), 1.1, "between two levels, the next one up");
  assert.equal(fontStep(13, 1), 14);
  assert.equal(fontStep(8, -1), 8);
  assert.equal(fontStep(20, 0), 13);
});

test("dropped paths are typed as a shell reads them", () => {
  assert.equal(shellPaths(["/Users/alex/My Files/a (1).png"], false), "/Users/alex/My\\ Files/a\\ \\(1\\).png ");
  assert.equal(shellPaths(["/tmp/it's", "/tmp/été.txt"], false), "/tmp/it\\'s /tmp/été.txt ");
  assert.equal(shellPaths(["/tmp/$HOME;rm"], false), "/tmp/\\$HOME\\;rm ");
  assert.equal(shellPaths(["/tmp/a\nb"], false), "'/tmp/a\nb' ");
  assert.equal(shellPaths(["C:\\Users\\Alex\\My Files\\a.txt", "C:\\b.txt"], true), '"C:\\Users\\Alex\\My Files\\a.txt" C:\\b.txt ');
  assert.equal(shellPaths([], false), "");
});

test("the recently closed keep the ten newest", () => {
  let list = [];
  for (let i = 0; i < 12; i++) list = pushRecent(list, i);
  assert.deepEqual(list, [11, 10, 9, 8, 7, 6, 5, 4, 3, 2]);
});
