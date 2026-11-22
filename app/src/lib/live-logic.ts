// No imports but types, so `pnpm test` runs it in plain Node.
import type { StoredEvent } from "$lib/conversation-logic";

/** What an agent does: thinks, writes its answer, or uses a tool (running, done or failed). */
export type Act =
  | { kind: "thinking"; at: number }
  | { kind: "writing"; at: number }
  | { kind: "tool"; at: number; call: string; name: string; input: unknown; state: "running" | "done" | "failed" };
export type ToolAct = Extract<Act, { kind: "tool" }>;
/** A session's activity: what it does now (null between turns) and its last tool calls, oldest first. */
export type Activity = { now: Act | null; recent: ToolAct[] };

const KEEP = 30;

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
      const act: ToolAct = { kind: "tool", at: e.at, call: e.call, name: e.name, input: e.input, state: "running" };
      return { now: act, recent: [...was.recent, act].slice(-KEEP) };
    }
    case "tool_result": {
      const state = e.isError ? "failed" : "done";
      const mark = (a: ToolAct): ToolAct => (a.call === e.call ? { ...a, state } : a);
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

const EDITS = new Set(["Edit", "MultiEdit", "Write", "NotebookEdit"]);
const edited = (act: ToolAct) => first((act.input ?? {}) as Record<string, unknown>, ["file_path", "notebook_path", "path"]);

/** A duration as `{key, vars}`: 42 s, 7 min, 1 h 05. */
export function elapsed(seconds: number): Phrase {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return { key: "liveSeconds", vars: { n: s } };
  if (s < 3600) return { key: "liveMinutes", vars: { n: Math.floor(s / 60) } };
  return { key: "liveHours", vars: { h: Math.floor(s / 3600), m: String(Math.floor((s % 3600) / 60)).padStart(2, "0") } };
}

type FeedStep = { key: string; role: string | null; state: string; session: number | null; started: number | null; finished: number | null; report: { status: string } | null };
/** A line of a run's feed: a step that starts or ends, or a call of a step's agent. */
export type Moment =
  | { kind: "start"; at: number; step: string; role: string | null }
  | { kind: "end"; at: number; step: string; how: "done" | "blocked" | "failed" }
  | { kind: "act"; at: number; step: string; role: string | null; act: ToolAct }
  | { kind: "edits"; at: number; step: string; role: string | null; files: string[] };

/**
 * A run's feed, oldest first: its steps' starts and ends, and what their agents did that this window heard. Files a
 * step changes in a row make one line.
 */
export function feed(steps: FeedStep[], heard: (session: number) => ToolAct[]): Moment[] {
  const out: Moment[] = [];
  for (const s of steps) {
    if (s.started !== null) out.push({ kind: "start", at: s.started, step: s.key, role: s.role });
    if (s.finished !== null && (s.state === "done" || s.state === "blocked")) {
      const how = s.state === "done" ? "done" : s.report?.status === "failed" ? "failed" : "blocked";
      out.push({ kind: "end", at: s.finished, step: s.key, how });
    }
    for (const act of s.session === null ? [] : heard(s.session)) out.push({ kind: "act", at: act.at, step: s.key, role: s.role, act });
  }
  // Stable: a step's start stays before its first call of the same second.
  const sorted = out.map((m, i) => [m, i] as const).sort(([a, i], [b, j]) => a.at - b.at || i - j);
  const merged: Moment[] = [];
  for (const [m] of sorted) {
    const last = merged.at(-1);
    const edit = m.kind === "act" && EDITS.has(m.act.name) ? edited(m.act) : null;
    if (edit === null || !last || last.step !== m.step) merged.push(m);
    else if (last.kind === "edits") last.files = [...new Set([...last.files, edit])];
    else if (last.kind === "act" && EDITS.has(last.act.name)) {
      merged[merged.length - 1] = { kind: "edits", at: last.at, step: m.step, role: last.role, files: [...new Set([edited(last.act), edit])] };
    } else merged.push(m);
  }
  return merged;
}
