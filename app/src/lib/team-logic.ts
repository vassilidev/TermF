// No imports, so `pnpm test` runs it in plain Node.

/** Two letters for a role's mark: `front` → FR, `dev-ops` → DO. */
export function initials(name: string): string {
  const words = name.split(/[-_\s]+/).filter(Boolean);
  const first = (word: string) => [...word][0];
  const letters = words.length > 1 ? first(words[0]) + first(words[1]) : [...(words[0] ?? "?")].slice(0, 2).join("");
  return letters.toUpperCase();
}

/** A frontmatter list, written as a YAML list or as a comma-separated string. */
function list(value: unknown): string[] | null {
  const items = Array.isArray(value) ? value.map(String) : typeof value === "string" ? value.split(",") : null;
  return items && items.map((s) => s.trim()).filter(Boolean);
}

/**
 * What a role may write: its `writes` globs; "readOnly" when it has none, when its `tools` leave out both Edit and
 * Write, or when its `disallowedTools` deny both; null when the role declares no limit.
 */
export function access(meta: Record<string, unknown>): string[] | "readOnly" | null {
  const writes = list(meta.writes);
  if (writes) return writes.length ? writes : "readOnly";
  const tools = list(meta.tools);
  if (tools && !tools.includes("Edit") && !tools.includes("Write")) return "readOnly";
  const denied = list(meta.disallowedTools) ?? [];
  return denied.includes("Edit") && denied.includes("Write") ? "readOnly" : null;
}

/** The project a save just created: the one missing before. */
export function added(before: { id: string }[], after: { id: string }[]): string | null {
  return after.find((p) => !before.some((b) => b.id === p.id))?.id ?? null;
}

/** A role setting as the Team view shows it: a list joined, anything else as text. */
export function shown(value: unknown): string {
  return Array.isArray(value) ? value.map(String).join(", ") : typeof value === "string" ? value : JSON.stringify(value);
}

/** The repository and license of a role's credit: `owner/repo@commit (MIT): path` gives `owner/repo (MIT)`. */
export function source(credit: string): string {
  return credit.replace(/@[0-9a-f]+/i, "").replace(/: .*$/, "").trim();
}

/** A squad as its editor sets it (`team_squad_save`): an empty lead is the default one. */
export type SquadForm = {
  id: string;
  lead: string;
  roles: Record<string, number>;
  repos: string[];
  paths: string[];
  domains: string[];
};

/** What the editor says of a squad: an error stops the save, a warning only tells. `subject`: what it is about. */
export type SquadIssue = {
  level: "error" | "warning";
  kind: "id" | "taken" | "role" | "count" | "path" | "repo" | "overlap";
  subject: string;
};

/** Whether `path` stays inside a repository: neither absolute nor going up. */
function inside(path: string): boolean {
  return !!path && !/^([/\\]|[A-Za-z]:)/.test(path) && !path.split(/[/\\]/).includes("..");
}

/** A glob's fixed part, before its first wildcard: `apps/web/**` → `apps/web`. */
function fixed(glob: string): string {
  return glob.replace(/^\.\//, "").split(/[*?[{]/)[0].replace(/\/+$/, "");
}

/** Whether two squads may change the same files: a repository in common, and paths that meet (none = all). */
export function overlaps(a: SquadForm, b: SquadForm): boolean {
  if (a.repos.length && b.repos.length && !a.repos.some((r) => b.repos.includes(r))) return false;
  if (!a.paths.length || !b.paths.length) return true;
  const under = (x: string, y: string) => !y || x === y || x.startsWith(`${y}/`);
  return a.paths.some((p) => b.paths.some((q) => under(fixed(p), fixed(q)) || under(fixed(q), fixed(p))));
}

/** What is wrong with `squad` among the project's `others`, its `roles` (known to the library) and `repos`. */
export function squadIssues(squad: SquadForm, others: SquadForm[], roles: string[], repos: string[]): SquadIssue[] {
  const issues: SquadIssue[] = [];
  const add = (level: SquadIssue["level"], kind: SquadIssue["kind"], subject: string) =>
    issues.push({ level, kind, subject });
  const id = squad.id.trim();
  if (!/^[A-Za-z0-9_-]+$/.test(id)) add("error", "id", id);
  else if (others.some((o) => o.id === id)) add("error", "taken", id);
  const known = (r: string) => roles.includes(r === "orchestrator" ? "lead" : r);
  const lead = squad.lead.trim();
  if (lead && !known(lead)) add("error", "role", lead);
  for (const [role, n] of Object.entries(squad.roles)) {
    if (!known(role)) add("error", "role", role);
    if (!Number.isInteger(n) || n < 1) add("error", "count", role);
  }
  for (const path of squad.paths) if (!inside(path)) add("error", "path", path);
  for (const repo of squad.repos) if (!repos.includes(repo)) add("error", "repo", repo);
  for (const other of others) if (overlaps(squad, other)) add("warning", "overlap", other.id);
  return issues;
}
