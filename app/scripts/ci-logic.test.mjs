import { test } from "node:test";
import assert from "node:assert/strict";
import { branches, cellState, columns, cycle, excerpt, failedJobs, firstFailure, history, matches, origin, outcome } from "../src/lib/ci-logic.ts";

test("going back right after a search starts at the last match", () => {
  assert.equal(cycle(-1, -1, 7), 6);
  assert.equal(cycle(-1, 1, 7), 0);
  assert.equal(cycle(6, 1, 7), 0);
  assert.equal(cycle(0, -1, 7), 6);
  assert.equal(cycle(3, -1, 7), 2);
  assert.equal(cycle(-1, 1, 0), -1);
});

const job = (id, status, conclusion) => ({ id, name: `job ${id}`, status, conclusion, steps: [] });
const detail = (id, jobs) => ({ run: { id }, jobs });

test("failed jobs come from the latest read only", () => {
  const first = failedJobs([detail(1, [job(10, "completed", "failure"), job(11, "completed", "success")])]);
  assert.deepEqual(Object.keys(first), ["1"]);
  assert.deepEqual(first[1].map((j) => j.id), [10]);
  // Rerun: run 1 passes now, run 2 fails with a timeout.
  const next = failedJobs([detail(2, [job(20, "completed", "timed_out"), job(21, "in_progress", null)])]);
  assert.deepEqual(next, { 2: [job(20, "completed", "timed_out")] });
  assert.deepEqual(failedJobs([]), {});
});

test("outcomes", () => {
  assert.equal(outcome({ status: "queued", conclusion: null }), "queued");
  assert.equal(outcome({ status: "in_progress", conclusion: null }), "running");
  assert.equal(outcome({ status: "completed", conclusion: "success" }), "passed");
  assert.equal(outcome({ status: "completed", conclusion: "startup_failure" }), "failed");
  assert.equal(outcome({ status: "completed", conclusion: "cancelled" }), "stopped");
});

const node = (key, needs = [], name = key, partial = false) => ({ key, name, partial, needs });
const j = (id, name, status = "completed", conclusion = "success") => ({ id, name, status, conclusion, steps: [] });

test("the graph puts each job right of what it needs, a matrix in one box", () => {
  const nodes = [node("lint"), node("test", ["lint"]), node("build", ["lint", "test"], "Build"), node("deploy", ["build"])];
  const jobs = [j(1, "lint"), j(2, "test (ubuntu)", "completed", "failure"), j(3, "test (macos)"), j(4, "Build", "completed", "skipped"), j(9, "extra")];
  const cols = columns(nodes, jobs);
  assert.deepEqual(cols.map((c) => c.map((cell) => cell.key)), [["lint", "job:9"], ["test"], ["build"], ["deploy"]]);
  assert.deepEqual(cols[1][0].jobs.map((x) => x.id), [2, 3]);
  assert.equal(cellState(cols[1][0]), "failed");
  assert.equal(cellState(cols[2][0]), "skipped");
  assert.equal(cellState(cols[3][0]), "waiting");
  assert.equal(cellState({ jobs: [j(5, "x", "in_progress", null)] }), "running");
  assert.equal(firstFailure(cols).job.id, 2);
  assert.ok(matches(node("t", [], "test", true), "test ubuntu-latest"));
  assert.ok(!matches(node("t", [], "test"), "tests"));
});

test("a cycle in a broken file still lays out", () => {
  const cols = columns([node("a", ["b"]), node("b", ["a"])], []);
  assert.equal(cols.flat().length, 2);
});

const run = (id, conclusion, minutes, branch = "main", prs = []) => ({
  id,
  status: conclusion ? "completed" : "in_progress",
  conclusion,
  branch,
  pullRequests: prs,
  sha: `sha${id}`,
  created: new Date(Date.UTC(2026, 10, 20, 10, minutes)).toISOString(),
});

test("the squares go oldest to newest, and count the failures in a row", () => {
  const runs = [run(1, "success", 1), run(2, "failure", 2), run(3, "failure", 3), run(4, null, 4)];
  const h = history(runs, 3);
  assert.deepEqual(h.squares.map((r) => r.id), [2, 3, 4]);
  assert.equal(h.failing, 2);
  assert.equal(history([run(1, "success", 1)]).failing, 0);
});

test("branches group their runs, the most recent first, with their pull requests", () => {
  const list = branches([run(1, "success", 1, "main"), run(2, "failure", 5, "feat/x", [57]), run(3, "success", 3, "main")]);
  assert.deepEqual(
    list.map((b) => [b.branch, b.prs, b.runs.map((r) => r.id)]),
    [["feat/x", [57], [2]], ["main", [], [3, 1]]],
  );
});

test("the excerpt shows the first real error, not GitHub's exit line", () => {
  const lines = ["a", "b", "c", "error: boom", "d", "##[error]Process completed with exit code 1."];
  assert.deepEqual(excerpt({ lines, errors: [5, 3] }, 2, 1), { start: 1, lines: ["b", "c", "error: boom", "d"], at: 2 });
  assert.equal(excerpt({ lines: [], errors: [] }), null);
});

test("a deployment finds its pull request and the TermF run behind it", () => {
  const runs = [run(7, "success", 1, "termf/42-fix", [57])];
  const tickets = [{ id: 12, branch: "termf/42-fix", prUrl: "https://github.com/acme/api/pull/57" }];
  assert.deepEqual(origin({ sha: "sha7", branch: null, pr: null }, runs, tickets), { pr: 57, run: 12 });
  assert.deepEqual(origin({ sha: null, branch: null, pr: 57 }, [], tickets), { pr: 57, run: 12 });
  assert.deepEqual(origin({ sha: "zzz", branch: "main", pr: null }, runs, tickets), { pr: null, run: null });
});
