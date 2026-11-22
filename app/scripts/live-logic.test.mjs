import { test } from "node:test";
import assert from "node:assert/strict";
import { elapsed, feed, follow, phrase, replay } from "../src/lib/live-logic.ts";

let n = 0;
const ev = (kind, fields = {}, at = 0) => ({ id: ++n, session: 1, at, kind, ...fields });

test("an agent thinks, writes, runs a tool, then is done", () => {
  let a = follow(undefined, ev("user", { text: "go" }));
  assert.equal(a.now.kind, "thinking");
  a = follow(a, { ...ev("delta", { text: "Hel" }), id: 0 });
  assert.equal(a.now.kind, "writing");
  // Every other token changes nothing: no update for the view.
  assert.equal(follow(a, { ...ev("delta", { text: "lo" }), id: 0 }), a);
  a = follow(a, ev("tool_use", { call: "t1", name: "Bash", input: { command: "npm test" } }, 5));
  assert.deepEqual([a.now.kind, a.now.state, a.recent.length], ["tool", "running", 1]);
  a = follow(a, ev("tool_result", { call: "t1", output: "boom", isError: true, truncated: false }));
  assert.equal(a.recent[0].state, "failed");
  assert.equal(a.now.kind, "thinking");
  a = follow(a, ev("turn_end", { outcome: "done" }));
  assert.equal(a.now, null);
  assert.equal(a.recent.length, 1);
});

test("a result for a call never heard changes nothing", () => {
  const a = follow(undefined, ev("user", { text: "go" }));
  assert.equal(follow(a, ev("tool_result", { call: "x", output: "", isError: false, truncated: false })), a);
});

test("a tool call in plain words, what it is about as its target", () => {
  assert.deepEqual(phrase("Bash", { command: "npm test\nnpm run lint" }), { key: "liveRun", target: "npm test" });
  assert.deepEqual(phrase("Read", { file_path: "/Users/alex/code/acme/api/src/app.ts" }), { key: "liveRead", target: "…/api/src/app.ts" });
  assert.deepEqual(phrase("Edit", { file_path: "src/app.ts" }), { key: "liveEdit", target: "src/app.ts" });
  assert.deepEqual(phrase("WebSearch", { query: "svelte 5 runes" }), { key: "liveWebSearch", target: "svelte 5 runes" });
  assert.deepEqual(phrase("mcp__github__create_issue", {}), { key: "liveMcp", vars: { server: "github", tool: "create issue" } });
  assert.deepEqual(phrase("Frobnicate", {}), { key: "liveTool", vars: { tool: "Frobnicate" } });
  assert.equal(phrase("Bash", { command: "x".repeat(200) }).target.length, 80);
});

test("durations read as seconds, minutes, then hours", () => {
  assert.deepEqual(elapsed(42), { key: "liveSeconds", vars: { n: 42 } });
  assert.deepEqual(elapsed(7 * 60 + 3), { key: "liveMinutes", vars: { n: 7 } });
  assert.deepEqual(elapsed(3600 + 5 * 60), { key: "liveHours", vars: { h: 1, m: "05" } });
  assert.deepEqual(elapsed(-3), { key: "liveSeconds", vars: { n: 0 } });
});

test("a run's feed: starts and ends in time order, files changed in a row as one line", () => {
  const step = (key, fields) => ({ key, role: "back", state: "running", session: null, started: null, finished: null, report: null, ...fields });
  const call = (at, name, input, call = `c${at}`) => ({ kind: "tool", at, call, name, input, state: "done" });
  const heard = { 7: [call(12, "Read", { file_path: "a.ts" }), call(13, "Edit", { file_path: "a.ts" }), call(14, "Edit", { file_path: "b.ts" }), call(15, "Write", { file_path: "c.ts" }), call(16, "Bash", { command: "npm test" })] };
  const steps = [
    step("plan", { role: "orchestrator", state: "done", started: 1, finished: 10 }),
    step("back", { session: 7, started: 11 }),
    step("review", { role: "reviewer", state: "blocked", started: 2, finished: 9, report: { status: "failed" } }),
    step("later", { state: "pending" }),
  ];
  const moments = feed(steps, (s) => heard[s] ?? []);
  assert.deepEqual(
    moments.map((m) => [m.kind, m.at, m.step]),
    [
      ["start", 1, "plan"],
      ["start", 2, "review"],
      ["end", 9, "review"],
      ["end", 10, "plan"],
      ["start", 11, "back"],
      ["act", 12, "back"],
      ["edits", 13, "back"],
      ["act", 16, "back"],
    ],
  );
  assert.equal(moments[2].how, "failed");
  assert.deepEqual(moments[6].files, ["a.ts", "b.ts", "c.ts"]);
});

test("a session's past read after it was heard live counts each call once", () => {
  const use = ev("tool_use", { call: "r1", name: "Read", input: { file_path: "a.ts" } });
  const done = ev("tool_result", { call: "r1", output: "", isError: false, truncated: false });
  // Heard live, then the same events again with the stored past.
  const live = [use, done].reduce(follow, undefined);
  assert.equal(follow(live, use), live);
  const later = ev("tool_use", { call: "r2", name: "Bash", input: { command: "npm test" } });
  const writing = { ...ev("delta", { text: "ok" }), id: 0 };
  const a = replay([use, done], [done, later, writing]);
  assert.deepEqual(a.recent.map((c) => c.call), ["r1", "r2"]);
  assert.equal(a.now.kind, "writing");
});

test("a failed change, or one with no file, keeps its own line in a run's feed", () => {
  const step = { key: "front", role: "front", state: "running", session: 3, started: 1, finished: null, report: null };
  const edit = (at, path, state = "done") => ({ kind: "tool", at, call: `e${at}`, name: "Edit", input: path ? { file_path: path } : {}, state });
  const kinds = (calls) => feed([step], () => calls).map((m) => (m.kind === "edits" ? `edits:${m.files.length}` : m.kind));
  assert.deepEqual(kinds([edit(2, "a.ts"), edit(3, "b.ts", "failed"), edit(4, "c.ts")]), ["start", "act", "act", "act"]);
  assert.deepEqual(kinds([edit(2, "a.ts"), edit(3, ""), edit(4, "c.ts"), edit(5, "d.ts")]), ["start", "act", "act", "edits:2"]);
});
