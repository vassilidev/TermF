// No imports but types, so `pnpm test` runs it in plain Node.
import type { Gauge } from "$lib/state.svelte";

export type AgentState = "working" | "waiting" | "idle" | "interactive" | "stopped" | "error" | "budget";
export type Outcome = "done" | "interrupted" | "error" | "budget";
export type Usage = { input: number; output: number; cacheRead: number; cacheCreation: number };

/** What `agent_events` and `agent-event` give; `id` 0 is a `delta`, never stored. */
export type StoredEvent = { id: number; session: number; at: number } & (
  | { kind: "started"; resumed: boolean }
  | { kind: "init"; sessionId: string; model: string; permissionMode: string }
  | { kind: "user"; text: string; images?: number }
  | { kind: "delta"; text: string }
  | { kind: "text"; text: string; parent?: string }
  | { kind: "tool_use"; call: string; name: string; input: unknown; parent?: string }
  | { kind: "tool_result"; call: string; output: string; isError: boolean; truncated: boolean; parent?: string }
  | { kind: "permission"; request: string; tool: string; input: unknown; description: string | null }
  | { kind: "permission_answer"; request: string; allow: boolean; message: string | null }
  | { kind: "denied"; tool: string; message: string }
  | { kind: "limit"; status: string; window: string | null; resetsAt: number | null; used: number | null }
  | { kind: "background"; tasks: BackgroundTask[] }
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
  | ({ kind: "notice" } & NoticeFields)
);

/** A command or subagent left running; `activity`: what a subagent does at the moment. */
export type BackgroundTask = { id: string; kind: string; description: string; activity?: string };

/** A message from TermF, not from the user: `waiting` (a gate), `blocked`, `step` or `direct` (sent to another agent), `review` (a round of the code review). */
export type NoticeFields = {
  about: string;
  run?: number;
  step?: string;
  role?: string;
  conversation?: number;
  detail?: string;
  text: string;
};

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
  /** `asked`: the user asked a question instead of answering. */
  answer: { allow: boolean; message: string | null; asked?: true } | null;
};
export type Item =
  | { type: "text"; key: number; text: string }
  | Tool
  | Permission
  | { type: "denied"; key: number; tool: string; message: string }
  | { type: "limit"; key: number; reached: boolean; window: string | null; resetsAt: number | null }
  | { type: "error"; key: number; message: string }
  | { type: "taken_over"; key: number; tile: number }
  | { type: "handed_back"; key: number }
  | ({ type: "notice"; key: number } & NoticeFields)
  /** The user's question about a request, written while it waited. */
  | { type: "question"; key: number; text: string };
export type End = Extract<StoredEvent, { kind: "turn_end" }>;
/** A message and what followed; `user` is null for what happened outside a turn; `images`: how many it carried. */
export type Turn = { key: number; user: string | null; images?: number; items: Item[]; end: End | null };

/** An image of a message, base64 as Claude reads it. */
export type Image = { mediaType: string; data: string };
export const IMAGE_TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp"];
/** 10 MB once in base64, the Claude API's limit per image. */
export const IMAGE_MAX = ((10 * 1024 * 1024) / 4) * 3;

/** Why a file cannot go with a message: not an image Claude reads, or too heavy; null when it can. */
export function unfit(file: { type: string; size: number }): "type" | "size" | null {
  if (!IMAGE_TYPES.includes(file.type)) return "type";
  return file.size > IMAGE_MAX ? "size" : null;
}

/** The media type of a dropped file by its name, "" for one Claude does not read. */
export function imageType(path: string): string {
  const ext = /\.([a-z]+)$/i.exec(path)?.[1].toLowerCase();
  return { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp" }[ext ?? ""] ?? "";
}

/** What the agent reads when the user asks about a request instead of answering it: a refusal that is no decision. */
export function questionAbout(question: string): string {
  return `The user has not decided yet: they ask a question about this request first, so it did not run. Answer them; then, if you still need it, ask for it again.\n<user-question>\n${question}\n</user-question>`;
}

/** The question in a refusal that `questionAbout` wrote, else null. */
export function questionIn(text: string | null): string | null {
  return /<user-question>\n([\s\S]*?)\n<\/user-question>/.exec(text ?? "")?.[1] ?? null;
}

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

/**
 * The agent's text as it streams: deltas add up until the stored text that holds them all. Null until a stored event
 * shows where a message starts: opened in the middle of an answer, its end alone would show.
 */
export function stream(live: string | null, e: StoredEvent): string | null {
  if (e.kind === "delta") return live === null ? null : live + e.text;
  // A usage reading can arrive mid-answer: it is no message.
  if (e.kind === "limit" || e.kind === "background") return live;
  return "parent" in e && e.parent ? (live ?? "") : "";
}

/** What runs in the background now: the last list Claude Code gave, none once its process ended. */
export function inBackground(events: StoredEvent[]): BackgroundTask[] {
  const last = events.findLast((e) => e.kind === "background" || e.kind === "exited" || e.kind === "started");
  return last?.kind === "background" ? last.tasks : [];
}

/** The conversation by turns, a subagent's text and tool calls under the call that started it. */
export function thread(events: StoredEvent[]): Turn[] {
  const turns: Turn[] = [];
  const tools = new Map<string, Tool>();
  const asks = new Map<string, Permission>();
  // A question asked on a permission card comes back as the refused call's result too: shown once.
  const asked: string[] = [];
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
        turns.push({ key: e.id, user: e.text, ...(e.images ? { images: e.images } : {}), items: [], end: null });
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
        const question = questionIn(e.output);
        const twice = question === null ? -1 : asked.indexOf(question);
        if (twice >= 0) asked.splice(twice, 1);
        else if (question !== null) place({ type: "question", key: e.id, text: question }, e.parent);
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
        const question = questionIn(e.message);
        if (ask) ask.answer = question === null ? { allow: e.allow, message: e.message } : { allow: e.allow, message: null, asked: true };
        if (question !== null) {
          asked.push(question);
          place({ type: "question", key: e.id, text: question });
        }
        break;
      }
      case "denied":
        place({ type: "denied", key: e.id, tool: e.tool, message: e.message });
        break;
      case "limit":
        place({ type: "limit", key: e.id, reached: e.status === "rejected", window: e.window, resetsAt: e.resetsAt });
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
      case "notice": {
        const { id, session, at, kind, ...fields } = e;
        place({ type: "notice", key: id, ...fields });
        break;
      }
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

const said = ["ticket", "request", "run", "step", "role", "repo", "workflow", "budget", "question", "query", "feedback", "answers", "note", "text", "id"];

export function fields(input: unknown): [string, string][] {
  if (!input || typeof input !== "object") return [];
  const o = input as Record<string, unknown>;
  return said.flatMap((k): [string, string][] => {
    const v = o[k];
    if (typeof v === "number") return [[k, String(v)]];
    return typeof v === "string" && v.trim() ? [[k, v.trim()]] : [];
  });
}

const flat = (v: unknown): string =>
  Array.isArray(v) ? v.map(flat).join(", ") : v !== null && typeof v === "object" ? JSON.stringify(v) : String(v);

/** A call's answers, lists… are what is allowed: shown before the details, each entry on its own (`""` for a list). */
export function unsaid(input: unknown): [string, [string, string][]][] {
  if (!input || typeof input !== "object") return [];
  return Object.entries(input).flatMap(([name, v]): [string, [string, string][]][] => {
    if (v === null || typeof v !== "object") return [];
    return [[name, Array.isArray(v) ? [["", flat(v)]] : Object.entries(v).map(([k, x]) => [k, flat(x)])]];
  });
}

/** `api#42` or `#42` is often all a call names: the end of a key, when one ticket alone has it. */
export function ticketNamed<T extends { key: string; url: string }>(name: string, known: T[]): T | undefined {
  const exact = known.find((t) => t.key === name || t.url === name);
  if (exact) return exact;
  const tail = name.startsWith("#") ? name : `/${name}`;
  const ends = known.filter((t) => t.key.endsWith(tail));
  return ends.length && ends.every((t) => t.key === ends[0].key) ? ends[0] : undefined;
}

/** The request still waiting for an answer: the last unanswered one since the process or the turn began. */
export function pending(events: StoredEvent[]): string | null {
  let open: string[] = [];
  for (const e of events) {
    if (e.kind === "started" || e.kind === "turn_end") open = [];
    else if (e.kind === "permission") open.push(e.request);
    else if (e.kind === "permission_answer") open = open.filter((r) => r !== e.request);
  }
  return open.at(-1) ?? null;
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

/** `@role message` to one of `roles`: who it goes to and what; null for a message to the CTO itself. */
export function addressed(draft: string, roles: string[]): { role: string; text: string } | null {
  const m = /^@([\w-]+)\s+(\S[\s\S]*)$/.exec(draft.trim());
  const role = m?.[1].toLowerCase() ?? "";
  return m && roles.includes(role) ? { role, text: m[2].trim() } : null;
}

type Pinnable = { mcp?: boolean; budget?: number | null; role?: string | null };

/** A project's standing conversation (the CTO's, the expert's): TermF's tools and no budget. The expert's answers to
 * other agents each have a budget. */
export function pinned(s: Pinnable): boolean {
  return !!s.mcp && s.budget == null;
}

/** For display only: the standing conversations first, the CTO's on top, the rest in the order given. */
export function pinnedFirst<T extends Pinnable>(list: T[]): T[] {
  const rank = (s: T) => (!pinned(s) ? 0 : s.role === "cto" ? 2 : 1);
  return [2, 1, 0].flatMap((r) => list.filter((s) => rank(s) === r));
}

/** While the draft is `@` and the start of a role's name: the roles it may be, in their order. */
export function mentioned(draft: string, roles: string[]): string[] {
  const m = /^@([\w-]*)$/.exec(draft);
  return m ? roles.filter((r) => r.startsWith(m[1].toLowerCase())) : [];
}
