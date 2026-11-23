import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  asks,
  hear,
  tellable,
  batchTasks,
  budgetError,
  byTheme,
  current,
  implications,
  editRouting,
  escalations,
  held,
  isForm,
  levels,
  notices,
  passes,
  planned,
  progress,
  projected,
  proposed,
  runs,
  rounds,
  routeBudget,
  taskRoles,
  tally,
  unanswered,
  reversed,
  swapped,
  withOthers,
  upsert,
} from "../src/lib/team-run-logic.ts";

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

const question = (id, kind, options = [], proposed = [], theme = "product") => ({
  id,
  theme,
  question: `${id}?`,
  why: "it matters",
  kind,
  options,
  proposed,
  implications: [],
});
test("implications: one per option for a choice, yes then no, a free text's as they come", () => {
  const choice = { ...question("format", "choice", ["CSV", "Excel"]), implications: ["light", " "] };
  assert.deepEqual(implications(choice), [{ option: "CSV", text: "light" }]);
  const odd = { ...question("format", "choice", ["CSV", "Excel"]), implications: ["light"] };
  assert.deepEqual(implications(odd), [], "not one per option: none shown");
  const bool = { ...question("public", "bool"), implications: ["a public link", "signed-in users only"] };
  assert.deepEqual(implications(bool), [
    { option: "yes", text: "a public link" },
    { option: "no", text: "signed-in users only" },
  ]);
  const text = { ...question("title", "text", [], ["Multiply"]), implications: ["matches the page", "yours"] };
  assert.deepEqual(implications(text), [
    { option: null, text: "matches the page" },
    { option: null, text: "yours" },
  ]);
});

test("implications: a yes/no question's that read the wrong way round are shown with a warning, never hidden", () => {
  const ask = (text, implications) => ({ ...question("q", "bool"), question: text, implications });
  const swappedEn = ask("Should the export include archived invoices?", [
    "Archived invoices are not exported",
    "Archived invoices are exported too",
  ]);
  assert.equal(implications(swappedEn).length, 2);
  assert.equal(reversed(swappedEn), true);
  const swappedFr = ask("Faut-il garder l’ancienne API ?", ["On n’a plus d’ancienne API", "L’ancienne API reste en place"]);
  assert.equal(implications(swappedFr).length, 2);
  assert.equal(reversed(swappedFr), true);
  // The review of PR #135: letters out of ASCII end no word, so "chaîne" or "aérien" hold no negation.
  const accents = ask("Faut-il supprimer l’ancienne chaîne de build ?", ["La chaîne de build est supprimée", "La chaîne aérienne reste"]);
  assert.equal(implications(accents).length, 2);
  assert.equal(reversed(accents), false);
  assert.equal(swapped("Garder le pont aérien ?", "Le pont aérien reste", "Le pont aérien ferme"), false);
  assert.equal(reversed({ ...question("q", "choice", ["a", "b"]), implications: ["no", "yes"] }), false, "yes/no questions only");
  const right = ask("Should the export include archived invoices?", ["Archived invoices are exported too", "Only open invoices are exported, never archived ones"]);
  assert.equal(implications(right).length, 2);
  assert.equal(reversed(right), false);
  const negative = ask("Should we not send the e-mail?", ["The e-mail is not sent", "The e-mail goes out"]);
  assert.equal(implications(negative).length, 2, "a negative question: yes keeps its negation");
  const plain = ask("Keep the old API?", ["The old API stays", "The old API is removed"]);
  assert.equal(implications(plain).length, 2, "no negation either way: taken as it comes");
  assert.equal(swapped("Add a button?", "No button is added", "A button is added"), true);
  assert.equal(swapped("Add a button?", "A button is added", "Nothing changes"), false);
});

const routing = (more = {}) => ({
  kind: "feature",
  size: "M",
  reason: "a new export",
  workflow: "default",
  skip: [],
  add: [],
  efforts: [],
  squads: [],
  questions: [],
  budget_usd: 4,
  changed: false,
  ...more,
});

test("asks: the routing gate, and the CTO's questions, never accepted as they are", () => {
  const gate = run([step("triage", "done"), step("approve-route", "waiting", { kind: "gate", needs: ["triage"] })], {
    state: "waiting",
    routing: routing(),
  });
  assert.deepEqual(asks(gate).map((a) => [a.step.key, a.kind, a.accept]), [["approve-route", "route", true]]);
  const asking = routing({ questions: [question("Q1", "bool", [], ["yes"])] });
  const blocked = run([step("triage", "blocked"), step("approve-route", "pending", { kind: "gate" })], {
    state: "waiting",
    routing: asking,
  });
  assert.deepEqual(
    asks(blocked).map((a) => [a.kind, a.accept, a.questions.map((q) => q.id)]),
    [["questions", false, ["Q1"]]],
  );
});

test("questionnaire: by theme, proposed answers, what is left to answer, old sentences are no form", () => {
  const qs = [
    question("format", "choice", ["CSV", "Excel"], ["CSV"], "product"),
    question("cols", "multi", ["date", "amount"], [], "scope"),
    question("public", "bool", [], [], "security"),
    question("name", "text", [], ["invoices.csv"], "mood"),
    question("who", "text", [], [], "product"),
  ];
  assert.deepEqual(
    byTheme(qs).map(([theme, list]) => [theme, list.map((q) => q.id)]),
    [
      ["product", ["format", "who"]],
      ["scope", ["cols"]],
      ["security", ["public"]],
      ["mood", ["name"]],
    ],
  );
  const answers = proposed(qs);
  assert.deepEqual(answers.format, ["CSV"]);
  answers.format.push("x");
  assert.deepEqual(qs[0].proposed, ["CSV"], "a copy");
  assert.deepEqual(unanswered(qs, proposed(qs)), ["public", "who"]);
  assert.deepEqual(
    unanswered(qs, { format: [], cols: ["date", " "], public: ["maybe"], name: ["  "], who: ["finance"] }),
    ["format", "cols", "public", "name"],
  );
  assert.deepEqual(unanswered(qs, { format: ["Excel"], cols: [], public: ["no"], name: ["a"], who: ["b"] }), []);
  assert.deepEqual(
    unanswered(qs, { format: ["PDF for the bank"], cols: ["date", "tax code"], public: ["no"], name: ["a"], who: ["b"] }),
    [],
    "an answer of one's own for a choice",
  );
  const own = withOthers(qs, proposed(qs), { format: " PDF ", cols: "tax code", public: "maybe", who: "finance" });
  assert.deepEqual(own.format, ["PDF"], "replaces the option of a choice");
  assert.deepEqual(own.cols, ["tax code"], "adds to a multiple choice");
  assert.deepEqual([own.public, own.who], [[], []], "only for a choice");
  assert.deepEqual(withOthers(qs, proposed(qs), { format: "  " }).format, ["CSV"], "blank: the option stays");
  assert.equal(isForm(qs), true);
  assert.equal(isForm([]), false);
  assert.equal(isForm(["CSV or XLSX?"].map((q) => ({ ...question("", "text"), question: q }))), false);
});

test("editRouting: skip and add against the new workflow's defaults, efforts of steps that no longer run dropped", () => {
  const workflow = {
    name: "default",
    description: "",
    steps: [
      { id: "prd", kind: "agent", role: "pm", optional: true, on: false },
      { id: "frame", kind: "agent", role: "po", optional: true, on: true },
      { id: "ux", kind: "agent", role: "ux", optional: true, on: false },
      { id: "plan", kind: "agent", role: "lead", optional: false, on: true },
      { id: "checks", kind: "shell", role: null, optional: false, on: true },
      { id: "security", kind: "agent", role: "secops", optional: false, on: true },
      { id: "update-base", kind: "update", role: "integrator", optional: false, on: true },
    ],
  };
  const cto = routing({
    skip: ["frame"],
    efforts: [
      { step: "security", effort: "low" },
      { step: "update-base", effort: "xhigh" },
      { step: "frame", effort: "high" },
      { step: "ux", effort: "high" },
    ],
    questions: [question("Q1", "text")],
  });
  assert.equal(runs(workflow.steps[1], cto), false);
  assert.equal(runs(workflow.steps[2], { skip: [], add: ["ux"] }), true);
  const next = editRouting(cto, workflow, ["ux"], ["web", "api"]);
  assert.deepEqual(
    [next.workflow, next.skip, next.add, next.efforts, next.squads, next.questions, next.budget_usd],
    ["default", ["frame"], ["ux"], [{ step: "security", effort: "low" }, { step: "update-base", effort: "xhigh" }, { step: "ux", effort: "high" }], ["web", "api"], [], 4],
  );
  assert.equal(next.reason, "a new export", "the rest is the CTO's");
  assert.equal(next.task, null, "a workflow with a plan takes no task");

  const direct = { name: "direct", description: "", steps: workflow.steps.filter((s) => s.id !== "frame") };
  const small = editRouting(cto, direct, [], ["api"], "S", { role: "back", goal: " Export invoices " });
  assert.deepEqual(
    [small.workflow, small.size, small.task],
    ["direct", "S", { id: "T1", role: "back", squad: "api", repo: "", size: "S", covers: [], goal: "Export invoices", files_hint: [], depends_on: [], ui_impact: false }],
  );
  const own = { ...task("T7"), covers: ["AC1"], files_hint: ["src/a.ts"] };
  const kept = editRouting({ ...cto, task: own }, direct, [], ["web", "api"], "M", { role: "front", goal: "g" });
  assert.deepEqual([kept.task.id, kept.task.squad, kept.task.covers, kept.task.files_hint, kept.task.repo], ["T7", "", ["AC1"], ["src/a.ts"], "web"]);
  assert.equal(editRouting({ ...cto, task: own }, workflow, [], []).task, null, "back to a plan, no task");
});

test("taskRoles: the roles a task may have, leads aside, each once", () => {
  const member = (role, lead = false, plannable = true) => ({ role, lead, plannable });
  const squads = [{ members: [member("lead", true), member("back"), member("qa")] }, { members: [member("front"), member("back"), member("ops", false, false)] }, {}];
  assert.deepEqual(taskRoles(squads), ["back", "qa", "front"]);
});

test("routeBudget: a change of the routing starts below the run's budget and the advice", () => {
  assert.equal(routeBudget(15, 25), 15, "the advice never raises it");
  assert.equal(routeBudget(15, 6), 6);
  assert.equal(routeBudget(15, 0), 15, "no advice");
  assert.equal(routeBudget(null, 6), 6);
  assert.equal(routeBudget(null, 0), null);
});

test("rounds and tally: a review step's rounds in order, the tasks a round sends back to, what is left", () => {
  const finding = (id, fate, to = null, severity = "important") => ({ id, severity, file: "src/a.ts", line: 3, scenario: "s", fix: "f", task: "", fate, to, reason: null });
  const first = { step: "review", round: 1, summary: "s", at: 1, findings: [finding("F1", "sent", "T2"), finding("F2", "sent", "T1"), finding("F3", "sent", "T2"), finding("F4", "reported", "T1", "nit")] };
  const second = { step: "review", round: 2, summary: "s", at: 2, findings: [finding("F1", "asked", "T1"), finding("F2", "left")] };
  const other = { step: "review-api", round: 1, summary: "s", at: 3, findings: [] };
  const run = { reviews: [first, other, second] };
  assert.deepEqual(rounds(run, "review").map((r) => r.round), [1, 2]);
  assert.deepEqual(rounds({}, "review"), [], "a run made before the review");
  assert.deepEqual(tally(first), { count: 4, to: ["T1", "T2"], asked: 0, reported: 1, left: 0, superseded: 0 });
  assert.deepEqual(tally(second), { count: 2, to: [], asked: 1, reported: 0, left: 1, superseded: 0 }, "a finding that waits goes nowhere yet");
});

test("held: only the last round of a blocked review that asks you has sent nothing yet", () => {
  const finding = (fate) => ({ id: "F1", severity: "important", file: "a", line: 1, scenario: "s", fix: "f", task: "T1", fate, to: "T1", reason: null });
  const first = { step: "review", round: 1, summary: "s", at: 1, findings: [finding("sent")] };
  const second = { step: "review", round: 2, summary: "s", at: 2, findings: [finding("sent"), { ...finding("asked"), id: "F2" }] };
  const run = { reviews: [first, second] };
  const review = (state) => step("review", state, { kind: "review" });
  assert.equal(held(run, review("blocked"), second), true);
  assert.equal(held(run, review("blocked"), first), false, "an earlier round was sent");
  assert.equal(held(run, review("done"), second), false, "decided: what was sent went");
  assert.equal(held({ reviews: [first] }, review("blocked"), first), false, "blocked for another reason: nothing held");
});

test("the delivery goes when every criterion of the definition of done holds, or the user passed over them", () => {
  const done = (met, overridden = null) => ({ criteria: [{ id: "checks", met: true, note: "passed" }, { id: "docs", met, note: "" }], overridden });
  assert.equal(passes(done(true)), true);
  assert.equal(passes(done(false)), false);
  assert.equal(passes(done(false, "the docs come next")), true);
  assert.equal(passes(null), true, "no gate read it: nothing holds the delivery here");
  const warned = { criteria: [{ id: "docs", met: false, note: "no documentation changed", warn: true }], overridden: null };
  assert.equal(passes(warned), true, "a criterion that only warns holds nothing");
});

test("a step's escalations are the CTO's steps that decide on it", () => {
  const t1 = step("implement:T1", "pending", { kind: "task" });
  const cto = step("escalate-implement-T1", "running", { role: "cto", escalates: "implement:T1" });
  const r = run([t1, step("implement:T2", "done", { kind: "task" }), cto]);
  assert.deepEqual(escalations(r, t1), [cto]);
  assert.deepEqual(escalations(r, r.steps[1]), []);
});

test("batches: a sub-plan waits as its own ask, its tasks show under their batch with the ids they take", () => {
  const plan = { summary: "", acceptance: [], out_of_scope: [], questions: [], tasks: [task("T1"), task("T2", ["T1"])] };
  const sub = { ...plan, tasks: [task("T1"), task("T2", ["T1"]), task("QA", ["T2"])] };
  const proposed = run(
    [
      step("implement:T1", "done", { kind: "task", task: "T1" }),
      step("implement:T2", "waiting", { kind: "task", task: "T2", batch: true, subplan: sub }),
    ],
    { plan },
  );
  assert.deepEqual(
    asks(proposed).map((a) => [a.kind, a.step.key, a.accept]),
    [["subplan", "implement:T2", true]],
  );
  assert.deepEqual(
    planned(proposed).map((t) => t.id),
    ["T1", "T2"],
  );
  const shown = batchTasks(proposed, "T2");
  assert.equal(shown.grafted, false);
  assert.deepEqual(
    shown.tasks.map((t) => [t.id, t.depends_on]),
    [
      ["T2.1", ["T1"]],
      ["T2.2", ["T1", "T2.1"]],
      ["T2.QA", ["T1", "T2.2"]],
    ],
  );
  assert.equal(batchTasks(proposed, "T1"), null, "not a batch");

  const grafted = run(
    [
      step("implement:T2", "done", { kind: "task", task: "T2", batch: true, subplan: sub }),
      step("implement:T2.1", "running", { kind: "task", task: "T2.1" }),
    ],
    { plan: { ...plan, tasks: [...plan.tasks, task("T2.1", ["T1"])] } },
  );
  assert.deepEqual(
    planned(grafted).map((t) => t.id),
    ["T1", "T2"],
    "grafted tasks show under their batch",
  );
  assert.deepEqual(batchTasks(grafted, "T2"), { tasks: [task("T2.1", ["T1"])], grafted: true });

  const blocked = { kind: "task", task: "T2", batch: true, subplan: { ...sub, questions: [{ id: "Q1" }] } };
  assert.deepEqual(
    asks(run([step("implement:T2", "blocked", blocked)], { plan })).map((a) => [a.kind, a.questions.length, a.accept]),
    [["questions", 1, false]],
    "a batch without a valid sub-plan is never accepted as it is",
  );
});

test("tell: a finished agent is offered only while the run is at rest or done", () => {
  const steps = [
    step("implement:T1", "running", { kind: "task", session: 1, worktree: "/t1" }),
    step("frame", "done", { session: 2 }),
    step("review", "done", { kind: "review", session: 3, worktree: "/copy" }),
    step("plan", "pending"),
  ];
  const keys = (r, open = true) => tellable(r, open).map((s) => s.key);
  assert.deepEqual(keys(run(steps)), ["implement:T1"], "picked up again while the run goes on, it would change the worktree under it");
  for (const state of ["waiting", "paused", "stopped", "budget", "done"]) {
    assert.deepEqual(keys(run(steps, { state })), ["implement:T1", "frame"], state);
  }
  assert.deepEqual(keys(run(steps, { state: "done" }), false), ["implement:T1"], "closed: its worktree is gone");
  const asking = [...steps, step("prd", "blocked", { session: 4 }), step("ux", "waiting", { session: 5 })];
  assert.deepEqual(keys(run(asking, { state: "waiting" })), ["implement:T1", "frame"], "a step that waits is answered on its lane");
});

test("projected: the cost at the pace so far, only while the team works and something is left", () => {
  const steps = [step("frame", "done"), step("plan", "done"), step("implement:T1", "running", { kind: "task" }), step("checks", "pending")];
  assert.equal(projected(run(steps, { cost: 3 })), 6);
  assert.equal(projected(run(steps, { cost: 3, state: "paused" })), null);
  assert.equal(projected(run([step("frame", "running")], { cost: 1 })), null, "nothing over yet");
  assert.equal(projected(run([step("frame", "done")], { cost: 1 })), null, "all over: the cost is the cost");
});

test("tell: the reply ends with its turn, or when the agent fails or exits", () => {
  const reply = () => ({ session: 4, who: "CTO", sent: "hi", text: "", heard: false, done: false, error: null });
  const ev = (kind, more = {}) => ({ id: 1, session: 4, at: 1, kind, ...more });
  let r = reply();
  for (const e of [
    ev("delta", { text: "before" }),
    ev("exited", { code: 0 }),
    ev("user", { text: "hi" }),
    ev("delta", { text: "Hel" }),
    ev("delta", { text: "lo" }),
    { ...ev("delta", { text: "other" }), session: 5 },
  ])
    hear(r, e);
  assert.deepEqual([r.text, r.done], ["Hello", false], "what it said after the message; the end of the turn before it is not its end");
  hear(r, ev("turn_end", { error: null }));
  assert.deepEqual([r.done, r.error], [true, null]);
  hear(r, ev("user", { text: "again" }));
  hear(r, ev("delta", { text: "!" }));
  assert.deepEqual([r.text, r.done], ["Hello", true], "its answer stays as its turn ended");

  r = reply();
  hear(r, ev("user", { text: "hi" }));
  hear(r, ev("exited", { code: 1 }));
  assert.equal(r.done, true, "its process ended during the turn");
  hear(r, ev("error", { message: "the agent's process ended during its turn" }));
  assert.equal(r.error, "the agent's process ended during its turn");
  r = reply();
  hear(r, ev("error", { message: "the session's budget is spent" }));
  assert.deepEqual([r.done, r.error], [true, "the session's budget is spent"], "it failed before it heard the message: never waits for ever");
  r = reply();
  hear(r, ev("user", { text: "hi" }));
  hear(r, ev("turn_end", { error: "overloaded" }));
  assert.deepEqual([r.done, r.error], [true, "overloaded"]);

  // The third review of PR #135: another message, or one from TermF, may start a turn before ours.
  r = reply();
  for (const e of [
    ev("user", { text: "The checks failed: fix them" }),
    ev("delta", { text: "Fixing" }),
    ev("turn_end", { error: null }),
    ev("user", { text: "@expert why?\n\nhi" }),
    ev("delta", { text: "Hello" }),
  ])
    hear(r, e);
  assert.deepEqual([r.text, r.heard, r.done], ["Hello", true, false], "the answer starts with the turn that holds our message");
});

test("negations: no lookbehind, which WebKit before Safari 16.4 cannot load", () => {
  const source = readFileSync(new URL("../src/lib/team-run-logic.ts", import.meta.url), "utf8");
  assert.doesNotMatch(source, /\(\?<[=!]/);
});

test("tell: the CTO is named in the user's language", () => {
  const source = readFileSync(new URL("../src/lib/RunTell.svelte", import.meta.url), "utf8");
  assert.doesNotMatch(source, /["'`]CTO["'`]/);
});
