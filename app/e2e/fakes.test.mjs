import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { fill, positional } from "./fakes/claude.mjs";
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

test("the fake gh finds the issue assigned to alex", () => {
  const [out, code] = answer(["api", "--include", "--method", "GET", "search/issues", "-f", "q=is:issue repo:acme/api", "-f", "page=1"]);
  assert.equal(code, 0);
  const body = JSON.parse(out.split("\r\n\r\n")[1]);
  assert.equal(body.items[0].title, "Fix the login button");
});
