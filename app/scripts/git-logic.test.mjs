import { test } from "node:test";
import assert from "node:assert/strict";
import { changeKind, lanes, localFor, onRemote, path, prFor, samePath, sorted, tally } from "../src/lib/git-logic.ts";

const c = (oid, ...parents) => ({ oid, parents });
const strokes = (row) => row.lines.map((l) => `${l.kind}:${l.from}>${l.to}`).sort();

test("a straight history stays in one lane", () => {
  const rows = lanes([c("c", "b"), c("b", "a"), c("a")]);
  assert.deepEqual(rows.map((r) => r.lane), [0, 0, 0]);
  assert.deepEqual(strokes(rows[0]), ["out:0>0"], "the tip has nothing above");
  assert.deepEqual(strokes(rows[1]), ["in:0>0", "out:0>0"]);
  assert.deepEqual(strokes(rows[2]), ["in:0>0"], "the root has nothing below");
  assert.deepEqual(rows.map((r) => r.width), [1, 1, 1]);
});

test("a merge opens a lane for its other parent, which joins back at the fork", () => {
  const rows = lanes([c("m", "a", "f"), c("a", "b"), c("f", "b"), c("b")]);
  assert.deepEqual(rows.map((r) => r.lane), [0, 0, 1, 0]);
  assert.deepEqual(strokes(rows[0]), ["out:0>0", "out:0>1"]);
  assert.deepEqual(strokes(rows[1]), ["in:0>0", "out:0>0", "through:1>1"]);
  assert.deepEqual(strokes(rows[2]), ["in:1>1", "out:1>1", "through:0>0"]);
  assert.deepEqual(strokes(rows[3]), ["in:0>0", "in:1>0"]);
  assert.equal(rows[0].color, rows[1].color, "the first parent keeps its child's color");
  assert.notEqual(rows[2].color, rows[0].color, "the merged branch has its own");
  assert.equal(rows[3].width, 2);
});

test("a merge into a lane already waiting for that parent draws into it", () => {
  // `x` waits for `b` in lane 1 when `m` merges `b` too.
  const rows = lanes([c("x", "b"), c("m", "a", "b"), c("a", "b"), c("b")]);
  assert.deepEqual(rows.map((r) => r.lane), [0, 1, 1, 0]);
  assert.deepEqual(strokes(rows[1]), ["out:1>0", "out:1>1", "through:0>0"]);
  assert.deepEqual(strokes(rows[3]), ["in:0>0", "in:1>0"]);
});

test("a new tip takes the first free lane, and lanes free at the end are dropped", () => {
  const rows = lanes([c("t1", "a"), c("t2", "b"), c("a", "b"), c("b"), c("t3", "z")]);
  assert.deepEqual(rows.map((r) => r.lane), [0, 1, 0, 0, 0]);
  assert.deepEqual(strokes(rows[3]), ["in:0>0", "in:1>0"]);
  assert.equal(rows[4].width, 1, "lane 1 freed at b is gone");
  assert.deepEqual(strokes(rows[4]), ["out:0>0"], "a parent never read keeps its lane to the end");
});

test("colors come in turn and wrap", () => {
  const rows = lanes([c("a"), c("b"), c("c")], 2);
  assert.deepEqual(rows.map((r) => r.color), [0, 1, 0]);
});

test("strokes become SVG paths", () => {
  assert.equal(path({ kind: "through", from: 1, to: 1, color: 0 }, 14, 28), "M21 0V28");
  assert.equal(path({ kind: "in", from: 0, to: 0, color: 0 }, 14, 28), "M7 0V14");
  assert.equal(path({ kind: "out", from: 0, to: 0, color: 0 }, 14, 28), "M7 14V28");
  assert.equal(path({ kind: "out", from: 0, to: 2, color: 0 }, 14, 28), "M7 14C7 28 35 14 35 28");
  assert.equal(path({ kind: "in", from: 1, to: 0, color: 0 }, 14, 28), "M21 0C21 14 7 0 7 14");
});

const tip = (date) => ({ oid: `o${date}`, author: "alex", date, subject: "s" });
const branch = (name, extra = {}) => ({
  name,
  remote: null,
  current: false,
  upstream: null,
  gone: false,
  ahead: 0,
  behind: 0,
  tip: tip(1),
  merged: null,
  worktree: null,
  ...extra,
});
const pr = (number, branch, state = "OPEN") => ({ number, title: "t", state, draft: false, url: `u${number}`, branch });

test("a branch's pull request goes by its upstream's name, an open one first", () => {
  const prs = [pr(9, "feature/csv", "CLOSED"), pr(7, "feature/csv"), pr(3, "main", "MERGED"), pr(2, "fix")];
  const remotes = ["origin", "team/fork"];
  assert.equal(prFor(branch("csv", { upstream: "origin/feature/csv" }), prs, remotes)?.number, 7);
  assert.equal(prFor(branch("fix"), prs, remotes)?.number, 2, "no upstream: its own name");
  assert.equal(prFor(branch("team/fork/fix", { remote: "team/fork" }), prs, remotes)?.number, 2);
  assert.equal(prFor(branch("main"), prs, remotes)?.number, 3, "no open one: the newest");
  assert.equal(prFor(branch("other"), prs, remotes), undefined);
});

test("remote names, local twins and order", () => {
  const remote = branch("origin/feature/x", { remote: "origin" });
  assert.equal(onRemote(remote), "feature/x");
  assert.equal(onRemote(branch("main")), "main");
  const all = [
    branch("old", { tip: tip(5) }),
    branch("x", { upstream: "origin/feature/x", tip: tip(9) }),
    branch("main", { current: true, tip: tip(1) }),
    remote,
    branch("origin/main", { remote: "origin", tip: tip(7) }),
  ];
  assert.equal(localFor(remote, all)?.name, "x");
  assert.equal(localFor(all[4], all), undefined);
  const { local, remote: remotes } = sorted(all);
  assert.deepEqual(local.map((b) => b.name), ["main", "x", "old"], "current first, then the most recent");
  assert.deepEqual(remotes.map((b) => b.name), ["origin/main", "origin/feature/x"]);
});

test("what happened to a file", () => {
  const ch = (index, tree, extra = {}) => ({ path: "p", from: null, index, tree, conflict: false, ...extra });
  assert.equal(changeKind(ch(".", "M")), "modified");
  assert.equal(changeKind(ch("M", "M")), "modified");
  assert.equal(changeKind(ch("A", ".")), "added");
  assert.equal(changeKind(ch(".", "D")), "deleted");
  assert.equal(changeKind(ch("R", "M", { from: "old" })), "renamed");
  assert.equal(changeKind(ch("?", "?")), "untracked");
  assert.equal(changeKind(ch("U", "U", { conflict: true })), "conflict");
  assert.deepEqual(tally([ch(".", "M"), ch("U", "U", { conflict: true }), ch("?", "?")], 4), { changed: 6, conflicts: 1 });
  assert.deepEqual(tally([]), { changed: 0, conflicts: 0 });
});

test("paths compare whatever the slashes", () => {
  assert.ok(samePath("/w/acme/.claude/worktrees/termf-1", "/w/acme/.claude/worktrees/termf-1/"));
  assert.ok(samePath("C:/w/acme", "C:\\w\\acme\\"));
  assert.ok(!samePath("/w/acme", "/w/acme-api"));
});
