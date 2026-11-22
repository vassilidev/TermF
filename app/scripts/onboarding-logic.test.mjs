import { test } from "node:test";
import assert from "node:assert/strict";
import {
  byTheme,
  githubIssues,
  globList,
  members,
  pick,
  reachable,
  recipeLines,
  replacesHand,
  teamEdit,
  teamKeys,
  ticketProposal,
  toReview,
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

const squad = { id: "main", lead: "lead", roles: { front: 2, qa: 1 }, repos: [], paths: [], domains: [] };
const proposal = {
  team: { squads: [squad], staff: ["architect"] },
  workflow: "default",
  roles: { qa: { writes: ["**/*.test.*"] }, front: { model: "opus" } },
};

test("team: reviewed part by part, only the accepted parts written, as changed", () => {
  assert.deepEqual(teamKeys(proposal), ["team", "workflow", "roles.qa", "roles.front"]);
  assert.deepEqual(teamKeys({ team: null, workflow: null, roles: {} }), []);
  const decisions = { team: "accepted", workflow: "rejected", "roles.qa": "accepted" };
  assert.deepEqual(teamEdit(proposal, decisions, { roles: {} }), {
    team: proposal.team,
    workflow: null,
    roles: { qa: { writes: ["**/*.test.*"] } },
  });
  const changes = { workflow: "squads", roles: { qa: { writes: ["tests/**"], effort: "high" } } };
  const changed = teamEdit(proposal, { ...decisions, workflow: "accepted" }, changes);
  assert.equal(changed.workflow, "squads");
  assert.deepEqual(changed.roles.qa, { writes: ["tests/**"], effort: "high" });
  assert.deepEqual(teamEdit(proposal, {}, changes), { team: null, workflow: null, roles: {} });
});

test("team: replacing a hand edit only when an accepted part was changed by hand", () => {
  const status = { team: "edited", workflow: "same", "roles.qa": "changed" };
  assert.equal(replacesHand({ team: proposal.team, workflow: null, roles: {} }, status), true);
  assert.equal(replacesHand({ team: null, workflow: "default", roles: { qa: {} } }, status), false);
});

test("team: a part already in place is not to review, unless decided anyway", () => {
  const keys = ["team", "workflow", "roles.qa"];
  const status = { team: "same", workflow: "new", "roles.qa": "same" };
  assert.deepEqual(toReview(keys, status, {}), ["workflow"]);
  assert.deepEqual(toReview(keys, status, { "roles.qa": "accepted" }), ["workflow", "roles.qa"]);
  assert.deepEqual(toReview(keys, { team: "same", workflow: "same", "roles.qa": "same" }, {}), []);
});

test("team: globs and members in a few words", () => {
  assert.deepEqual(globList(" apps/web/** \n\npackages/ui/**, apps/web/**,"), ["apps/web/**", "packages/ui/**"]);
  assert.deepEqual(globList("  "), []);
  assert.deepEqual(members({ front: 2, qa: 1 }), ["front ×2", "qa"]);
});
