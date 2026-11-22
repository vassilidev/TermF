import { test } from "node:test";
import assert from "node:assert/strict";
import { askKey, openRuns, place, primary, waits } from "../src/lib/home-logic.ts";

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

test("place: a card takes the latest notice of its step, a step nobody told of goes after the thread", () => {
  const notices = [
    { key: 10, about: "waiting", run: 7, step: "approve-plan" },
    { key: 14, about: "direct", role: "back" },
    { key: 20, about: "blocked", run: 7, step: "approve-plan" },
    { key: 22, about: "waiting", run: 9, step: "visual" },
    { key: 25, about: "review", run: 7, step: "review" },
  ];
  const asks = [
    { run: 7, step: "approve-plan" },
    { run: 6, step: "triage" },
    { run: 8, step: "deliver" },
  ];
  const placed = place(notices, asks);
  assert.deepEqual([...placed.at], [[20, askKey(7, "approve-plan")]], "the older notice and the answered step keep their line");
  assert.deepEqual(placed.rest, ["ask:6:triage", "ask:8:deliver"]);
  assert.deepEqual(place([], []), { at: new Map(), rest: [] });
});

test("primary: the last decision of the thread, none while the CTO asks", () => {
  assert.equal(primary({ at: new Map([[20, "ask:7:a"], [30, "ask:8:b"]]), rest: [] }, false), "ask:8:b");
  assert.equal(primary({ at: new Map([[30, "ask:8:b"]]), rest: ["ask:6:t", "ask:5:d"] }, false), "ask:5:d");
  assert.equal(primary({ at: new Map([[30, "ask:8:b"]]), rest: [] }, true), null);
  assert.equal(primary({ at: new Map(), rest: [] }, false), null);
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
});
