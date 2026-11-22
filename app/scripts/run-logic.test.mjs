import { test } from "node:test";
import assert from "node:assert/strict";
import { awaitsDelivery, expected, prName, shown, stageOf, stages } from "../src/lib/run-logic.ts";

const ticket = (o = {}) => ({ status: "running", prUrl: null, prState: null, ...o });
const step = (key, state, kind = "agent") => ({ key, kind, state });
const team = (steps, state = "running") => ({ state, steps });
const flat = (list) => list.map((s) => `${s.id}:${s.state}`).join(" ");

const plan = [
  step("triage", "done"),
  step("approve-route", "done", "gate"),
  step("prd", "skipped"),
  step("frame", "done"),
  step("ux", "skipped"),
  step("plan", "done"),
  step("approve-plan", "waiting", "gate"),
  step("implement", "pending", "foreach"),
  step("checks", "pending", "shell"),
  step("rules", "pending", "rules"),
  step("review", "pending", "review"),
  step("security", "pending"),
  step("seo", "skipped"),
  step("visual", "pending", "visual"),
  step("deliver", "pending", "connector"),
  step("implement:T1", "pending", "task"),
];

test("the shipped steps fold into seven stages; a custom step is its own", () => {
  assert.equal(stageOf(step("prd", "done")), "frame");
  assert.equal(stageOf(step("approve-plan", "done", "gate")), "plan");
  assert.equal(stageOf(step("implement:T2", "done", "task")), "tasks");
  assert.equal(stageOf(step("second-look", "done", "review")), "review");
  assert.equal(stageOf(step("ship", "done", "connector")), "deliver");
  assert.equal(stageOf(step("migrate-db", "done", "shell")), "migrate-db");
});

test("a team's timeline: the workflow's stages in order, what waits for the user amber, delivery last", () => {
  assert.equal(
    flat(stages(ticket(), team(plan, "waiting"))),
    "triage:done approve-route:done frame:done plan:waiting tasks:pending review:pending deliver:pending",
  );
});

test("a stage half done works while the team does; one all skipped is left out", () => {
  const steps = [step("frame", "done"), step("architecture", "pending"), step("approve-route", "skipped", "gate"), step("deliver", "pending", "connector")];
  assert.equal(flat(stages(ticket(), team(steps))), "frame:working deliver:pending");
  assert.equal(flat(stages(ticket(), team(steps, "paused"))), "frame:pending deliver:pending");
});

test("a blocked step waits for the user while the team works, and shows failed once it stopped", () => {
  const steps = [step("frame", "done"), step("plan", "blocked"), step("deliver", "pending", "connector")];
  assert.equal(flat(stages(ticket(), team(steps))), "frame:done plan:waiting deliver:pending");
  assert.equal(flat(stages(ticket(), team(steps, "budget"))), "frame:done plan:failed deliver:pending");
});

test("delivery is done only once the ticket has a pull request; a finished team waits for it", () => {
  const steps = [step("plan", "done"), step("deliver", "done", "connector")];
  assert.equal(flat(stages(ticket(), team(steps, "done"))), "plan:done deliver:waiting");
  assert.equal(flat(stages(ticket({ status: "delivered", prUrl: "u" }), team(steps, "done"))), "plan:done deliver:done");
  const noConnector = [step("plan", "done")];
  assert.equal(flat(stages(ticket(), team(noConnector, "running"))), "plan:done deliver:pending");
});

test("a run without a team: launched, pull request, delivered once merged", () => {
  assert.equal(flat(stages(ticket())), "launched:done pr:working delivered:pending");
  assert.equal(flat(stages(ticket({ status: "delivered", prUrl: "u" }))), "launched:done pr:done delivered:working");
  assert.equal(flat(stages(ticket({ status: "delivered", prUrl: "u", prState: "merged" }))), "launched:done pr:done delivered:done");
  assert.equal(flat(stages(ticket({ status: "delivered", prUrl: "u", prState: "closed" }))), "launched:done pr:done delivered:pending");
  assert.equal(flat(stages(ticket({ status: "failed" }))), "launched:failed pr:pending delivered:pending");
});

test("one state for a ticket and its team", () => {
  const waiting = team(plan, "waiting");
  assert.deepEqual(shown(ticket(), waiting), { status: "waiting", label: null });
  assert.deepEqual(shown(ticket(), team([step("plan", "running")])), { status: "working", label: null });
  assert.deepEqual(shown(ticket(), team([step("plan", "done")], "paused")), { status: "idle", label: "teamRunsPaused" });
  assert.deepEqual(shown(ticket(), team([], "budget")), { status: "budget", label: null });
  assert.deepEqual(shown(ticket(), team([], "stopped")), { status: "stopped", label: null });
  assert.deepEqual(shown(ticket(), team([], "done")), { status: "waiting", label: null });
  assert.deepEqual(shown(ticket({ status: "delivered", prUrl: "u" }), team([], "done")), { status: "delivered", label: null });
  assert.deepEqual(shown(ticket({ status: "delivered", prUrl: "u", prState: "merged" })), { status: "delivered", label: "ticketsUiReady" });
  assert.deepEqual(shown(ticket({ status: "failed" }), waiting), { status: "failed", label: null });
  assert.deepEqual(shown(ticket({ status: "closed" }), waiting), { status: "closed", label: null });
  assert.deepEqual(shown(ticket()), { status: "running", label: null });
});

test("what the run expects: answers, the delivery, or the cleanup", () => {
  assert.equal(expected(ticket(), team(plan, "waiting")), "answer");
  assert.equal(expected(ticket(), team(plan, "paused")), null);
  assert.equal(expected(ticket(), team([], "done")), "deliver");
  assert.equal(expected(ticket()), null);
  assert.equal(expected(ticket({ status: "delivered", prUrl: "u", prState: "merged" })), "clean");
  assert.equal(expected(ticket({ status: "closed" }), team(plan, "waiting")), null);
  assert.equal(awaitsDelivery(ticket({ status: "delivered" }), team([], "done")), false);
});

test("a GitHub pull request is named owner/repo#n", () => {
  assert.equal(prName("https://github.com/acme/api/pull/486"), "acme/api#486");
  assert.equal(prName("https://github.com/acme/api/pull/486/files"), "acme/api#486");
  assert.equal(prName("https://gitlab.example/acme/api/-/merge_requests/3"), "https://gitlab.example/acme/api/-/merge_requests/3");
});
