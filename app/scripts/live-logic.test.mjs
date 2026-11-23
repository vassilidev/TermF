import { test } from "node:test";
import assert from "node:assert/strict";
import { elapsed, follow, phrase, replay } from "../src/lib/live-logic.ts";

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
  assert.equal(a.recent[0].output, "boom", "a failure keeps what it said");
  assert.equal(a.now.kind, "thinking");
  a = follow(a, ev("turn_end", { outcome: "done" }));
  assert.equal(a.now, null);
  assert.equal(a.recent.length, 1);
});

test("a result for a call never heard changes nothing", () => {
  const a = follow(undefined, ev("user", { text: "go" }));
  assert.equal(follow(a, ev("tool_result", { call: "x", output: "", isError: false, truncated: false })), a);
  // A long failed run keeps its last lines only, where it says what broke.
  let b = follow(undefined, ev("tool_use", { call: "t9", name: "Bash", input: { command: "pytest" } }));
  b = follow(b, ev("tool_result", { call: "t9", output: Array.from({ length: 40 }, (_, i) => `line ${i}`).join("\n") + "\n", isError: true, truncated: false }));
  assert.deepEqual(b.recent[0].output.split("\n"), Array.from({ length: 12 }, (_, i) => `line ${i + 28}`));
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
