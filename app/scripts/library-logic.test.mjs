import { test } from "node:test";
import assert from "node:assert/strict";
import { broughtBy, cost, groupOf, ladder, merge, rolesOf, sorted, switchTo, why } from "../src/lib/library-logic.ts";

const item = (name, extra = {}) => ({
  kind: "skill",
  name,
  description: null,
  appliesTo: [],
  tags: [],
  runtime: null,
  meta: {},
  body: "",
  error: null,
  layer: "shipped",
  path: null,
  shadows: [],
  toggles: {},
  active: false,
  activatedBy: [],
  package: null,
  ...extra,
});

test("each repository's sessions are merged: on where one gets it, the repository layer kept with its repository", () => {
  const [next, offered] = merge([
    { repo: "api", items: [item("nextjs")] },
    { repo: "web", items: [item("nextjs", { active: true, activatedBy: ["nextjs"] }), item("web-errors", { layer: "repo" })] },
  ]);
  assert.equal(next.active, true);
  assert.deepEqual(next.activeIn, ["web"]);
  assert.deepEqual(why(next), { key: "found", tags: "nextjs", repos: "web" });
  assert.equal(offered.repo, "web");
  assert.equal(groupOf(offered), "offered");
  assert.deepEqual(why(offered), { key: "offered", repo: "web" });
});

test("the most specific switch says why, and an item on by default but cut here says it is cut here only", () => {
  const [cut] = merge([{ repo: null, items: [item("notes", { layer: "global", toggles: { project: false } })] }]);
  assert.deepEqual(why(cut), { key: "offHereOnlyHere" });
  assert.equal(groupOf(cut), "off");
  const [plain] = merge([{ repo: null, items: [item("make", { toggles: { project: false } })] }]);
  assert.deepEqual(why(plain), { key: "offHere" });
  const [everywhere] = merge([{ repo: null, items: [item("make", { toggles: { global: true }, active: true })] }]);
  assert.deepEqual(why(everywhere), { key: "onEverywhere" });
  const rungs = ladder(everywhere);
  assert.deepEqual(rungs.map((r) => [r.layer, r.says, r.decides]), [
    ["repo", "nothing", false],
    ["project", "nothing", false],
    ["global", "on", true],
    ["base", "shipped", false],
  ]);
  // Without a switch, the layer it comes from decides.
  assert.equal(ladder(cut).find((r) => r.decides).layer, "project");
  assert.equal(ladder(merge([{ repo: null, items: [item("x")] }])[0]).find((r) => r.decides).layer, "base");
});

test("what was cut on purpose comes first among the items off", () => {
  const list = merge([{ repo: null, items: [item("a"), item("b", { toggles: { project: false } })] }]);
  assert.deepEqual(sorted(list).map((e) => e.name), ["b", "a"]);
});

test("a switch here writes the project only; everywhere writes the global one and lets the project follow, and both undo", () => {
  assert.deepEqual(switchTo({ toggles: {} }, "acme", false, false), {
    apply: [{ project: "acme", enabled: false }],
    undo: [{ project: "acme", enabled: null }],
  });
  assert.deepEqual(switchTo({ toggles: { project: false, global: true } }, "acme", true, true), {
    apply: [{ project: null, enabled: true }, { project: "acme", enabled: null }],
    undo: [{ project: null, enabled: true }, { project: "acme", enabled: false }],
  });
  // Without a project, the global library.
  assert.deepEqual(switchTo({ toggles: {} }, null, true, false).apply, [{ project: null, enabled: true }]);
});

test("cost, roles and the roles that bring an item", () => {
  assert.deepEqual(cost({ kind: "rule", description: "d", body: "x".repeat(400) }), { always: 100, used: 0, each: "turn" });
  assert.deepEqual(cost({ kind: "skill", description: "x".repeat(40), body: "x".repeat(4000) }), { always: 10, used: 1000, each: "session" });
  assert.deepEqual(rolesOf({ kind: "skill", meta: { metadata: { "termf-roles": "front back" } }, appliesTo: [] }), ["front", "back"]);
  assert.deepEqual(rolesOf({ kind: "rule", meta: {}, appliesTo: ["all", "qa", "squad:web"] }), ["qa"]);
  assert.deepEqual(broughtBy({ kind: "skill", name: "git" }, [{ name: "lead", meta: { skills: ["git"] } }, { name: "qa", meta: {} }]), ["lead"]);
});
