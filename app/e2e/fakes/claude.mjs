// A stand-in for Claude Code 2.1.330, for the end-to-end runs: no network, no account. Headless (`-p`) it answers
// in json or stream-json, with a `structured_output` that fits `--json-schema`; in a terminal it runs the hooks
// of its `--plugin-dir` and `--settings` as the real one does, so TermF sees its status.
import { spawnSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { createInterface } from "node:readline";

const VERSION = "2.1.330";
const args = process.argv.slice(2);
const flag = (name) => {
  const i = args.lastIndexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};
const all = (name) => args.flatMap((a, i) => (a === name ? [args[i + 1]] : []));
const say = (value) => process.stdout.write(JSON.stringify(value) + "\n");
if (process.env.TERMF_E2E_LOG) appendFileSync(process.env.TERMF_E2E_LOG, `claude ${JSON.stringify(args)}\n`);

/** The smallest value that `schema` accepts: required properties, first enum value, minimum lengths. */
export function fill(schema, root = schema) {
  if (!schema || typeof schema !== "object") return null;
  if (schema.$ref) return fill(schema.$ref.split("/").slice(1).reduce((s, k) => s?.[k], root), root);
  if ("const" in schema) return schema.const;
  if (schema.enum) return schema.enum[0];
  if ("default" in schema) return schema.default;
  const alt = schema.anyOf ?? schema.oneOf;
  if (alt) return fill(alt.find((s) => s.type !== "null") ?? alt[0], root);
  if (schema.allOf) return Object.assign({}, ...schema.allOf.map((s) => fill(s, root)));
  const types = [schema.type ?? (schema.properties ? "object" : schema.items ? "array" : "string")].flat();
  switch (types.find((t) => t !== "null") ?? "null") {
    case "object": {
      const props = schema.properties ?? {};
      const keys = schema.required ?? Object.keys(props);
      return Object.fromEntries(keys.map((k) => [k, fill(props[k] ?? {}, root)]));
    }
    case "array":
      return Array.from({ length: schema.minItems ?? 0 }, () => fill(schema.items ?? {}, root));
    case "string":
      return "x".repeat(schema.minLength ?? 0);
    case "integer":
    case "number":
      return schema.minimum ?? 0;
    case "boolean":
      return false;
    default:
      return null;
  }
}

/** What the agents of the team's recette (PR #131) ran in their steps, each of which asked the user then. */
export const RECETTE = [
  "git status --short; echo ---; cat README.md; npm test | tail",
  "git diff HEAD~1 -- src",
  "git log --oneline -5",
  "ls -la src",
  "cat package.json | jq .scripts",
  "grep -rn greet src",
  "find . -name '*.js' -not -path './node_modules/*'",
  "npm test 2>&1 | tail -20",
  "wc -l src/app.js",
  "head -n 20 README.md",
  "npm install left-pad",
  "curl -s https://example.com",
];

/**
 * A step of a team run, as an agent meets it: the skills its brief lists (and whether its plugin has them), whether
 * the brief and its role tell it to use them, and which of the recette's commands its PreToolUse hooks still leave
 * to the user (allowed: a hook allows it and none refuses or asks). One line in the log.
 */
function teamStep(prompt) {
  const key = prompt.match(/^Step `([^`]+)`/)?.[1];
  const section = prompt.split("\n## Your skills\n")[1]?.split("\n## ")[0] ?? "";
  const skills = [...section.matchAll(/^- `([^`]+)`: /gm)].map((m) => m[1]);
  const missing = skills.filter((n) => !all("--plugin-dir").some((d) => existsSync(`${d}/skills/${n}/SKILL.md`)));
  const told = section.includes("invoke with the Skill tool");
  const prompt_file = flag("--append-system-prompt-file");
  const role = !!prompt_file && existsSync(prompt_file) && readFileSync(prompt_file, "utf8").includes("Start from your skills");
  const pre = hooks().PreToolUse ?? [];
  const asked = RECETTE.filter((command) => {
    const input = JSON.stringify({ session_id: "e2e", cwd: process.cwd(), hook_event_name: "PreToolUse", tool_name: "Bash", tool_input: { command } });
    const outs = pre.map((h) => spawnSync(h.command, h.args ?? [], { input, encoding: "utf8", timeout: (h.timeout ?? 60) * 1000 }));
    const said = (d) => outs.some((o) => o.stdout?.includes(`"permissionDecision":"${d}"`));
    return !said("allow") || said("ask") || outs.some((o) => o.status === 2);
  });
  const line = `team-step ${key} skills=${skills.join(",") || "none"} missing=${missing.join(",") || "none"} told=${told} role=${role} asks=${asked.length}/${RECETTE.length} ${asked.join(" | ")}`;
  if (process.env.TERMF_E2E_LOG) appendFileSync(process.env.TERMF_E2E_LOG, `${line}\n`);
}

/** A plan of one task for the back-end developer, for a planner's schema. */
const PLAN = {
  summary: "Fix the greeting.",
  acceptance: [{ id: "AC1", text: "The greeting says hello." }],
  out_of_scope: [],
  questions: [],
  tasks: [{ id: "T1", role: "back", squad: "", repo: "", size: "S", covers: ["AC1"], goal: "Fix the greeting in src/app.js", files_hint: ["src/app.js"], depends_on: [], ui_impact: false }],
};

const usage = { input_tokens: 5, output_tokens: 7, cache_read_input_tokens: 900, cache_creation_input_tokens: 100 };

function turn(sid, prompt, schema) {
  const text = `Fake Claude received: ${prompt}`.slice(0, 200);
  const result = { type: "result", subtype: "success", is_error: false, result: text, total_cost_usd: 0.001, usage, session_id: sid };
  if (schema) result.structured_output = schema.properties?.tasks && schema.properties?.acceptance ? PLAN : fill(schema);
  return [
    { type: "system", subtype: "init", session_id: sid, model: "claude-haiku-5-5", permissionMode: "default", tools: [], mcp_servers: [] },
    { type: "assistant", message: { model: "claude-haiku-5-5", content: [{ type: "text", text }], usage }, parent_tool_use_id: null, session_id: sid },
    result,
  ];
}

const pause = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * A turn asked to go "slowly" streams as the real one does: its words one at a time, then a Bash call that takes a
 * while, then the end of its answer. `delay` (ms) paces the words; the call lasts 30 of them.
 */
export async function slowTurn(sid, prompt, write = say, delay = 60) {
  const main = { parent_tool_use_id: null, session_id: sid };
  const words = async (text) => {
    for (const word of text.match(/\S+\s*/g)) {
      write({ type: "stream_event", event: { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: word } }, ...main });
      await pause(delay);
    }
    write({ type: "assistant", message: { model: "claude-haiku-5-5", content: [{ type: "text", text }], usage }, ...main });
  };
  write({ type: "system", subtype: "init", session_id: sid, model: "claude-haiku-5-5", permissionMode: "default", tools: [], mcp_servers: [] });
  await pause(delay * 5);
  await words(`Fake Claude streams an answer to: ${prompt}. First, the tests.`.slice(0, 200));
  const call = { type: "tool_use", id: "toolu_e2e", name: "Bash", input: { command: "npm test", description: "Run the tests" } };
  write({ type: "assistant", message: { model: "claude-haiku-5-5", content: [call], usage }, ...main });
  await pause(delay * 30);
  write({ type: "user", message: { role: "user", content: [{ tool_use_id: "toolu_e2e", type: "tool_result", content: "12 passing", is_error: false }] }, ...main });
  const end = "The tests pass.";
  await words(end);
  write({ type: "result", subtype: "success", is_error: false, result: end, total_cost_usd: 0.002, usage, session_id: sid });
}

// Options that take a value, so that value is never taken for the prompt (the variadic ones end at `--`).
const VALUED = new Set(
  "--output-format --input-format --json-schema --model --fallback-model --effort --max-budget-usd --max-turns --session-id --resume --settings --plugin-dir --append-system-prompt --append-system-prompt-file --system-prompt --permission-mode --permission-prompt-tool --allowedTools --allowed-tools --disallowedTools --disallowed-tools --tools --add-dir --mcp-config --remote-control".split(" "),
);

/** The prompt given as an argument: after `--`, else the last word that is neither an option nor its value. */
export function positional(list = args) {
  const dash = list.indexOf("--");
  if (dash >= 0) return list.slice(dash + 1).join(" ") || undefined;
  let found;
  for (let i = 0; i < list.length; i++) {
    if (VALUED.has(list[i])) i++;
    else if (!list[i].startsWith("-")) found = list[i];
  }
  return found;
}

function promptOf(line) {
  const content = JSON.parse(line).message?.content;
  return typeof content === "string" ? content : (content ?? []).map((c) => c.text ?? "").join(" ");
}

async function headless() {
  const sid = flag("--session-id") ?? flag("--resume") ?? randomUUID();
  const schemaText = flag("--json-schema");
  const schema = schemaText ? JSON.parse(schemaText) : null;
  if (flag("--input-format") === "stream-json") {
    for await (const line of createInterface({ input: process.stdin })) {
      if (line.includes('"control_request"')) {
        const id = JSON.parse(line).request_id;
        say({ type: "control_response", response: { subtype: "success", request_id: id, response: {} } });
      } else if (line.includes('"type":"user"')) {
        const prompt = promptOf(line);
        if (prompt.startsWith("Step `")) teamStep(prompt);
        if (/\bslowly\b/i.test(prompt)) await slowTurn(sid, prompt);
        else turn(sid, prompt, schema).forEach(say);
      }
    }
    return;
  }
  const prompt = positional() ?? readFileSync(0, "utf8");
  const events = turn(sid, prompt.trim(), schema);
  // "slowly": a file read that takes a while before the answer, as a real one-shot run streams it.
  if (flag("--output-format") === "stream-json" && /\bslowly\b/i.test(prompt)) {
    say({ type: "assistant", message: { content: [{ type: "tool_use", id: "toolu_read", name: "Read", input: { file_path: "README.md" } }] } });
    await pause(3000);
  }
  if (flag("--output-format") === "stream-json") events.forEach(say);
  else if (flag("--output-format") === "json") say(events.at(-1));
  else process.stdout.write(events.at(-1).result + "\n");
}

/** Hooks of every `--plugin-dir` and `--settings` file, by event. */
function hooks() {
  const files = [...all("--plugin-dir").map((d) => `${d}/hooks/hooks.json`), ...all("--settings")];
  const out = {};
  for (const file of files) {
    let json;
    try {
      // `--settings` takes a file or the JSON itself.
      json = JSON.parse(file.trim().startsWith("{") ? file : readFileSync(file, "utf8"));
    } catch {
      continue;
    }
    for (const [event, groups] of Object.entries(json.hooks ?? {})) {
      for (const group of groups) (out[event] ??= []).push(...(group.hooks ?? []));
    }
  }
  return out;
}

function interactive() {
  const sid = flag("--session-id") ?? flag("--resume") ?? randomUUID();
  const registered = hooks();
  const fire = (event, extra = {}) => {
    const payload = JSON.stringify({ session_id: sid, cwd: process.cwd(), hook_event_name: event, transcript_path: "", ...extra });
    const answers = [];
    for (const h of registered[event] ?? []) {
      const options = { input: payload, timeout: (h.timeout ?? 60) * 1000, stdio: ["pipe", "pipe", "ignore"], encoding: "utf8" };
      const out = h.args ? spawnSync(h.command, h.args, options) : spawnSync(h.command, { ...options, shell: true });
      try {
        const answer = JSON.parse(out.stdout);
        if (answer && typeof answer === "object") answers.push(answer);
      } catch {
        // A hook that prints nothing, or no JSON, answers nothing.
      }
    }
    return answers;
  };
  process.stdout.write(`Fake Claude Code ${VERSION}\r\n> `);
  fire("SessionStart", { source: flag("--resume") ? "resume" : "startup" });
  const lines = createInterface({ input: process.stdin });
  // TermF's hidden deletion: the Artifact tool asks before each delete whatever PreToolUse said, unless a
  // PermissionRequest hook allows it; a question nobody answers waits in the terminal (2.1.330).
  const first = positional();
  const doomed = first?.startsWith("Delete these claude.ai artifacts") ? (first.match(/https:\/\/claude\.ai\/[\w/-]+/g) ?? []) : [];
  const deleted = [];
  const deleteNext = () => {
    while (doomed.length) {
      const url = doomed[0];
      const call = { tool_name: "Artifact", tool_input: { action: "delete", url } };
      if (fire("PreToolUse", call).some((a) => a.hookSpecificOutput?.permissionDecision === "deny")) {
        doomed.shift();
        continue;
      }
      if (!fire("PermissionRequest", call).some((a) => a.hookSpecificOutput?.decision?.behavior === "allow")) {
        process.stdout.write(`Permanently delete ${url}? No / Yes\r\n`);
        fire("Notification", { notification_type: "permission_prompt", message: "Claude needs your permission" });
        return;
      }
      doomed.shift();
      fire("PostToolUse", { ...call, tool_response: { artifact_delete: { url, deleted: true } } });
      deleted.push(url);
    }
    const answer = JSON.stringify({ deleted, failed: [] });
    process.stdout.write(`● ${answer}\r\n> `);
    fire("Stop", { last_assistant_message: answer, stop_hook_active: false });
  };
  if (doomed.length) deleteNext();
  lines.on("line", (line) => {
    const prompt = line.trim();
    if (doomed.length) {
      const url = doomed.shift();
      if (/^y/i.test(prompt)) {
        fire("PostToolUse", { tool_name: "Artifact", tool_input: { action: "delete", url }, tool_response: {} });
        deleted.push(url);
      }
      return deleteNext();
    }
    if (prompt === "/exit") return lines.close();
    if (!prompt) return process.stdout.write("> ");
    fire("UserPromptSubmit", { prompt });
    const answer = `Fake Claude received: ${prompt}`;
    process.stdout.write(`● ${answer}\r\n> `);
    fire("Stop", { last_assistant_message: answer, stop_hook_active: false });
  });
  lines.on("close", () => {
    fire("SessionEnd", { reason: "prompt_input_exit" });
    process.exit(0);
  });
}

if (process.argv[1]?.endsWith("claude.mjs")) {
  if (args[0] === "--version" || args[0] === "-v") console.log(`${VERSION} (Claude Code)`);
  else if (args[0] === "mcp" && args[1] === "list") console.log("No MCP servers configured. Use `claude mcp add` to add a server.");
  else if (args[0] === "auth" && args[1] === "status") console.log(JSON.stringify({ loggedIn: true, authMethod: "claude.ai", email: "alex@example.com" }));
  else if (args[0] === "plugin" && args.includes("--available")) console.log(JSON.stringify({ installed: [], available: [] }));
  else if (args[0] === "plugin" || args[0] === "plugins") console.log("[]");
  else if (["auth", "doctor", "update", "config", "install"].includes(args[0])) console.log("ok");
  else if (args.includes("-p") || args.includes("--print")) await headless();
  else interactive();
}
