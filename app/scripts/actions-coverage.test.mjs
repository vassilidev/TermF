import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { EXCLUDED, READS, THROUGH } from "../src/lib/actions-coverage.ts";
import { CHANGE_ACTIONS, READ_ACTIONS, gate } from "../src/lib/actions-logic.ts";

const lib = new URL("../src/lib/", import.meta.url);
const read = (name) => readFileSync(new URL(name, lib), "utf8");

/** Every command the app registers, as `generate_handler!` lists them. */
const handlers = (() => {
  const rust = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  const lines = rust
    .slice(rust.indexOf("generate_handler!["))
    .split("\n")
    .slice(1)
    .map((l) => l.trim());
  return lines
    .slice(0, lines.findIndex((l) => l.startsWith("]")))
    .map((l) => l.replace(/,$/, ""))
    .filter((l) => /^[a-z_:0-9]+$/.test(l))
    .map((l) => l.split("::").pop());
})();

const actionFiles = readdirSync(lib).filter((f) => f.endsWith("-actions.ts"));
const actionsCode = actionFiles.map(read).join("\n");
const INVOKE = /invoke(?:<[^(]*?>)?\(\s*"([a-z_0-9]+)"/g;
const ID = /\bid: "([a-z]+\.[a-z_]+)"/g;
/** The commands the actions invoke themselves. */
const invoked = new Set([...actionsCode.matchAll(INVOKE)].map((m) => m[1]));
const ids = new Set([...actionsCode.matchAll(ID)].map((m) => m[1]));
/** Each action's code: from its id to the next action's id in its file (a helper written after it counts as its own). */
const blocks = new Map(
  actionFiles.flatMap((f) => {
    const code = read(f);
    const at = [...code.matchAll(ID)];
    return at.map((m, i) => [m[1], code.slice(m.index, at[i + 1]?.index)]);
  }),
);

test("every command of the app is covered by an action, only reads, or is excluded with why", () => {
  assert.ok(handlers.length > 300, "the handlers were read from lib.rs");
  const bucket = new Map();
  const put = (command, where) => {
    assert.ok(!bucket.has(command), `${command} is both ${bucket.get(command)} and ${where}`);
    bucket.set(command, where);
  };
  for (const c of READS) put(c, "a read");
  for (const c of Object.keys(THROUGH)) put(c, `run through ${THROUGH[c]}`);
  for (const [why, commands] of EXCLUDED) {
    assert.ok(why.length > 20, `a real reason for ${commands.join(", ")}`);
    for (const c of commands) put(c, "excluded");
  }
  // An action may read first (a read stays a read); what it changes is covered by it.
  for (const c of invoked) if (!READS.includes(c)) put(c, "invoked by an action");
  const missing = handlers.filter((c) => !bucket.has(c));
  assert.deepEqual(missing, [], "a new command: give it an action, or put it in READS or EXCLUDED with why (actions-coverage.ts)");
  const unknown = [...bucket.keys()].filter((c) => !handlers.includes(c));
  assert.deepEqual(unknown, [], "listed but not a command of the app any more");
});

test("what runs through a screen's code names an action that exists", () => {
  for (const [command, action] of Object.entries(THROUGH)) assert.ok(ids.has(action), `${command}: no action ${action}`);
});

test("a prerequisite names an action that settles it", () => {
  const code = actionsCode + read("actions-logic.ts");
  const named = [...code.matchAll(/propose ([a-z]+\.[a-z_]+)/g)].map((m) => m[1]);
  assert.ok(named.includes("vault.create") && named.includes("vault.unlock") && named.includes("ops.add_connection"));
  for (const action of named) assert.ok(ids.has(action), `a prerequisite names ${action}, which is no action`);
});

test("every action's card says its title and what it does in French and English", () => {
  const i18n = read("i18n.ts");
  const en = i18n.indexOf("const en:");
  for (const [, key] of actionsCode.matchAll(/label: "([A-Za-z0-9]+)"/g)) {
    const at = (from, to) => i18n.slice(from, to).includes(`\n  ${key}:`);
    assert.ok(at(0, en) && at(en), `${key} in fr and en`);
  }
});

test("an action that runs without a card only reads: it invokes no command but reads", () => {
  for (const id of READ_ACTIONS) {
    assert.ok(blocks.has(id), `${id} is no action`);
    for (const [, command] of blocks.get(id).matchAll(INVOKE))
      assert.ok(READS.includes(command), `${id} runs without a card, yet invokes ${command}, which is not in READS`);
  }
});

test("an action nobody classified asks every time, and so do deletions, the vault, prod and what agents may do", () => {
  assert.equal(gate("ops.brand_new"), "ask", "safe by default");
  for (const id of CHANGE_ACTIONS) assert.ok(ids.has(id) && !READ_ACTIONS.includes(id), `${id}: an action, in one list only`);
  const always = /\.(delete|remove|uninstall)|^vault\.|^settings\.(set_sensitive|claude_)/;
  for (const id of ids) if (always.test(id)) assert.equal(gate(id), "ask", id);
  const prodAndRights = ["ci.dispatch", "ci.redeploy", "ops.forge_deploy", "ops.nginx_reload", "ops.laravel_job", "ops.redis_write"];
  prodAndRights.push("ops.forge_account", "ops.forge_import", "git.cleanup", "project.edit", "setup.apply", "env.install", "env.save_script");
  prodAndRights.push("library.install_plugin", "library.set_plugin", "library.toggle");
  for (const id of prodAndRights) assert.ok(ids.has(id) && gate(id) === "ask", id);
});
