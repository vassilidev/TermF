import { test } from "node:test";
import assert from "node:assert/strict";
import { askTitle, awaitsDelivery, baseUpdate, defaultStage, expected, mergeRequest, named, prName, repoRuns, shown, stageOf, stages, stepLabel, stepNames, stuck } from "../src/lib/run-logic.ts";

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

test("a team out of budget or stopped failed where it worked; a paused one only waits", () => {
  const steps = [step("plan", "done"), step("implement", "running", "foreach"), step("implement:T1", "running", "task")];
  assert.equal(flat(stages(ticket(), team(steps))), "plan:done tasks:working deliver:pending");
  assert.equal(flat(stages(ticket(), team(steps, "budget"))), "plan:done tasks:failed deliver:pending");
  assert.equal(flat(stages(ticket(), team(steps, "stopped"))), "plan:done tasks:failed deliver:pending");
  assert.equal(flat(stages(ticket(), team(steps, "paused"))), "plan:done tasks:pending deliver:pending");
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
  assert.deepEqual(shown(ticket({ status: "delivered", prUrl: "u", prState: "merged" })), { status: "delivered", label: "ticketsUiReady", hint: "uxfT1ReadyHint" });
  assert.deepEqual(shown(ticket({ status: "delivered", prUrl: "u", prState: "closed" })), { status: "delivered", label: "uxfT1ClosedReady", hint: "uxfT2ClosedHint" });
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

test("without its ticket run, a team run shows its own state", () => {
  assert.deepEqual(shown(null, team(plan, "waiting")), { status: "waiting", label: null });
  assert.deepEqual(shown(undefined, team([], "done")), { status: "done", label: null });
  assert.deepEqual(shown(null, team([], "budget")), { status: "budget", label: null });
});

test("a step reads as a task's id, a shipped step's name, or any other step's id", () => {
  assert.deepEqual(stepLabel("approve-plan"), { name: "ux3t2StepApprovePlan" });
  assert.deepEqual(stepLabel("review"), { name: "ux3t2StepCodeReview" });
  assert.deepEqual(stepLabel("implement:T2", "T2"), { text: "T2" });
  assert.deepEqual(stepLabel("implement:T2"), { text: "T2" });
  assert.deepEqual(stepLabel("migrate-db"), { text: "migrate-db" });
  assert.deepEqual(stepLabel("constructor"), { text: "constructor" });
  assert.equal(named(stepNames, "toString"), null);
});

test("a wait's title, naming its step only where the title says it", () => {
  const ask = (kind, key = "x", more = {}) => ({ kind, step: { key, kind: "agent", plan: false, ...more } });
  assert.deepEqual(askTitle(ask("route", "approve-route")), { key: "ctoEntryAskRoute", step: false });
  assert.deepEqual(askTitle(ask("plan", "approve-plan")), { key: "teamRunsAskPlan", step: false });
  assert.deepEqual(askTitle(ask("visual", "visual")), { key: "teamRunsAskVisual", step: false });
  assert.deepEqual(askTitle(ask("deliver", "deliver")), { key: "teamRunsAskDeliver", step: false });
  assert.deepEqual(askTitle(ask("blocked", "implement:T3")), { key: "teamRunsAskBlocked", step: true });
  assert.deepEqual(askTitle(ask("gate", "sign-off")), { key: "teamRunsAskGate", step: true });
  assert.deepEqual(askTitle(ask("questions", "triage")), { key: "ctoEntryAskQuestions", step: false });
  assert.deepEqual(askTitle(ask("questions", "plan", { plan: true })), { key: "teamRunsAskPlanQuestions", step: false });
  assert.deepEqual(askTitle(ask("questions", "review", { kind: "review" })), { key: "reviewLoopAsk", step: false });
  assert.deepEqual(askTitle(ask("questions", "migrations", { kind: "review" })), { key: "roles2ReviewAsk", step: true });
  assert.deepEqual(askTitle(ask("questions", "frame")), { key: "teamRunsAskQuestions", step: true });
  assert.deepEqual(askTitle(ask("subplan", "implement:T2", { kind: "task", batch: true })), { key: "teams2AskSubplan", step: true });
  assert.deepEqual(askTitle(ask("questions", "implement:T2", { kind: "task", batch: true })), { key: "teams2AskSubplanQuestions", step: true });
});

test("a GitHub pull request is named owner/repo#n", () => {
  assert.equal(prName("https://github.com/acme/api/pull/486"), "acme/api#486");
  assert.equal(prName("https://github.com/acme/api/pull/486/files"), "acme/api#486");
  assert.equal(prName("https://gitlab.example/acme/api/-/merge_requests/3"), "acme/api!3");
  assert.equal(prName("https://gitlab.com/acme/platform/api/-/merge_requests/7/diffs"), "acme/platform/api!7");
  assert.equal(prName("https://example.com/acme/api/pull/3"), "https://example.com/acme/api/pull/3");
  assert.equal(mergeRequest("https://github.com/acme/api/pull/486"), null);
});

test("the CTO's escalation shows through the step it decides on, never as a stage of its own", () => {
  const steps = [step("implement:T1", "pending", "task"), { ...step("escalate-implement-T1", "running"), escalates: "implement:T1" }, step("deliver", "pending", "connector")];
  assert.equal(flat(stages(ticket(), team(steps))), "tasks:working deliver:pending");
  assert.deepEqual(stepLabel("escalate-implement-T1"), { name: "disciplineEscalation" });
  assert.deepEqual(stepLabel("escalate-review"), { name: "disciplineEscalation" });
});

test("the update with the base, and the CTO's decision on it, belong to the delivery", () => {
  const steps = [
    step("review", "done", "review"),
    step("update-base", "pending", "update"),
    { ...step("escalate-update-base", "running"), escalates: "update-base" },
    step("deliver", "pending", "connector"),
  ];
  assert.equal(flat(stages(ticket(), team(steps))), "review:done deliver:working");
  assert.deepEqual(stepLabel("update-base"), { name: "integratorStep" });
  const delivered = ticket({ status: "delivered", prUrl: "u" });
  assert.equal(flat(stages(delivered, team(steps))), "review:done deliver:working", "updated again after its pull request");
  const again = [steps[0], step("update-base", "done", "update"), step("deliver", "waiting", "connector")];
  assert.equal(flat(stages(delivered, team(again, "waiting"))), "review:done deliver:waiting");
  assert.equal(flat(stages(delivered, team(again.map((s) => ({ ...s, state: "done" })), "done"))), "review:done deliver:done");
});

test("a pull request in conflict or behind its base is said, with the team's update once the team is done", () => {
  const delivered = ticket({ status: "delivered", prUrl: "https://github.com/acme/api/pull/7" });
  const done = team([step("update-base", "done", "update"), step("deliver", "done", "connector")], "done");
  const conflict = { conflict: true, behind: false };
  assert.deepEqual(baseUpdate(delivered, conflict, done), { state: "conflict", action: true });
  assert.deepEqual(baseUpdate(delivered, { conflict: false, behind: true }, done), { state: "behind", action: true });
  assert.equal(baseUpdate(delivered, { conflict: false, behind: false }, done), null, "up to date");
  assert.equal(baseUpdate(delivered, null, done), null, "not known yet");
  assert.deepEqual(baseUpdate(delivered, conflict), { state: "conflict", action: false }, "without a team: from its session");
  const older = team([step("deliver", "done", "connector")], "done");
  assert.deepEqual(baseUpdate(delivered, conflict, older), { state: "conflict", action: false }, "a workflow without the step");
  assert.equal(baseUpdate(delivered, conflict, team(done.steps, "running")), null, "the team at work shows its own state");
  assert.equal(baseUpdate({ ...delivered, prState: "merged" }, conflict, done), null, "merged");
  assert.equal(baseUpdate(ticket({ prUrl: delivered.prUrl }), conflict, done), null, "not delivered");
});

test("a change over several repositories: its own run first, each with where its pull request is", () => {
  const r = (id, more = {}) => ({ id, status: "running", prUrl: null, prState: null, parent: null, ...more });
  const pr = "https://github.com/acme/api/pull/8";
  const runs = [r(1), r(3, { parent: 1, status: "delivered", prUrl: pr }), r(2, { parent: 1 }), r(4), r(5, { parent: 4, status: "closed" })];
  assert.deepEqual(
    repoRuns(runs, 1).map((x) => [x.run.id, x.state]),
    [
      [1, "working"],
      [2, "working"],
      [3, "open"],
    ],
  );
  assert.deepEqual(repoRuns(runs, 2), [], "a run of one repository, or one that works for another");
  assert.deepEqual(
    repoRuns(runs, 4).map((x) => x.state),
    ["working", "gone"],
  );
  const merged = repoRuns([r(1), r(2, { parent: 1, status: "closed", prUrl: pr, prState: "merged" })], 1);
  assert.equal(merged[1].state, "merged");
});

test("stuck: where a team no longer at work stopped, nothing while it works", () => {
  const steps = [step("frame", "done"), step("implement:T1", "done", "task"), step("checks", "blocked", "shell"), step("deliver", "pending", "connector")];
  assert.deepEqual(stuck(team(steps, "running")), [], "at work, what waits is on a lane");
  assert.deepEqual(stuck(team(steps, "paused")).map((s) => s.key), ["checks"]);
  const halted = [step("frame", "done"), step("implement", "running", "foreach"), step("implement:T1", "running", "task")];
  assert.deepEqual(stuck(team(halted, "stopped")).map((s) => s.key), ["implement:T1"], "stopped mid-way: the step at work, not its foreach");
  assert.deepEqual(stuck(team(halted, "paused")), [], "a pause only waits");
});

test("the Run page's stage: where it stopped, else the tasks, else the stage under way or the last done", () => {
  const at = (steps, state = "running") => defaultStage(team(steps, state), stages(ticket(), team(steps, state)));
  const tasks = [step("plan", "done"), step("implement", "running", "foreach"), step("implement:T1", "running", "task"), step("checks", "pending", "shell")];
  assert.equal(at(tasks), "tasks");
  // A failed check of a team that stopped shows, not the tasks.
  const failed = [step("plan", "done"), step("implement:T1", "done", "task"), step("checks", "blocked", "shell"), step("deliver", "pending", "connector")];
  assert.equal(at(failed, "stopped"), "review");
  // The integrator's conflict, on the delivery stage.
  const conflict = [step("implement:T1", "done", "task"), step("update-base", "blocked", "update")];
  assert.equal(at(conflict, "paused"), "deliver");
  // A workflow without a foreach: the stage at work, else the last one done.
  assert.equal(at([step("spike", "done"), step("write", "running"), step("ship", "pending")]), "write");
  assert.equal(at([step("spike", "done"), step("write", "done")], "done"), "write", "not the delivery, which has no step here");
  // The CTO's escalation counts in the stage of the step it decides on.
  const escalated = [step("implement:T1", "done", "task"), { ...step("escalate-checks", "blocked"), escalates: "checks" }, step("checks", "done", "shell")];
  assert.equal(at(escalated, "paused"), "review");
});
