import { test } from "node:test";
import assert from "node:assert/strict";
import { todos, version, watches } from "../src/lib/todo-logic.ts";

const base = {
  project: "acme",
  scout: { draft: null, journal: [], claude: "2.1.330 (Claude Code)" },
  globalScout: null,
  conventions: [{ slug: "front", draft: null }],
  scheduled: true,
  claude: "2.1.330 (Claude Code)",
  items: [],
  dismissed: [],
};

test("nothing waits in an up-to-date library", () => {
  assert.deepEqual(todos(base), []);
});

test("each kind of to-do, the most pressing first", () => {
  const entry = (name, flaw, review = "pending") => ({ kind: "skill", name, review, critique: [{ flaw }] });
  const got = todos({
    ...base,
    scout: {
      draft: { entries: [entry("php-laravel", "outdated"), entry("handoff", "vague"), entry("done", "vague", "accepted")] },
      journal: [
        { kind: "skill", name: "react", action: "applied", project: "acme", at: 2 },
        { kind: "skill", name: "react", action: "applied", project: "acme", at: 1 },
        { kind: "skill", name: "rust", action: "applied", project: "other", at: 1 },
        { kind: "rule", name: "scope", action: "rejected", project: null, at: 1 },
      ],
      claude: "2.1.300 (Claude Code)",
    },
    conventions: [
      { slug: "api", draft: { questions: ["Laquelle ?"], items: [] } },
      { slug: "front", draft: { questions: [], items: [{ review: "pending" }, { review: "accepted" }] } },
    ],
    scheduled: false,
    items: [
      { kind: "skill", name: "react", active: false },
      { kind: "skill", name: "rust", active: false },
      { kind: "rule", name: "scope", active: false },
    ],
  });
  assert.deepEqual(got.map((t) => t.id), ["questions:api", "decide:front", "scout", "off:skill/react", "claude:2.1.330", "noSchedule"]);
  assert.deepEqual(got[2], { id: "scout", kind: "scout", count: 2, outdated: 1, names: ["php-laravel", "handoff"] });
  assert.deepEqual(got[4], { id: "claude:2.1.330", kind: "claude", seen: "2.1.300", now: "2.1.330" });
});

test("a watch never run, a project without conventions; dismissed lines stay away", () => {
  const fresh = { ...base, scout: { draft: null, journal: [], claude: null }, conventions: [] };
  assert.deepEqual(todos(fresh).map((t) => t.id), ["never", "noConventions"]);
  assert.deepEqual(todos({ ...fresh, dismissed: ["never"] }).map((t) => t.id), ["noConventions"]);
  assert.deepEqual(todos({ ...fresh, project: null, conventions: null }).map((t) => t.id), ["never"]);
});

test("versions and the scheduled watch of a scope", () => {
  assert.equal(version("2.1.330 (Claude Code)"), "2.1.330");
  assert.equal(version(null), null);
  assert.ok(watches({ mode: "scout", dir: "/r/api" }, ["/r/api"], ["/r/api"]));
  assert.ok(!watches({ mode: "headless", dir: "/r/api" }, ["/r/api"], ["/r/api"]));
  assert.ok(watches({ mode: "scout", dir: "/home/alex" }, [], ["/r/api"]), "the global library's");
  assert.ok(!watches({ mode: "scout", dir: "/r/api" }, [], ["/r/api"]));
});
