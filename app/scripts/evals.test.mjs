import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { KEY, build, command, exitCode, main, rolePrompt, suites, withSystemPrompt } from "./evals.mjs";

test("the system prompt goes into a frontmatter, made when missing, never over a case's own", () => {
  const line = 'append_system_prompt: "## Stack"';
  assert.equal(withSystemPrompt("Do it.\n", "## Stack"), `---\n${line}\n---\n\nDo it.\n`);
  assert.equal(withSystemPrompt("---\nmax_turns: 5\n---\nDo it.\n", "## Stack"), `---\n${line}\nmax_turns: 5\n---\nDo it.\n`);
  assert.throws(() => withSystemPrompt("---\nappend_system_prompt: mine\n---\nDo it.\n", "## Stack"), /sets append_system_prompt/);
});

test("a failed suite fails the run even when the budget stopped others", () => {
  assert.equal(exitCode([{ exit: 0 }, { skipped: "budget" }]), 2);
  assert.equal(exitCode([{ exit: 1 }, { skipped: "budget" }]), 1);
  assert.equal(exitCode([{ exit: 2 }]), 2);
  assert.equal(exitCode([{ exit: null }, { exit: 0 }]), 1);
  assert.equal(exitCode([{ exit: 0 }]), 0);
});

test(
  "a suite that dies without a report reads no older one and counts all it was allowed",
  { skip: process.platform === "win32" && "a fake claude needs a POSIX shell" },
  () => {
    const bin = mkdtempSync(join(tmpdir(), "termf-evals-bin-"));
    writeFileSync(join(bin, "claude"), "#!/bin/sh\nexit 1\n");
    chmodSync(join(bin, "claude"), 0o755);
    const out = mkdtempSync(join(tmpdir(), "termf-evals-out-"));
    mkdirSync(join(out, "rust", "results"), { recursive: true });
    const old = { costUsd: 0.01, aggregates: { casesPassed: 3, casesTotal: 3 }, cases: [] };
    writeFileSync(join(out, "rust", "results", "aggregate-result.json"), JSON.stringify(old));
    const path = process.env.PATH;
    process.env.PATH = `${bin}${delimiter}${path}`;
    try {
      assert.equal(main(["rust", "python", "--budget", "1", "--out", out]), 1);
    } finally {
      process.env.PATH = path;
    }
    const summary = JSON.parse(readFileSync(join(out, "summary.json"), "utf8"));
    assert.equal(summary.spent, 1);
    assert.deepEqual(
      summary.results.map((r) => r.skipped ?? `${r.passed}/${r.total}`),
      ["0/0", "budget"],
    );
  },
);

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
  const system = JSON.parse(line.slice("append_system_prompt: ".length));
  assert.ok(system.startsWith(`${rolePrompt(role, "front")}\n\n## Stack\n\n`), system.slice(0, 200));
  assert.ok(system.includes("\n#### git\n"), "the cards of the skills it gets");
  assert.ok(!existsSync(join(dir, "skills", "legal-sources")), "not a skill of the front role");
  assert.ok(rolePrompt(role, "front").startsWith("## Role: front\n\nYou are the front-end developer"));

  const args = command(dir, "out", needs, 2.5, ["--runs", "3"]);
  assert.deepEqual(args.slice(0, 3), ["plugin", "eval", dir]);
  assert.ok(args.join(" ").includes("--max-cost-usd 2.50"));
  assert.ok(args.join(" ").endsWith("--allow-tools Write Edit --scaffold --runs 3"));
});

test("a skill's suite holds that skill, and its card in the stack section, without a role", () => {
  const rust = suites().find((s) => s.name === "rust");
  const dir = mkdtempSync(join(tmpdir(), "termf-evals-test-"));
  assert.deepEqual(build(rust, dir), { grants: [], scaffold: false });
  assert.ok(existsSync(join(dir, "skills", "rust", "SKILL.md")));
  const prompt = readFileSync(join(dir, "evals", "does-the-task", "prompt.md"), "utf8");
  const system = JSON.parse(prompt.split("\n")[1].slice("append_system_prompt: ".length));
  assert.ok(system.startsWith("## Stack\n\n"));
  assert.ok(system.includes("\n#### rust\n\n- Version:"));
  assert.ok(!system.includes("## Role"));
});
