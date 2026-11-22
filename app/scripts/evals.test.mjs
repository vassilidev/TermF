import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { KEY, build, command, rolePrompt, suites } from "./evals.mjs";

test("every key suite has evals", () => {
  const names = suites().map((s) => s.name);
  assert.deepEqual(
    KEY.filter((k) => !names.includes(k)),
    [],
  );
});

test("a role's suite plays the role as TermF gives it, with the grants its cases ask for", () => {
  const front = suites().find((s) => s.name === "front");
  const dir = mkdtempSync(join(tmpdir(), "termf-evals-test-"));
  const needs = build(front, dir);
  assert.deepEqual(needs, { grants: ["Write", "Edit"], scaffold: true });
  assert.ok(existsSync(join(dir, "skills", "git", "SKILL.md")), "the skills of every session");
  assert.ok(!existsSync(join(dir, "skills", "git", "evals")), "no other suite's cases");
  assert.ok(!existsSync(join(dir, "skills", "react")), "no skill of a stack");

  const prompt = readFileSync(join(dir, "evals", "asks-for-the-missing-text", "prompt.md"), "utf8");
  const line = prompt.split("\n")[1];
  assert.ok(line.startsWith("append_system_prompt: "), line);
  const role = readFileSync(new URL("../../library/roles/front.md", import.meta.url), "utf8");
  assert.equal(JSON.parse(line.slice("append_system_prompt: ".length)), rolePrompt(role, "front"));
  assert.ok(rolePrompt(role, "front").startsWith("## Role: front\n\nYou are the front-end developer"));

  const args = command(dir, "out", needs, 2.5, ["--runs", "3"]);
  assert.deepEqual(args.slice(0, 3), ["plugin", "eval", dir]);
  assert.ok(args.join(" ").includes("--max-cost-usd 2.50"));
  assert.ok(args.join(" ").endsWith("--allow-tools Write Edit --scaffold --runs 3"));
});

test("a skill's suite holds that skill and leaves its prompts as they are", () => {
  const rust = suites().find((s) => s.name === "rust");
  const dir = mkdtempSync(join(tmpdir(), "termf-evals-test-"));
  assert.deepEqual(build(rust, dir), { grants: [], scaffold: false });
  assert.ok(existsSync(join(dir, "skills", "rust", "SKILL.md")));
  const prompt = readFileSync(join(dir, "evals", "does-the-task", "prompt.md"), "utf8");
  assert.ok(!prompt.includes("append_system_prompt"));
});
