import { test } from "node:test";
import assert from "node:assert/strict";
import { access, added, initials } from "../src/lib/team-logic.ts";

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
