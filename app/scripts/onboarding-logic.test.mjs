import { test } from "node:test";
import assert from "node:assert/strict";
import {
  byTheme,
  githubIssues,
  pick,
  reachable,
  recipeLines,
  ticketProposal,
  trustState,
} from "../src/lib/onboarding-logic.ts";

const q = (id, theme, kind, options = []) => ({ id, theme, text: id, kind, options, default: [] });

test("questions are grouped by theme in a fixed order", () => {
  const list = [q("a", "agents", "bool"), q("b", "quality", "bool"), q("c", "agents", "text")];
  assert.deepEqual(
    byTheme(list).map(([theme, qs]) => [theme, qs.map((x) => x.id)]),
    [
      ["quality", ["b"]],
      ["agents", ["a", "c"]],
    ],
  );
  assert.deepEqual(byTheme([]), []);
});

test("picking an option: one for a choice, ticked or not in the options' order for a multiple choice", () => {
  const choice = q("c", "tests", "choice", ["x", "y"]);
  assert.deepEqual(pick(choice, ["x"], "y"), ["y"]);
  assert.deepEqual(pick(q("b", "tests", "bool"), ["yes"], "no"), ["no"]);
  const multi = q("m", "quality", "multi", ["lint", "tests", "types"]);
  assert.deepEqual(pick(multi, ["types"], "lint"), ["lint", "types"]);
  assert.deepEqual(pick(multi, ["lint", "types"], "lint"), ["types"]);
  assert.deepEqual(pick(multi, [], "tests"), ["tests"]);
});

test("steps: back to any step reached, never forward past the assistant, nothing before the analysis", () => {
  assert.equal(reachable("analysis", "analysis", false), true);
  assert.equal(reachable("questions", "analysis", false), false);
  assert.equal(reachable("questions", "proposals", true), true);
  assert.equal(reachable("trust", "proposals", true), false);
  assert.equal(reachable("done", "done", true), true);
});

test("trust: every repository, some or none", () => {
  const r = (trusted) => ({ name: "a", path: "/a", root: "/a", trusted, allow: 0, mcp: 0 });
  assert.equal(trustState([r(true), r(true)]), "on");
  assert.equal(trustState([r(true), r(false)]), "partial");
  assert.equal(trustState([r(false)]), "off");
  assert.equal(trustState([]), "off");
});

test("tickets: saved sources stay, GitHub Issues proposed from the remotes, nothing to guess otherwise", () => {
  const implicit = { github: null, jira: null, mcp: {}, implicit: true };
  assert.equal(ticketProposal(implicit, ["acme/api"]), "github");
  assert.equal(ticketProposal(implicit, []), "none");
  assert.equal(ticketProposal({ ...implicit, implicit: false }, []), "saved");
  const written = githubIssues(implicit);
  assert.equal(written.implicit, false);
  assert.equal(written.github.issues, true);
  assert.equal(written.github.mine, true);
  assert.equal(written.jira, null);
});

test("a recipe in a few words per repository", () => {
  const lines = recipeLines({
    api: {
      stack: ["node:pnpm"],
      include: [".env"],
      deps: [{ dir: "node_modules", clone: true, install: ["pnpm install --offline", "pnpm install"] }],
      database: ".env",
      compose: false,
      ports: ["web"],
      commands: ["pnpm db:migrate"],
    },
    docs: { stack: [], include: [], deps: [], compose: false, ports: [], commands: [] },
  });
  assert.deepEqual(lines, [
    { repo: "api", parts: ["pnpm install", ".env", ".env", "TERMF_PORT_WEB", "pnpm db:migrate"] },
    { repo: "docs", parts: [] },
  ]);
});
