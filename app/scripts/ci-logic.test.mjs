import { test } from "node:test";
import assert from "node:assert/strict";
import { cycle, failedJobs, outcome } from "../src/lib/ci-logic.ts";

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
