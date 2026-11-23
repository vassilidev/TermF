// No imports, so `pnpm test` runs it in plain Node.

/**
 * A piece of what the screen shows, sent with the question: the view, the object open, the selection. `text` is what
 * the agent reads under the label; a `secret` chip sends its label (a name, a type) and never its text.
 */
export type Chip = { id: string; kind: "view" | "object" | "selection"; label: string; text?: string; secret?: boolean };

/** Who answers on each screen, unless the screen says better (the design study's table). */
export const ROLE_BY_VIEW: Record<string, string> = {
  home: "cto",
  tickets: "cto",
  conversations: "cto",
  run: "lead",
  terminals: "cto",
  ci: "devops",
  ops: "devops",
  ssh: "devops",
  vault: "secops",
  design: "ux",
  review: "ux",
  git: "expert",
  files: "expert",
};

export function roleFor(view: string, given?: string | null): string {
  return given || ROLE_BY_VIEW[view] || "cto";
}

/** `@role` at the start of the question wins over the role chosen; the rest is the question. */
export function target(text: string, chosen: string, roles: string[]): { role: string; text: string } {
  const m = /^@([\w-]+)\s+(\S[\s\S]*)$/.exec(text.trim());
  const role = m?.[1].toLowerCase() ?? "";
  return m && (roles.includes(role) || role === "cto" || role === "expert") ? { role, text: m[2].trim() } : { role: chosen, text: text.trim() };
}

/** What a selection may weigh in a question. */
export const MAX_CONTEXT = 4000;

const cut = (text: string) => (text.length > MAX_CONTEXT ? `${text.slice(0, MAX_CONTEXT)}\n[…]` : text);

/** What the agent reads: the screen's context, then the question. A secret chip gives its name only. */
export function compose(text: string, chips: Chip[], screen: string): string {
  if (!chips.length) return text;
  const lines = chips.map((c) => {
    const body = !c.secret && c.text?.trim() ? `\n${cut(c.text.trim()).replace(/^/gm, "  ")}` : "";
    return `- ${c.label}${body}`;
  });
  return `<termf-context screen="${screen}">\nWhat the user is looking at in TermF, as context for the question:\n${lines.join("\n")}\n</termf-context>\n\n${text}`;
}

/** A message as the user wrote it, without the context the panel sent before it. */
export function question(text: string): string {
  return text.replace(/^<termf-context[^>]*>[\s\S]*?<\/termf-context>\s*/, "");
}

/** The chips sent: the screen's and those added, without those taken off; one per id, the last one given winning. */
export function chipsOf(given: Chip[], added: Chip[], removed: string[]): Chip[] {
  const byId = new Map<string, Chip>();
  for (const chip of [...given, ...added]) byId.set(chip.id, chip);
  return [...byId.values()].filter((c) => !removed.includes(c.id));
}

/** A permission the user answers as a proposal: a file change can be undone when the file existed before. */
export type Change = { path: string; kind: "edit" | "write" };

/** The file a tool call is about to change, if it is a file change. */
export function changeOf(tool: string, input: unknown): Change | null {
  const path = (input as { file_path?: unknown } | null)?.file_path;
  if (typeof path !== "string" || !path) return null;
  if (tool === "Edit" || tool === "MultiEdit") return { path, kind: "edit" };
  if (tool === "Write") return { path, kind: "write" };
  return null;
}

/** The repository holding an absolute path, and the path inside it; null outside every repository given. */
export function within(path: string, repos: string[]): { repo: string; rel: string } | null {
  const repo = repos
    .filter((r) => path.startsWith(r.endsWith("/") ? r : `${r}/`))
    .sort((a, b) => b.length - a.length)[0];
  return repo ? { repo, rel: path.slice(repo.replace(/\/$/, "").length + 1) } : null;
}

/**
 * The file as the change leaves it, from the file before: what an undo checks the file still is before putting it
 * back. Null when the change would not apply as given (the text to replace is not there).
 */
export function afterChange(tool: string, input: unknown, before: string): string | null {
  const i = (input ?? {}) as { old_string?: unknown; new_string?: unknown; replace_all?: unknown; content?: unknown; edits?: unknown };
  if (tool === "Write") return typeof i.content === "string" ? i.content : null;
  const edits = tool === "Edit" ? [i] : tool === "MultiEdit" && Array.isArray(i.edits) ? (i.edits as (typeof i)[]) : [];
  if (!edits.length) return null;
  let text = before;
  for (const e of edits) {
    if (typeof e.old_string !== "string" || typeof e.new_string !== "string" || !e.old_string || !text.includes(e.old_string)) return null;
    const by = e.new_string;
    text = e.replace_all === true ? text.split(e.old_string).join(by) : text.replace(e.old_string, () => by);
  }
  return text;
}

/** An edit as the lines it takes off and the lines it puts in, for the proposal's preview. */
export function diffOf(tool: string, input: unknown): { del: string[]; add: string[] }[] {
  const i = (input ?? {}) as { old_string?: unknown; new_string?: unknown; content?: unknown; edits?: unknown };
  const lines = (v: unknown) => (typeof v === "string" && v ? v.split("\n") : []);
  if (tool === "Edit") return [{ del: lines(i.old_string), add: lines(i.new_string) }];
  if (tool === "MultiEdit" && Array.isArray(i.edits))
    return i.edits.map((e: { old_string?: unknown; new_string?: unknown }) => ({ del: lines(e.old_string), add: lines(e.new_string) }));
  if (tool === "Write") return [{ del: [], add: lines(i.content) }];
  return [];
}
