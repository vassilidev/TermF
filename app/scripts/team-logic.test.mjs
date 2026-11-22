import { test } from "node:test";
import assert from "node:assert/strict";
import { access, added, initials, overlaps, shown, source, squadIssues } from "../src/lib/team-logic.ts";

test("a role's mark takes two letters of its name", () => {
  assert.equal(initials("front"), "FR");
  assert.equal(initials("po"), "PO");
  assert.equal(initials("dev-ops"), "DO");
  assert.equal(initials("worktree_setup"), "WS");
  assert.equal(initials("x"), "X");
  assert.equal(initials("😀-ops"), "😀O");
  assert.equal(initials("éq"), "ÉQ");
});

test("access: writes globs, read-only roles, and roles without a limit", () => {
  assert.deepEqual(access({ writes: ["apps/web/**", "packages/ui/**"] }), ["apps/web/**", "packages/ui/**"]);
  assert.deepEqual(access({ writes: "apps/web/**, packages/ui/**" }), ["apps/web/**", "packages/ui/**"]);
  assert.equal(access({ writes: [] }), "readOnly");
  assert.equal(access({ writes: "" }), "readOnly");
  assert.equal(access({ tools: "Read, Grep, Glob" }), "readOnly");
  assert.equal(access({ tools: ["Read", "Bash"] }), "readOnly");
  assert.equal(access({ tools: "Read, Edit" }), null);
  assert.equal(access({ disallowedTools: "Edit, Write, NotebookEdit" }), "readOnly");
  assert.equal(access({ disallowedTools: ["Edit", "Write"] }), "readOnly");
  assert.equal(access({ disallowedTools: "Write" }), null);
  assert.equal(access({ model: "sonnet" }), null);
});

test("the project a save created is the one that was missing", () => {
  const before = [{ id: "acme" }, { id: "docs" }];
  assert.equal(added(before, [...before, { id: "mobile" }]), "mobile");
  assert.equal(added(before, before), null);
});

test("role settings and credits, as the Team view shows them", () => {
  assert.equal(shown(["medium", "xhigh"]), "medium, xhigh");
  assert.equal(shown("opus"), "opus");
  assert.equal(shown(2), "2");
  assert.equal(source("obra/superpowers@22f0b463e7a5 (MIT): skills/x/task-reviewer-prompt.md"), "obra/superpowers (MIT)");
  assert.equal(source("acme/roles"), "acme/roles");
  assert.equal(source("https://github.com/obra/superpowers (MIT): skills/x.md"), "https://github.com/obra/superpowers (MIT)");
});

test("a squad's editor flags what is wrong, and warns of perimeters that meet", () => {
  const squad = (id, repos, paths, roles = {}) => ({ id, lead: "", roles, repos, paths, domains: [] });
  const web = squad("web", ["acme-web"], ["apps/web/**", "packages/ui"]);
  const api = squad("api", ["acme-api"], []);
  const roles = ["lead", "front", "back", "qa"];
  const repos = ["acme-web", "acme-api"];
  assert.deepEqual(squadIssues(web, [api], roles, repos), []);

  assert.equal(overlaps(web, squad("ui", ["acme-web"], ["packages/ui/button.ts"])), true, "a path under another");
  assert.equal(overlaps(web, squad("docs", ["acme-web"], ["docs/**"])), false);
  assert.equal(overlaps(web, squad("all", [], ["apps/*"])), true, "no repository named: every one");
  assert.equal(overlaps(web, squad("whole", ["acme-web"], [])), true, "no path: the whole repository");
  assert.equal(overlaps(api, squad("other", ["acme-web"], [])), false, "another repository");

  const bad = { ...squad("w b", ["acme-mobile"], ["/etc", "../up", "ok/**"], { wizard: 1, front: 0 }), lead: "chief" };
  const said = (issues) => issues.map((i) => `${i.level} ${i.kind} ${i.subject}`);
  assert.deepEqual(said(squadIssues(bad, [web], roles, repos)), [
    "error id w b",
    "error role chief",
    "error role wizard",
    "error count front",
    "error path /etc",
    "error path ../up",
    "error repo acme-mobile",
  ]);
  assert.deepEqual(said(squadIssues(squad("web", [], []), [web], roles, repos)), ["error taken web", "warning overlap web"]);
  assert.deepEqual(squadIssues({ ...squad("api", [], []), lead: "orchestrator" }, [], roles, repos), []);
});
