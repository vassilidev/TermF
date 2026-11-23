import { test } from "node:test";
import assert from "node:assert/strict";
import { askKey, openRuns, waits } from "../src/lib/home-logic.ts";

test("waits: live tiles first, then the most recent first, the CTO's permission apart from other agents", () => {
  const list = waits(
    [{ id: 3 }],
    [
      { id: 32, updated: 100, cto: false },
      { id: 45, updated: 300, cto: true },
    ],
    [
      { run: 7, step: "approve-plan", kind: "plan", updated: 200 },
      { run: 6, step: "triage", kind: "questions", updated: 50 },
    ],
  );
  assert.deepEqual(
    list.map((w) => [w.kind, w.key]),
    [
      ["tile", "tile:3"],
      ["cto", "agent:45"],
      ["plan", "ask:7:approve-plan"],
      ["agent", "agent:32"],
      ["questions", "ask:6:triage"],
    ],
  );
  assert.equal(list[2].run, 7);
  assert.deepEqual(waits([], [], []), []);
});

test("waits: an Ops request and an agent's request for a key take their place by date", () => {
  const list = waits([], [{ id: 32, updated: 100, cto: false }], [], [{ id: 4, at: 50, project: "acme" }], [
    { id: 2, at: 150, project: "acme" },
  ]);
  assert.deepEqual(
    list.map((w) => [w.kind, w.key]),
    [
      ["secret", "secret:2"],
      ["agent", "agent:32"],
      ["ops", "ops:4"],
    ],
  );
  assert.equal(list[0].project, "acme");
});

test("openRuns: open ticket runs with their team, a team without its ticket run yet, most recent first", () => {
  const runs = [
    { id: 1, project: "acme", status: "running", updated: 10 },
    { id: 2, project: "acme", status: "delivered", updated: 50 },
    { id: 3, project: "acme", status: "closed", updated: 90 },
    { id: 4, project: "docs", status: "running", updated: 70 },
  ];
  const teams = [
    { run: 1, target: { project: "acme" }, state: "waiting", updated: 60 },
    { run: 5, target: { project: "acme" }, state: "running", updated: 40 },
    { run: 6, target: { project: "acme" }, state: "done", updated: 80 },
  ];
  const mine = openRuns(runs, teams, "acme");
  assert.deepEqual(
    mine.map((o) => [o.run?.id ?? null, o.team?.run ?? null, o.at]),
    [
      [1, 1, 60],
      [2, null, 50],
      [null, 5, 40],
    ],
  );
  assert.equal(openRuns(runs, teams, null).length, 4, "every project's");
  const other = { id: 9, project: "acme", status: "running", updated: 99, parent: 1 };
  assert.equal(openRuns([...runs, other], teams, "acme").length, 3, "the run of another repository shows on its change's page");
});
