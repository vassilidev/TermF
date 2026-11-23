// No imports, so `pnpm test` runs it in plain Node.

type Ref = { kind: string; name: string };
export type ScoutLike = {
  draft: { entries: (Ref & { review: string; critique: { flaw: string }[] })[] } | null;
  journal: (Ref & { action: string; project: string | null; at: number })[];
  claude: string | null;
};
export type ConventionLike = {
  slug: string;
  draft: { questions: string[]; items: { review: string }[] } | null;
};
export type TodoInput = {
  project: string | null;
  scout: ScoutLike | null;
  /** The global library's watch, when a project is current. */
  globalScout: ScoutLike | null;
  /** null: not loaded, or no project. */
  conventions: ConventionLike[] | null;
  /** A scheduled watch covers this scope. */
  scheduled: boolean;
  /** What `claude --version` says now. */
  claude: string | null;
  /** The items of the scope as sessions get them. */
  items: (Ref & { active: boolean; toggles?: Partial<Record<string, boolean>> })[];
  dismissed: string[];
};
export type Todo =
  | { id: string; kind: "questions" | "decide"; slug: string; count: number }
  | { id: string; kind: "scout" | "scoutGlobal"; count: number; outdated: number; names: string[] }
  /** `was`: the switch of the scope's layer before, to put back. */
  | { id: string; kind: "off"; item: Ref; was: boolean | null }
  | { id: string; kind: "claude"; seen: string; now: string }
  | { id: string; kind: "never" | "noConventions" | "noSchedule" };

/** `2.1.330 (Claude Code)` → `2.1.330`. */
export const version = (v: string | null) => v?.trim().split(/\s+/)[0] || null;

/** What waits in the library, the most pressing first: each line has one action in the view. */
export function todos(input: TodoInput): Todo[] {
  const out: Todo[] = [];
  for (const c of input.conventions ?? []) {
    if (c.draft?.questions.length) out.push({ id: `questions:${c.slug}`, kind: "questions", slug: c.slug, count: c.draft.questions.length });
    else {
      const count = c.draft?.items.filter((i) => i.review === "pending").length ?? 0;
      if (count) out.push({ id: `decide:${c.slug}`, kind: "decide", slug: c.slug, count });
    }
  }
  const scoutRow = (s: ScoutLike | null, kind: "scout" | "scoutGlobal") => {
    const pending = s?.draft?.entries.filter((e) => e.review === "pending") ?? [];
    if (!pending.length) return;
    const outdated = pending.filter((e) => e.critique.some((f) => f.flaw === "outdated")).length;
    out.push({ id: kind, kind, count: pending.length, outdated, names: pending.map((e) => e.name) });
  };
  scoutRow(input.scout, "scout");
  scoutRow(input.globalScout, "scoutGlobal");

  // Applied by the watch for this scope, and off in it: a shipped item improved lands as a switched-off override.
  const seen = new Set<string>();
  for (const l of input.scout?.journal ?? []) {
    const key = `${l.kind}/${l.name}`;
    if (l.action !== "applied" || seen.has(key) || (l.project !== null && l.project !== input.project)) continue;
    seen.add(key);
    const item = input.items.find((i) => i.kind === l.kind && i.name === l.name && !i.active);
    if (item) {
      const was = item.toggles?.[input.project ? "project" : "global"] ?? null;
      out.push({ id: `off:${key}`, kind: "off", item: { kind: l.kind, name: l.name }, was });
    }
  }

  const ran = input.scout?.claude ?? input.globalScout?.claude ?? null;
  const [was, now] = [version(ran), version(input.claude)];
  if (was && now && was !== now) out.push({ id: `claude:${now}`, kind: "claude", seen: was, now });
  // The version and the journal are shared by every scope: this one ran if it has results, or decided some.
  const everRan = !!input.scout?.draft || !!input.scout?.journal.some((l) => l.project === input.project);
  if (!everRan) out.push({ id: "never", kind: "never" });
  if (input.project && input.conventions && !input.conventions.length) out.push({ id: "noConventions", kind: "noConventions" });
  if (!input.scheduled) out.push({ id: "noSchedule", kind: "noSchedule" });
  return out.filter((t) => !input.dismissed.includes(dismissKey(input.project, t.id)));
}

/** "Later" holds for the project it was said in only. */
export const dismissKey = (project: string | null, id: string) => `${project ?? ""}:${id}`;

/** Where an item lives in a library, as `termf_library` lays each kind out. */
export function libraryFile(kind: string, name: string): string {
  if (kind === "skill") return `skills/${name}/SKILL.md`;
  if (kind === "rule") return `rules/${name}/rule.md`;
  return `${kind}s/${name}.md`;
}

/** Whether a scheduled task is the watch of this scope: a scout task in one of the project's repositories, or
 * outside any project's for the global library. */
export function watches(task: { mode: string; dir: string }, repos: string[], projectRepos: string[]): boolean {
  if (task.mode !== "scout") return false;
  return repos.length ? repos.includes(task.dir) : !projectRepos.includes(task.dir);
}
