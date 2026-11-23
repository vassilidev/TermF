import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { fill, positional, slowTurn } from "./fakes/claude.mjs";
import { answer } from "./fakes/gh.mjs";

const claude = join(dirname(fileURLToPath(import.meta.url)), "fakes", "claude.mjs");
const run = (args, input = "") => spawnSync(process.execPath, [claude, ...args], { input, encoding: "utf8" });

test("fill gives the smallest value a JSON schema accepts", () => {
  const schema = {
    type: "object",
    required: ["kind", "items", "note", "n", "ref"],
    properties: {
      kind: { enum: ["questions", "done"] },
      items: { type: "array", minItems: 1, items: { type: "string", minLength: 2 } },
      note: { type: ["string", "null"] },
      n: { type: "integer", minimum: 3 },
      ref: { $ref: "#/$defs/flag" },
      optional: { type: "string" },
    },
    $defs: { flag: { anyOf: [{ type: "null" }, { type: "boolean" }] } },
  };
  assert.deepEqual(fill(schema), { kind: "questions", items: ["xx"], note: "", n: 3, ref: false });
});

test("an option's value is never taken for the prompt", () => {
  assert.equal(positional(["-p", "--max-budget-usd", "0.25", "--output-format", "json"]), undefined);
  assert.equal(positional(["-p", "--model", "haiku", "hello"]), "hello");
  assert.equal(positional(["-p", "--allowedTools", "Read", "Grep", "--", "the prompt"]), "the prompt");
});

test("the fake claude answers --version and a structured -p call", () => {
  assert.equal(run(["--version"]).stdout.trim(), "2.1.330 (Claude Code)");
  const schema = '{"type":"object","required":["ok"],"properties":{"ok":{"type":"boolean"}}}';
  const out = run(["-p", "--output-format", "json", "--max-budget-usd", "0.25", "--json-schema", schema], "from stdin");
  const result = JSON.parse(out.stdout.trim().split("\n").at(-1));
  assert.equal(result.type, "result");
  assert.deepEqual(result.structured_output, { ok: false });
  assert.match(result.result, /from stdin/);
});

test("the fake claude runs a stream-json conversation", () => {
  const input = JSON.stringify({ type: "user", message: { role: "user", content: [{ type: "text", text: "hello" }] } }) + "\n";
  const lines = run(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--session-id", "s1"], input)
    .stdout.trim()
    .split("\n")
    .map((l) => JSON.parse(l));
  assert.deepEqual(lines.map((l) => l.type), ["system", "assistant", "result"]);
  assert.equal(lines[2].session_id, "s1");
  assert.match(lines[1].message.content[0].text, /hello/);
});

test("a slow turn streams its words, then a tool call, then the end of its answer", async () => {
  const lines = [];
  await slowTurn("s1", "go slowly", (l) => lines.push(l), 0);
  const kinds = lines.map((l) => l.message?.content?.[0]?.type ?? l.event?.delta?.type ?? l.subtype ?? l.type);
  const first = kinds.indexOf("tool_use");
  assert.equal(kinds[0], "init");
  assert.ok(kinds.slice(1, first - 1).every((k) => k === "text_delta") && first > 3);
  assert.deepEqual(kinds.slice(first - 1, first + 2), ["text", "tool_use", "tool_result"]);
  assert.equal(lines[first].message.content[0].input.command, "npm test");
  assert.equal(kinds.at(-1), "success");
});

test("the fake gh finds the issue assigned to alex", () => {
  const [out, code] = answer(["api", "--include", "--method", "GET", "search/issues", "-f", "q=is:issue repo:acme/api", "-f", "page=1"]);
  assert.equal(code, 0);
  const body = JSON.parse(out.split("\r\n\r\n")[1]);
  assert.equal(body.items[0].title, "Fix the login button");
});

test("the fake curl answers the release list only, never another host", () => {
  const curl = join(dirname(fileURLToPath(import.meta.url)), "fakes", "curl.mjs");
  const other = spawnSync(process.execPath, [curl, "-fsSL", "https://example.com"], { encoding: "utf8", env: {} });
  assert.equal(other.status, 6);
  const missing = spawnSync(process.execPath, [curl, "https://api.github.com/repos/vassilidev/TermF/releases?per_page=30"], { encoding: "utf8", env: {} });
  assert.equal(missing.status, 22);
});

test("totp matches RFC 6238's test vector", async () => {
  const { totp } = await import("./scenarios.mjs");
  // "12345678901234567890" in base32, at 59 s: 94287082 on 8 digits.
  assert.equal(totp("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", 59_000), "287082");
});

test("a real Claude Code reached from a sandbox never updates itself", async () => {
  const { createSandbox } = await import("./sandbox.mjs");
  const box = createSandbox({ project: false });
  try {
    assert.equal(box.env.DISABLE_AUTOUPDATER, "1");
  } finally {
    box.cleanup();
  }
});
