// No imports but types, so `pnpm test` runs it in plain Node.
import type { Gauge } from "$lib/state.svelte";

export type AgentState = "working" | "waiting" | "idle" | "interactive" | "stopped" | "error" | "budget";
export type Outcome = "done" | "interrupted" | "error" | "budget";
export type Usage = { input: number; output: number; cacheRead: number; cacheCreation: number };

/** What `agent_events` and `agent-event` give; `id` 0 is a `delta`, never stored. */
export type StoredEvent = { id: number; session: number; at: number } & (
  | { kind: "started"; resumed: boolean }
  | { kind: "init"; sessionId: string; model: string; permissionMode: string }
  | { kind: "user"; text: string }
  | { kind: "delta"; text: string }
  | { kind: "text"; text: string; parent?: string }
  | { kind: "tool_use"; call: string; name: string; input: unknown; parent?: string }
  | { kind: "tool_result"; call: string; output: string; isError: boolean; truncated: boolean; parent?: string }
  | { kind: "permission"; request: string; tool: string; input: unknown; description: string | null }
  | { kind: "permission_answer"; request: string; allow: boolean; message: string | null }
  | { kind: "denied"; tool: string; message: string }
  | {
      kind: "turn_end";
      outcome: Outcome;
      result: string | null;
      error: string | null;
      usage: Usage;
      cost: number;
      total: number;
      context: Gauge | null;
    }
  | { kind: "error"; message: string }
  | { kind: "exited"; code: number | null }
  | { kind: "taken_over"; tile: number }
  | { kind: "handed_back" }
);

export type Tool = {
  type: "tool";
  key: number;
  call: string;
  name: string;
  input: unknown;
  result: { output: string; isError: boolean; truncated: boolean } | null;
  /** What a subagent said and did under this call. */
  children: Item[];
};
export type Permission = {
  type: "permission";
  key: number;
  request: string;
  tool: string;
  input: unknown;
  description: string | null;
  answer: { allow: boolean; message: string | null } | null;
};
export type Item =
  | { type: "text"; key: number; text: string }
  | Tool
  | Permission
  | { type: "denied"; key: number; tool: string; message: string }
  | { type: "error"; key: number; message: string }
  | { type: "taken_over"; key: number; tile: number }
  | { type: "handed_back"; key: number };
export type End = Extract<StoredEvent, { kind: "turn_end" }>;
/** A message and what followed; `user` is null for what happened outside a turn. */
export type Turn = { key: number; user: string | null; items: Item[]; end: End | null };

/** Stored events in id order, each once. */
export function merge(events: StoredEvent[], incoming: StoredEvent[]): StoredEvent[] {
  const out = [...events];
  for (const e of incoming) {
    if (e.id <= 0) continue;
    const last = out.at(-1);
    if (!last || e.id > last.id) out.push(e);
    else if (!out.some((o) => o.id === e.id)) {
      out.push(e);
      out.sort((a, b) => a.id - b.id);
    }
  }
  return out;
}

/** The agent's text as it streams: deltas add up until the stored text that holds them all. */
export function stream(live: string, e: StoredEvent): string {
  if (e.kind === "delta") return live + e.text;
  const over = (e.kind === "text" && !e.parent) || e.kind === "user" || e.kind === "turn_end" || e.kind === "error";
  return over ? "" : live;
}

/** The conversation by turns, a subagent's text and tool calls under the call that started it. */
export function thread(events: StoredEvent[]): Turn[] {
  const turns: Turn[] = [];
  const tools = new Map<string, Tool>();
  const asks = new Map<string, Permission>();
  const open = (key: number): Turn => {
    const last = turns.at(-1);
    if (last && !last.end) return last;
    const turn: Turn = { key, user: null, items: [], end: null };
    turns.push(turn);
    return turn;
  };
  const place = (item: Item, parent?: string) => {
    const owner = parent ? tools.get(parent) : undefined;
    (owner ? owner.children : open(item.key).items).push(item);
  };
  for (const e of events) {
    switch (e.kind) {
      case "user":
        turns.push({ key: e.id, user: e.text, items: [], end: null });
        break;
      case "text":
        place({ type: "text", key: e.id, text: e.text }, e.parent);
        break;
      case "tool_use": {
        const tool: Tool = { type: "tool", key: e.id, call: e.call, name: e.name, input: e.input, result: null, children: [] };
        tools.set(e.call, tool);
        place(tool, e.parent);
        break;
      }
      case "tool_result": {
        const tool = tools.get(e.call);
        if (tool) tool.result = { output: e.output, isError: e.isError, truncated: e.truncated };
        break;
      }
      case "permission": {
        const ask: Permission = {
          type: "permission",
          key: e.id,
          request: e.request,
          tool: e.tool,
          input: e.input,
          description: e.description,
          answer: null,
        };
        asks.set(e.request, ask);
        place(ask);
        break;
      }
      case "permission_answer": {
        const ask = asks.get(e.request);
        if (ask) ask.answer = { allow: e.allow, message: e.message };
        break;
      }
      case "denied":
        place({ type: "denied", key: e.id, tool: e.tool, message: e.message });
        break;
      case "error":
        place({ type: "error", key: e.id, message: e.message });
        break;
      case "taken_over":
        place({ type: "taken_over", key: e.id, tile: e.tile });
        break;
      case "handed_back":
        place({ type: "handed_back", key: e.id });
        break;
      case "turn_end":
        open(e.id).end = e;
        break;
    }
  }
  return turns;
}

const telling = ["command", "file_path", "notebook_path", "pattern", "path", "url", "query", "description", "prompt"];

/** One line that says what a tool call is about: its command, file, pattern… else its first text field. */
export function summary(input: unknown): string {
  if (input === null || input === undefined) return "";
  if (typeof input !== "object") return String(input).split("\n")[0];
  const o = input as Record<string, unknown>;
  const text = (k: string) => typeof o[k] === "string" && (o[k] as string).trim() !== "";
  const key = telling.find(text) ?? Object.keys(o).find(text);
  if (key) return (o[key] as string).trim().split("\n")[0];
  return Object.keys(o).length ? JSON.stringify(o) : "";
}

/** Why a message cannot be sent now; null between turns, when it can. */
export function blocked(state: AgentState): "working" | "waiting" | "interactive" | "budget" | null {
  return state === "working" || state === "waiting" || state === "interactive" || state === "budget" ? state : null;
}

/** The next one waiting after `current` in `order`, else the first one waiting. */
export function nextAfter(order: string[], waiting: string[], current: string | null): string | null {
  const from = current === null ? -1 : order.indexOf(current);
  return waiting.find((k) => order.indexOf(k) > from) ?? waiting[0] ?? null;
}
