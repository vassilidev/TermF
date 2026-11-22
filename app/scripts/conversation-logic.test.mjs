import { test } from "node:test";
import assert from "node:assert/strict";
import { blocked, merge, nextAfter, pending, stream, summary, thread } from "../src/lib/conversation-logic.ts";

let n = 0;
const ev = (kind, fields = {}) => ({ id: ++n, session: 1, at: 0, kind, ...fields });
const usage = { input: 1, output: 1, cacheRead: 0, cacheCreation: 0 };
const end = (outcome = "done", cost = 0.01, total = 0.01) =>
  ev("turn_end", { outcome, result: null, error: null, usage, cost, total, context: null });

test("events come once, in id order, without deltas", () => {
  const a = { id: 1, kind: "user", text: "hi" };
  const b = { id: 2, kind: "text", text: "hello" };
  const c = { id: 3, kind: "turn_end" };
  assert.deepEqual(merge([a, b], [b, c, { id: 0, kind: "delta", text: "x" }]).map((e) => e.id), [1, 2, 3]);
  assert.deepEqual(merge([a, c], [b]).map((e) => e.id), [1, 2, 3]);
  assert.deepEqual(merge([], [c, a, b, a]).map((e) => e.id), [1, 2, 3]);
});

test("streamed text adds up until the stored text that holds it, from the start of a message", () => {
  const delta = (text) => ({ id: 0, kind: "delta", text });
  assert.equal(["ing.", " Done"].map(delta).reduce(stream, null), null, "opened mid-answer: nothing until a stored event");
  let live = stream(null, { id: 3, kind: "user", text: "go" });
  live = ["Hel", "lo"].map(delta).reduce(stream, live);
  assert.equal(live, "Hello");
  assert.equal(stream(live, { id: 4, kind: "text", text: "sub", parent: "t" }), "Hello", "a subagent's text is not the agent's");
  assert.equal(stream(null, { id: 4, kind: "text", text: "sub", parent: "t" }), "");
  assert.equal(stream(live, { id: 4, kind: "text", text: "Hello" }), "");
  assert.equal(stream("Hel", { id: 5, kind: "turn_end" }), "", "an interrupted turn drops what streamed");
});

test("only the last request since the turn or the process began waits", () => {
  n = 0;
  const ask = (request) => ev("permission", { request, tool: "Bash", input: {}, description: null });
  assert.equal(pending([ask("a"), ev("permission_answer", { request: "a", allow: true, message: null })]), null);
  assert.equal(pending([ask("old"), ev("started", { resumed: true }), ev("user", { text: "go" }), ask("new")]), "new");
  assert.equal(pending([ask("old"), end("interrupted")]), null);
  assert.equal(pending([ask("a"), ask("b"), ev("permission_answer", { request: "b", allow: false, message: null })]), "a");
});

test("a thread groups turns, tool results and a subagent's calls under its parent", () => {
  n = 0;
  const turns = thread([
    ev("started", { resumed: false }),
    ev("init", { sessionId: "s", model: "m", permissionMode: "manual" }),
    ev("user", { text: "fix it" }),
    ev("text", { text: "Looking." }),
    ev("tool_use", { call: "task", name: "Agent", input: { description: "Explore" } }),
    ev("text", { text: "sub says", parent: "task" }),
    ev("tool_use", { call: "grep", name: "Grep", input: { pattern: "login" }, parent: "task" }),
    ev("tool_result", { call: "grep", output: "a.ts", isError: false, truncated: false, parent: "task" }),
    ev("tool_result", { call: "task", output: "found", isError: false, truncated: false }),
    ev("permission", { request: "r1", tool: "Bash", input: { command: "rm x" }, description: null }),
    ev("permission_answer", { request: "r1", allow: false, message: "no" }),
    ev("denied", { tool: "Write", message: "mode" }),
    end("done", 0.02, 0.02),
    ev("taken_over", { tile: 3 }),
    ev("handed_back"),
    ev("user", { text: "again" }),
    ev("error", { message: "boom" }),
  ]);
  assert.deepEqual(turns.map((t) => t.user), ["fix it", null, "again"]);
  const [first, between, last] = turns;
  assert.deepEqual(first.items.map((i) => i.type), ["text", "tool", "permission", "denied"]);
  const task = first.items[1];
  assert.equal(task.result.output, "found");
  assert.deepEqual(task.children.map((i) => i.type), ["text", "tool"]);
  assert.equal(task.children[1].result.output, "a.ts");
  assert.deepEqual(first.items[2].answer, { allow: false, message: "no" });
  assert.equal(first.end.cost, 0.02);
  assert.deepEqual(between.items.map((i) => i.type), ["taken_over", "handed_back"]);
  assert.equal(between.end, null);
  assert.deepEqual(last.items.map((i) => i.type), ["error"]);
  assert.equal(last.end, null);
});

test("a call without its result, or a result without its call, does not break the thread", () => {
  n = 0;
  const [turn] = thread([
    ev("user", { text: "go" }),
    ev("tool_use", { call: "a", name: "Bash", input: {} }),
    ev("tool_result", { call: "unknown", output: "", isError: true, truncated: false }),
    ev("text", { text: "orphan", parent: "gone" }),
    end("interrupted"),
  ]);
  assert.equal(turn.items[0].result, null);
  assert.equal(turn.items[1].text, "orphan");
  assert.equal(turn.end.outcome, "interrupted");
});

test("a tool call is summed up by what it acts on", () => {
  assert.equal(summary({ command: "pnpm test\n--watch", description: "Run tests" }), "pnpm test");
  assert.equal(summary({ file_path: "/a/b.ts", old_string: "x" }), "/a/b.ts");
  assert.equal(summary({ pattern: "login", path: "src" }), "login");
  assert.equal(summary({ todos: [], label: "  plan " }), "plan");
  assert.equal(summary({ n: 3 }), '{"n":3}');
  assert.equal(summary({}), "");
  assert.equal(summary(null), "");
  assert.equal(summary("raw"), "raw");
});

test("a message goes only between turns", () => {
  for (const s of ["idle", "stopped", "error"]) assert.equal(blocked(s), null);
  for (const s of ["working", "waiting", "interactive", "budget"]) assert.equal(blocked(s), s);
});

test("the next one waiting comes after the current one, else the first", () => {
  const order = ["tile:1", "tile:2", "agent:9", "agent:8"];
  assert.equal(nextAfter(order, ["tile:1", "agent:8"], "tile:2"), "agent:8");
  assert.equal(nextAfter(order, ["tile:1", "agent:8"], "agent:8"), "tile:1");
  assert.equal(nextAfter(order, ["agent:9"], null), "agent:9");
  assert.equal(nextAfter(order, [], "tile:1"), null);
});
