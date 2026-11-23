// No imports but types, so `pnpm test` runs it in plain Node.
import type { StoredEvent } from "$lib/conversation-logic";

/** What an agent does: thinks, writes its answer, or uses a tool (running, done or failed). */
export type Act =
  | { kind: "thinking"; at: number }
  | { kind: "writing"; at: number }
  | { kind: "tool"; at: number; call: string; name: string; input: unknown; state: "running" | "done" | "failed"; output?: string };
export type ToolAct = Extract<Act, { kind: "tool" }>;
/** A session's activity: what it does now (null between turns) and its last tool calls, oldest first. */
export type Activity = { now: Act | null; recent: ToolAct[] };

const KEEP = 30;
/** What a failed call keeps of its output: its last lines, where a test run or a build says what broke. */
const TAIL = 12;
const tail = (output: string) => output.trimEnd().split("\n").slice(-TAIL).join("\n");

/**
 * The activity after `e`; the same object when nothing changed, so a stream of deltas costs no update. A subagent's
 * calls count as the session's own: they are what it does now.
 */
export function follow(activity: Activity | undefined, e: StoredEvent): Activity {
  const was = activity ?? { now: null, recent: [] };
  switch (e.kind) {
    case "started":
    case "user":
      return { ...was, now: { kind: "thinking", at: e.at } };
    case "delta":
    case "text":
      if (e.kind === "text" && e.parent) return was;
      return was.now?.kind === "writing" ? was : { ...was, now: { kind: "writing", at: e.at } };
    case "tool_use": {
      // Heard live, then read again with the session's past: once is enough.
      if (was.recent.some((a) => a.call === e.call)) return was;
      const act: ToolAct = { kind: "tool", at: e.at, call: e.call, name: e.name, input: e.input, state: "running" };
      return { now: act, recent: [...was.recent, act].slice(-KEEP) };
    }
    case "tool_result": {
      const state = e.isError ? "failed" : "done";
      const mark = (a: ToolAct): ToolAct => (a.call !== e.call ? a : e.isError ? { ...a, state, output: tail(e.output) } : { ...a, state });
      if (!was.recent.some((a) => a.call === e.call)) return was;
      const recent = was.recent.map(mark);
      // Between a call's end and what comes next, the agent reads what it got.
      const now = was.now?.kind === "tool" && was.now.call === e.call ? { kind: "thinking" as const, at: e.at } : was.now;
      return { now, recent };
    }
    case "turn_end":
    case "exited":
    case "error":
    case "taken_over":
      return was.now === null ? was : { ...was, now: null };
    default:
      return was;
  }
}

const FAILURE = /\b(fail(ed|ures?)?|errors?|assert(ion)?error|panic(ked)?)\b|✗|×/i;
/** "0 failed", "no errors": a count of failures that says there are none. */
const NONE = /\b(0|no)\s+(fail(ed|ures?)?|errors?)\b/gi;

/** Whether a line a command printed says something broke: a failure or an error, not a count of none. */
export const broke = (line: string) => FAILURE.test(line.replace(NONE, ""));

/** A phrase key and its values: `target` is what to show as code. */
export type Phrase = { key: string; target?: string; vars?: Record<string, string | number> };

const first = (o: Record<string, unknown>, keys: string[]): string => {
  const k = keys.find((k) => typeof o[k] === "string" && (o[k] as string).trim());
  return k ? (o[k] as string).trim().split("\n")[0] : "";
};
const clip = (s: string, n = 80) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);
/** The end of a path is what tells which file. */
const file = (path: string) => {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length > 3 ? `…/${parts.slice(-3).join("/")}` : path;
};

/**
 * A tool call in plain words: `Bash(npm test)` is « Lance npm test ». An unknown tool keeps its name; an MCP tool
 * names its server.
 */
export function phrase(name: string, input: unknown): Phrase {
  const o = input && typeof input === "object" ? (input as Record<string, unknown>) : {};
  const path = first(o, ["file_path", "notebook_path", "path"]);
  switch (name) {
    case "Read":
      return { key: "liveRead", target: file(path) };
    case "Write":
      return { key: "liveWrite", target: file(path) };
    case "Edit":
    case "MultiEdit":
    case "NotebookEdit":
      return { key: "liveEdit", target: file(path) };
    case "Bash":
    case "BashOutput":
      return { key: "liveRun", target: clip(first(o, ["command", "description"])) };
    case "Grep":
      return { key: "liveGrep", target: clip(first(o, ["pattern"]), 60) };
    case "Glob":
      return { key: "liveGlob", target: clip(first(o, ["pattern"]), 60) };
    case "LS":
      return { key: "liveList", target: file(path) };
    case "WebSearch":
      return { key: "liveWebSearch", target: clip(first(o, ["query"])) };
    case "WebFetch":
      return { key: "liveWebFetch", target: clip(first(o, ["url"])) };
    case "Task":
    case "Agent":
      return { key: "liveAgent", target: clip(first(o, ["description", "prompt"])) };
    case "TodoWrite":
      return { key: "liveTodo" };
    case "Skill":
      return { key: "liveSkill", target: first(o, ["skill", "command", "name"]) };
  }
  const mcp = /^mcp__(.+?)__(.+)$/.exec(name);
  if (mcp) return { key: "liveMcp", vars: { server: mcp[1], tool: mcp[2].replace(/_/g, " ") } };
  const target = clip(first(o, ["command", "file_path", "path", "pattern", "url", "query", "description"]));
  return target ? { key: "liveToolOn", target, vars: { tool: name } } : { key: "liveTool", vars: { tool: name } };
}

/**
 * A session's activity from its stored past, then what was heard while it was read and came after it. From nothing:
 * what was heard before is in the past too.
 */
export function replay(stored: StoredEvent[], heard: StoredEvent[]): Activity | undefined {
  const last = stored.at(-1)?.id ?? 0;
  return [...stored, ...heard.filter((e) => e.id === 0 || e.id > last)].reduce<Activity | undefined>(follow, undefined);
}

/** A duration as `{key, vars}`: 42 s, 7 min, 1 h 05. */
export function elapsed(seconds: number): Phrase {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return { key: "liveSeconds", vars: { n: s } };
  if (s < 3600) return { key: "liveMinutes", vars: { n: Math.floor(s / 60) } };
  return { key: "liveHours", vars: { h: Math.floor(s / 3600), m: String(Math.floor((s % 3600) / 60)).padStart(2, "0") } };
}
