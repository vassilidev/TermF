import { test } from "node:test";
import assert from "node:assert/strict";
import { blank, clean, fresh, groupOf, keyOf, lines, missing, parseKey, stateOf } from "../src/lib/devenv-logic.ts";

const entry = (manager, name, more = {}) => ({ ...blank(manager), name, ...more });
const found = (manager, name, more = {}) => ({ ...entry(manager, name, more), installed: null });

test("an agent names an entry manager:name, and only a known manager", () => {
  assert.equal(keyOf(entry("brew", "ripgrep")), "brew:ripgrep");
  assert.deepEqual(parseKey("npm:@scope/pkg"), { manager: "npm", name: "@scope/pkg" });
  assert.deepEqual(parseKey("manual:Visual Studio Code"), { manager: "manual", name: "Visual Studio Code" });
  assert.equal(parseKey("apt:curl"), null);
  assert.equal(parseKey("ripgrep"), null);
  assert.equal(parseKey("brew:"), null);
  assert.equal(groupOf("mise"), "languages");
  assert.equal(groupOf("cask"), "apps");
  assert.equal(groupOf("vscode"), "editors");
});

test("what is new is what the manifest lacks, once each", () => {
  const list = [entry("brew", "ripgrep")];
  const scan = [found("brew", "ripgrep"), found("npm", "typescript"), found("npm", "typescript"), found("manual", "mytool")];
  assert.deepEqual(
    fresh(scan, list).map(keyOf),
    ["npm:typescript", "manual:mytool"],
  );
});

test("missing only once a detection is over, and only what goes in the script", () => {
  const list = [entry("brew", "ripgrep"), entry("npm", "typescript"), entry("cargo", "bat", { active: false })];
  const scan = [found("brew", "ripgrep")];
  assert.equal(stateOf(list[0], scan, false), "there");
  assert.equal(stateOf(list[1], scan, false), null, "not yet known while the detection runs");
  assert.equal(stateOf(list[1], scan, true), "missing");
  assert.deepEqual(missing(list, scan, false), []);
  assert.deepEqual(missing(list, scan, true).map(keyOf), ["npm:typescript"], "an entry turned off is not missing");
});

test("the form saves trimmed texts and drops what its manager has no use for", () => {
  const saved = clean(entry("npm", "  typescript ", { version: " ", install: "curl x | sh", path: "~/x", id: "1" }));
  assert.deepEqual(
    [saved.name, saved.version, saved.install, saved.path, saved.id],
    ["typescript", null, null, null, null],
  );
  const own = clean(entry("shell", "claude", { install: " curl -fsSL https://claude.ai/install.sh | bash " }));
  assert.equal(own.install, "curl -fsSL https://claude.ai/install.sh | bash");
  assert.equal(clean(entry("mas", "Xcode", { id: " 497799835 " })).id, "497799835");
});

test("the agent reads one line per entry", () => {
  assert.equal(
    lines([
      { ...found("brew", "ripgrep", { description: "Search tool" }), installed: "14.1.0" },
      entry("manual", "mytool", { path: "~/.local/bin/mytool", active: false }),
    ]),
    "brew:ripgrep 14.1.0 (Search tool)\nmanual:mytool ~/.local/bin/mytool [off]",
  );
});
