import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { fileURLToPath } from "node:url";
import { compile, compileModule } from "svelte/compiler";

const src = fileURLToPath(new URL("../src/", import.meta.url));
const files = readdirSync(src, { recursive: true }).filter((f) => /\.svelte(\.ts)?$/.test(f));
const read = (f) => readFileSync(src + f, "utf8");
const components = files.filter((f) => f.endsWith(".svelte"));

/** Each file compiled the way `vite dev` does, so what the dev console would say about it shows here. */
const dev = (f) => {
  if (f.endsWith(".svelte")) return compile(read(f), { dev: true, filename: f }).js.code;
  // `strip` blanks the types out in place: lines and columns stay those of the source.
  return compileModule(stripTypeScriptTypes(read(f), { mode: "strip" }), { dev: true, filename: f }).js.code;
};

test("no assignment's value is read back from reactive state (assignment_value_stale)", () => {
  assert.ok(files.length > 100, "the Svelte files were found");
  const found = files.flatMap((f) => [...dev(f).matchAll(/\$\.assign(?:_async)?\(.*?:(\d+:\d+)'\)/g)].map((m) => `${f}:${m[1]}`));
  assert.deepEqual(found, [], "assign in a statement (`{ a.b = x; }`): the assignment's value is not the proxy kept in state");
});

/** Components that hear an agent's events without being its thread, and why. */
const NOT_A_THREAD = {
  "lib/RunTell.svelte": "a message to the team's lead or a step, with its reply in one line and a link to the conversation",
};

test("an agent's thread is drawn by Conversation.svelte only", () => {
  const hearing = components.filter((f) => /"agent-event"|"agent_events"/.test(read(f)));
  assert.ok(hearing.includes("lib/Conversation.svelte"), "the events were looked for where they are");
  for (const [f, why] of Object.entries(NOT_A_THREAD)) assert.ok(hearing.includes(f) && why.length > 20, `${f} is still listed for a reason`);
  const own = hearing.filter((f) => f !== "lib/Conversation.svelte" && !(f in NOT_A_THREAD));
  assert.deepEqual(own, [], "show the agent with <Conversation> (bare in a panel) instead of a thread of its own");
});

/** Empty states with nothing to do, by the text key they show, and why that is right. */
const RESTING = {
  gitclEmptyTitle: "nothing left to clean up: the repository is tidy",
  watchTodoEmpty: "nothing left to do: the library is up to date",
  claudeViewNoArtifacts: "the refresh from the account is in the header right above",
  claudeViewNone: "the hook list's add button sits right under it",
  claudeDgNoSessionsRepo: "Claude Code's own sessions, which come from using it",
  libraryEmpty: "the kind filters right above list only the kinds that have items",
  opsforgeNone: "servers are made in Forge; reading the account again is in the header",
  tjdNone: "the filters and the search are right above",
};

test("an empty state says what to do next: an action, asking the AI, or its own buttons", () => {
  const stuck = [];
  let seen = 0;
  for (const f of components) {
    for (const m of read(f).matchAll(/<EmptyState\b([\s\S]*?)(\/>|>([\s\S]*?)<\/EmptyState>)/g)) {
      seen++;
      const [, attrs, , children = ""] = m;
      if (/\baction=|\bonask=/.test(attrs) || children.trim()) continue;
      const key = attrs.match(/t\("(\w+)"/)?.[1];
      if (!key || !(key in RESTING)) stuck.push(`${f}:${read(f).slice(0, m.index).split("\n").length} ${key ?? ""}`);
    }
  }
  assert.ok(seen > 30, "the empty states were found");
  assert.deepEqual(stuck, [], "give it an `action`, or `onask` opening ⌘I with the right role and a question ready");
});
