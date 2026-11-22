import { test } from "node:test";
import assert from "node:assert/strict";
import { asks, budgetError, current, levels, notices, progress, upsert } from "../src/lib/team-run-logic.ts";

const task = (id, depends_on = []) => ({ id, role: "front", repo: "web", size: "S", covers: [], goal: id, files_hint: [], depends_on, ui_impact: false });
const step = (key, state, more = {}) => ({
  key,
  kind: "agent",
  role: null,
  task: null,
  needs: [],
  state,
  plan: false,
  session: null,
  sessions: [],
  attempt: 1,
  feedback: null,
  files: [],
  report: null,
  violations: [],
  cost: 0,
  note: null,
  started: null,
  finished: null,
  ...more,
});
const run = (steps, more = {}) => ({
  run: 7,
  target: { run: 7, project: "acme", repo: "web", worktree: "/w", ticket: "ACME-1", title: "Export" },
  workflow: "default",
  state: "running",
  budget: 15,
  cost: 0,
  plan: null,
  steps,
  note: null,
  created: 1,
  updated: 1,
  ...more,
});
const ids = (out) => out.map((level) => level.map((t) => t.id));

test("levels: a task comes after every task it depends on", () => {
  assert.deepEqual(ids(levels([task("T1"), task("T2", ["T1"]), task("T3")])), [["T1", "T3"], ["T2"]]);
  assert.deepEqual(ids(levels([task("T3", ["T2"]), task("T2", ["T1"]), task("T1")])), [["T1"], ["T2"], ["T3"]]);
  assert.deepEqual(ids(levels([task("T1"), task("T2", ["T1"]), task("T3", ["T1", "T2"])])), [["T1"], ["T2"], ["T3"]]);
  assert.deepEqual(ids(levels([])), []);
});

test("levels: unknown dependencies are ignored, a cycle comes last", () => {
  assert.deepEqual(ids(levels([task("T1", ["T9"])])), [["T1"]]);
  assert.deepEqual(ids(levels([task("T1"), task("T2", ["T3"]), task("T3", ["T2"])])), [["T1"], ["T2", "T3"]]);
});

test("asks: the plan gate, questions, the visual and delivery gates, a failed step", () => {
  const plan = { summary: "", acceptance: [], out_of_scope: [], questions: [], tasks: [] };
  const steps = [
    step("plan", "done", { plan: true }),
    step("approve-plan", "waiting", { kind: "gate", needs: ["plan"] }),
    step("implement:T1", "blocked", { kind: "task", report: { status: "blocked", questions: ["Which API?"] } }),
    step("implement:T2", "blocked", { kind: "task", note: "tests fail" }),
    step("visual", "waiting", { kind: "visual", needs: ["rules"] }),
    step("deliver", "waiting", { kind: "connector" }),
    step("other", "waiting", { kind: "gate", needs: ["checks"] }),
    step("checks", "running", { kind: "shell" }),
  ];
  const out = asks(run(steps, { plan }));
  assert.deepEqual(
    out.map((a) => [a.step.key, a.kind, a.accept, a.questions]),
    [
      ["approve-plan", "plan", true, []],
      ["implement:T1", "questions", true, ["Which API?"]],
      ["implement:T2", "blocked", true, []],
      ["visual", "visual", true, []],
      ["deliver", "deliver", true, []],
      ["other", "gate", true, []],
    ],
  );
});

test("asks: a blocked plan step is never accepted as it is; its questions are the plan's", () => {
  const blocked = [step("plan", "blocked", { plan: true })];
  const withQuestions = { summary: "", acceptance: [], out_of_scope: [], questions: ["CSV or XLSX?"], tasks: [] };
  assert.deepEqual(
    asks(run(blocked, { plan: withQuestions, state: "waiting" })).map((a) => [a.kind, a.accept, a.questions]),
    [["questions", false, ["CSV or XLSX?"]]],
  );
  assert.deepEqual(
    asks(run(blocked, { state: "waiting" })).map((a) => [a.kind, a.accept]),
    [["blocked", false]],
  );
});

test("asks: nothing while the team is paused, stopped, out of budget or done", () => {
  const steps = [step("approve-plan", "waiting", { kind: "gate" })];
  for (const state of ["paused", "stopped", "budget", "done"]) assert.deepEqual(asks(run(steps, { state })), []);
  assert.equal(asks(run(steps, { state: "waiting" })).length, 1);
});

test("current: what waits first, else what works, tasks rather than their foreach", () => {
  const working = [
    step("implement:T1", "running", { kind: "task" }),
    step("implement", "running", { kind: "foreach" }),
    step("checks", "pending", { kind: "shell" }),
  ];
  assert.deepEqual(current(run(working)).map((s) => s.key), ["implement:T1"]);
  const waiting = [...working, step("visual", "waiting", { kind: "visual" })];
  assert.deepEqual(current(run(waiting)).map((s) => s.key), ["visual"]);
  assert.deepEqual(current(run([step("checks", "pending")])), []);
});

test("progress: steps over, the foreach counted through its tasks, the budget left", () => {
  const steps = [
    step("frame", "done"),
    step("implement:T1", "done", { kind: "task" }),
    step("implement:T2", "running", { kind: "task" }),
    step("implement", "pending", { kind: "foreach" }),
    step("visual", "skipped", { kind: "visual" }),
  ];
  assert.deepEqual(progress(run(steps, { cost: 4.5 })), { over: 3, total: 4, left: 10.5, spent: 0.3 });
  assert.deepEqual(progress(run([step("implement", "pending", { kind: "foreach" })], { budget: null, cost: 1 })), {
    over: 0,
    total: 1,
    left: null,
    spent: null,
  });
  assert.equal(progress(run([], { cost: 20 })).left, 0);
  assert.equal(progress(run([], { cost: 20 })).spent, 1);
});

test("notices: a gate that starts waiting, a newly blocked step, the end and the budget", () => {
  const before = run([step("plan", "running", { plan: true }), step("approve-plan", "pending", { kind: "gate" })]);
  const gate = run([step("plan", "done", { plan: true }), step("approve-plan", "waiting", { kind: "gate" })], { state: "waiting" });
  assert.deepEqual(notices(before, gate), [{ kind: "gate", step: "approve-plan" }]);
  assert.deepEqual(notices(gate, gate), []);
  assert.deepEqual(notices(undefined, gate), []);
  const blocked = run([step("plan", "blocked", { plan: true }), step("approve-plan", "pending", { kind: "gate" })]);
  assert.deepEqual(notices(before, blocked), [{ kind: "blocked", step: "plan" }]);
  assert.deepEqual(notices(before, run(before.steps, { state: "done" })), [{ kind: "done" }]);
  assert.deepEqual(notices(before, run(before.steps, { state: "budget" })), [{ kind: "budget" }]);
  assert.deepEqual(notices(before, run(before.steps, { state: "stopped" })), []);
  const expanded = run([...gate.steps, step("implement:T1", "blocked", { kind: "task" })]);
  assert.deepEqual(notices(gate, expanded), [{ kind: "blocked", step: "implement:T1" }]);
});

test("budgetError: none keeps the run's, an amount must be positive and above what is spent", () => {
  assert.equal(budgetError(null), null);
  assert.equal(budgetError(undefined, 3), null);
  assert.equal(budgetError(10), null);
  assert.equal(budgetError(0), "invalid");
  assert.equal(budgetError(-2), "invalid");
  assert.equal(budgetError(Number.NaN), "invalid");
  assert.equal(budgetError(3, 3), "spent");
  assert.equal(budgetError(3.5, 3), null);
  assert.equal(budgetError(null, 6, true), "missing");
  assert.equal(budgetError(5, 6, true), "spent");
  assert.equal(budgetError(7, 6, true), null);
});

test("upsert: a newer copy replaces, an older one never does, a new run goes first", () => {
  const a = { run: 1, updated: 10, state: "running" };
  const b = { run: 2, updated: 10, state: "waiting" };
  const runs = [a, b];
  assert.deepEqual(upsert(runs, { run: 1, updated: 12, state: "waiting" }), [{ run: 1, updated: 12, state: "waiting" }, b]);
  assert.equal(upsert(runs, { run: 1, updated: 9, state: "paused" }), runs, "an answer older than the event heard");
  assert.deepEqual(upsert(runs, { run: 2, updated: 10, state: "running" })[1].state, "running", "equal times: the last received");
  assert.deepEqual(upsert(runs, { run: 3, updated: 1, state: "running" }).map((r) => r.run), [3, 1, 2]);
});
