import { test } from "node:test";
import assert from "node:assert/strict";
import { cloneTarget, compareRows, expandHome, inside, previewGroups, scopeOf, shownPath } from "../src/lib/sync-logic.ts";

test("the preview of a join sorts each file by scope, what needs a look first", () => {
  const groups = previewGroups(
    {
      pulled: ["library/commands/a.md", "termf.toml", "ssh/keys/id_ed25519-2", "ssh/config"],
      pushed: ["projects/web.toml", "termf.toml", "ssh/config"],
      merged: ["termf.toml", "ssh/config"],
      renamed: [{ path: "ssh/keys/id_ed25519", to: "ssh/keys/id_ed25519-2" }],
      conflicts: [{ path: "projects/acme.toml", keys: ["name"] }],
    },
    ["config", "projects", "sshKeys"],
  );
  assert.deepEqual(
    groups.map((g) => [g.scope, g.items.map((i) => [i.path, i.change])]),
    [
      ["config", [["termf.toml", "merged"], ["library/commands/a.md", "added"]]],
      ["projects", [["projects/acme.toml", "conflict"], ["projects/web.toml", "sent"]]],
      ["sshKeys", [["ssh/config", "merged"], ["ssh/keys/id_ed25519", "kept"]]],
    ],
  );
  assert.equal(groups[2].items[1].to, "ssh/keys/id_ed25519-2");
  assert.equal(scopeOf("projects/acme/conventions/style/source.md"), "conventions");
  assert.equal(shownPath("ssh/keys/id_work"), "id_work");
  assert.equal(shownPath("ssh/known_hosts"), "~/.ssh/known_hosts");
  assert.equal(shownPath("dev/claude/settings.json"), "~/.claude/settings.json");
  assert.equal(shownPath("dev/claude/memory/acme/MEMORY.md"), "dev/claude/memory/acme/MEMORY.md");
});

test("a missing repository goes beside the project's others", () => {
  const home = "/Users/alex";
  assert.equal(cloneTarget("~/work/acme/api", ["/Users/alex/code/acme/web"], home), "/Users/alex/code/acme/api");
  assert.equal(cloneTarget("~/work/acme/api", ["~/src/acme/web/"], home), "/Users/alex/src/acme/api");
  assert.equal(cloneTarget("~/work/acme/api", [], home), "/Users/alex/work/acme/api");
  assert.equal(cloneTarget("/srv/acme/api", [], home), "/srv/acme/api");
  assert.equal(cloneTarget("~/acme/api", ["C:\\Users\\alex\\code\\web"], "C:\\Users\\alex"), "C:\\Users\\alex\\code\\api");
});

test("a chosen folder gets the repository's own name", () => {
  assert.equal(inside("/Volumes/data/", "~/work/acme/api"), "/Volumes/data/api");
  assert.equal(inside("D:\\src", "~/acme/billing-service"), "D:\\src\\billing-service");
  assert.equal(expandHome("~", "/home/alex"), "/home/alex/");
  assert.equal(expandHome("~/a/b", "C:\\Users\\alex"), "C:\\Users\\alex\\a\\b");
  assert.equal(expandHome("~alex/a", "/home/alex"), "~alex/a");
});

test("the comparison lists every synced scope, the same as the sync or what differs", () => {
  const rows = compareRows(
    {
      pulled: ["library/commands/b.md", "vault/vault.json", "ssh/keys/id_ed25519-2"],
      pushed: ["projects/acme.toml", "termf.toml"],
      merged: [],
      renamed: [{ path: "ssh/keys/id_ed25519", to: "ssh/keys/id_ed25519-2" }],
      conflicts: [{ path: "projects/web.toml", keys: ["name"] }],
    },
    ["config", "projects", "conventions", "vault", "sshKeys"],
  );
  assert.deepEqual(
    rows.map((r) => [r.scope, r.counts]),
    [
      ["config", { conflict: 0, merge: 0, receive: 1, send: 1 }],
      ["projects", { conflict: 1, merge: 0, receive: 0, send: 1 }],
      ["conventions", { conflict: 0, merge: 0, receive: 0, send: 0 }],
      ["vault", { conflict: 0, merge: 0, receive: 1, send: 0 }],
      ["sshKeys", { conflict: 0, merge: 0, receive: 1, send: 0 }],
    ],
  );
  assert.deepEqual(rows[2].items, []);
});

test("nothing is proposed before the home folder is known", () => {
  assert.equal(expandHome("~/work/acme/api", ""), "");
  assert.equal(cloneTarget("~/work/acme/api", [], ""), "");
  assert.equal(cloneTarget("~/work/acme/api", ["~/code/acme/web"], ""), "");
  assert.equal(cloneTarget("~/work/acme/api", ["/srv/acme/web"], ""), "/srv/acme/api");
  assert.equal(cloneTarget("/srv/acme/api", [], ""), "/srv/acme/api");
});
