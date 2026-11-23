import { test } from "node:test";
import assert from "node:assert/strict";
import {
  changeKind,
  chosen,
  cleanupContext,
  joinMessage,
  keyOf,
  lanes,
  localFor,
  matchPlan,
  nextStep,
  onRemote,
  path,
  pickable,
  picks,
  planOf,
  prFor,
  runs,
  samePath,
  sides,
  sorted,
  span,
  splitMessage,
  tally,
  uncommitted,
  within,
} from "../src/lib/git-logic.ts";

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

test("changes split into conflicts, staged and the work tree's", () => {
  const ch = (path, index, tree, conflict = false) => ({ path, from: null, index, tree, conflict });
  const { conflicts, staged, unstaged } = sides([
    ch("both.rs", "M", "M"),
    ch("staged.rs", "A", "."),
    ch("new.md", "?", "?"),
    ch("fight.rs", "U", "U", true),
    ch("README.md", "D", "."),
  ]);
  assert.deepEqual(conflicts.map((c) => c.path), ["fight.rs"]);
  assert.deepEqual(staged.map((c) => c.path), ["both.rs", "staged.rs", "README.md"]);
  assert.deepEqual(unstaged.map((c) => c.path), ["both.rs", "new.md"], "a file staged and changed again comes twice");
});

test("picked lines become one pick per hunk, in order", () => {
  assert.deepEqual(picks(["1:4", "0:3", "1:2", "0:1"]), [
    { hunk: 0, lines: [1, 3] },
    { hunk: 1, lines: [2, 4] },
  ]);
  assert.deepEqual(picks([]), []);
  const lines = [{ kind: " " }, { kind: "-" }, { kind: "+" }, { kind: " " }, { kind: "+" }];
  assert.deepEqual(span(2, lines, 4, 0), ["2:1", "2:2", "2:4"], "a range keeps the changed lines only");
});

test("a blame groups the lines of one commit", () => {
  const r = runs([{ oid: "a" }, { oid: "a" }, { oid: "b" }, { oid: "a" }]);
  assert.deepEqual(r, [
    { oid: "a", start: 0, count: 2 },
    { oid: "b", start: 2, count: 1 },
    { oid: "a", start: 3, count: 1 },
  ]);
  assert.ok(uncommitted("0000000000000000000000000000000000000000"));
  assert.ok(!uncommitted("a0"));
});

test("a message splits into a subject and a body, and back", () => {
  assert.deepEqual(splitMessage("Add the API\n\nFirst version.\nWith tests.\n"), {
    subject: "Add the API",
    body: "First version.\nWith tests.",
  });
  assert.deepEqual(splitMessage("One line"), { subject: "One line", body: "" });
  assert.equal(joinMessage(" Fix ", ""), "Fix");
  assert.equal(joinMessage("Fix", "Because.\n"), "Fix\n\nBecause.");
});

test("a worktree's folder relative to the repository's", () => {
  assert.equal(within("/w/acme", "/w/acme"), "");
  assert.equal(within("/w/acme/", "/w/acme/.claude/worktrees/run-7"), ".claude/worktrees/run-7");
  assert.equal(within("C:\\w\\acme", "C:/w/acme/wt"), "wt");
  assert.equal(within("/w/acme", "/w/acme-other"), null);
});

const tip1 = { oid: "a1", author: "Alex", date: 1_700_000_000, subject: "Fix" };
const br = (name, o = {}) => ({ name, tip: tip1, reasons: [], lost: 0, run: null, blocked: null, suggested: false, ...o });
const wt = (path, o = {}) => ({ path, branch: null, head: "h", date: 0, subject: "", reasons: [], lost: 0, changes: 0, run: null, closeRun: false, blocked: null, suggested: false, ...o });
const mess = {
  defaultBranch: "origin/main",
  branches: [br("merged", { reasons: ["merged"], suggested: true }), br("wip", { lost: 3 }), br("run-8", { blocked: "runOpen", lost: 1 })],
  worktrees: [wt("/w/run-6", { branch: "termf/run-6", lost: 2, reasons: ["prMerged", "run"], suggested: true }), wt("/w/dirty", { changes: 4 })],
};

test("a cleanup selection: worktrees first, what it loses, blocked items never in it", () => {
  const c = chosen(mess, new Set(["b:merged", "b:wip", "b:run-8", "w:/w/dirty", "w:/w/run-6"]));
  assert.deepEqual(c.worktrees.map(keyOf), ["w:/w/run-6", "w:/w/dirty"]);
  assert.deepEqual(c.branches.map(keyOf), ["b:merged", "b:wip"]);
  assert.deepEqual([c.lost, c.changes, c.count], [5, 4, 4]);
  assert.deepEqual(pickable(mess), { all: ["w:/w/run-6", "w:/w/dirty", "b:merged", "b:wip"], suggested: ["w:/w/run-6", "b:merged"] });
});

test("a cleanup proposed by the expert, matched against what can go", () => {
  const text = "repo /w/acme\n- branch merged\nbranch `wip`\nworktree /w/run-6/\nbranch run-8\nbranch nope\nrm -rf /\nbranch merged";
  const plan = planOf(text);
  assert.equal(plan.repo, "/w/acme");
  assert.deepEqual(plan.keys, ["b:merged", "b:wip", "w:/w/run-6/", "b:run-8", "b:nope"]);
  // A blocked branch and an unknown one are not picked.
  assert.deepEqual(matchPlan(mess, plan.keys), { keys: ["b:merged", "b:wip", "w:/w/run-6"], unknown: 2 });
  assert.deepEqual(planOf("nothing here"), { repo: null, keys: [] });
});

test("the Git view's one next step, the most pressing first", () => {
  const f = { operation: false, behind: 0, changed: 0, ahead: 0, suggested: 0 };
  assert.equal(nextStep(f), null);
  assert.equal(nextStep({ ...f, suggested: 2 }), "cleanup");
  assert.equal(nextStep({ ...f, suggested: 2, ahead: 1 }), "push");
  assert.equal(nextStep({ ...f, ahead: 1, changed: 3 }), "commit");
  assert.equal(nextStep({ ...f, changed: 3, behind: 2 }), "pull");
  assert.equal(nextStep({ ...f, behind: 2, operation: true }), null);
});

test("the expert reads what can go, what each loses, the history and how to propose a cleanup", () => {
  const text = cleanupContext("/w/acme", { current: "main", defaultBranch: "origin/main" }, mess, [{ oid: "abcdef12", author: "Bo", date: 1_700_000_000, subject: "Ship it" }]);
  assert.match(text, /- merged: merged · 0 commit\(s\) only here · last commit 2023-11-14 "Fix"/);
  assert.match(text, /- wip: in progress · 3 commit\(s\) only here/);
  assert.match(text, /- run-8: .* cannot go: its run is still open/);
  assert.match(text, /- \/w\/run-6: branch termf\/run-6 goes with it · pull request merged, TermF run over · 0 uncommitted change\(s\) · 2 commit\(s\) lost/);
  assert.match(text, /- \/w\/dirty: its branch stays · in progress · 4 uncommitted/);
  assert.match(text, /- abcdef1 Ship it \(Bo, 2023-11-14\)/);
  assert.match(text, /language is termf-cleanup/);
  assert.doesNotMatch(cleanupContext("/w/acme", { current: null, defaultBranch: null }, null, []), /Local branches/);
});
