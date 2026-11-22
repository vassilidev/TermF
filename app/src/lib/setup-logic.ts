// No imports, so `pnpm test` runs it in plain Node.

export type Remote = { host: string; slug: string };
/** A repository as the setup lists it (`setup_repos`), before or after its git state and stack are read. */
export type Card = {
  name: string;
  path: string;
  modified: number | null;
  branch?: string | null;
  remote?: Remote | null;
  changes?: number;
  /** `null` once reading it failed. */
  stack?: string[] | null;
};
export type Filter = "all" | "recent" | "github";
/** Where a name comes from: the only repository, their GitHub owner, their folder, or the first one. */
export type NameSource = "repo" | "owner" | "folder" | "first";

/** Active in the last two weeks. */
const RECENT = 14 * 86_400;

const dir = (path: string) => path.replace(/[\\/]+$/, "").replace(/[\\/][^\\/]*$/, "");
const base = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
const owner = (c: Card) => (c.remote?.host === "github.com" ? c.remote.slug.split("/")[0] : null);

/** The most recently worked on first. */
export function byActivity<T extends Card>(cards: T[]): T[] {
  return [...cards].sort((a, b) => (b.modified ?? 0) - (a.modified ?? 0) || a.name.localeCompare(b.name));
}

export function shown(card: Card, filter: Filter, query: string, now: number): boolean {
  if (filter === "recent" && (card.modified ?? 0) < now - RECENT) return false;
  if (filter === "github" && card.remote?.host !== "github.com") return false;
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const text = [card.name, card.path, card.remote?.slug ?? "", ...(card.stack ?? [])].join(" ").toLowerCase();
  return words.every((w) => text.includes(w));
}

/**
 * The project's name, from what was chosen: one repository gives its name; several, their GitHub owner when they
 * share one, else the folder they share (never the home folder), else the first one's name.
 */
export function derive(chosen: Card[], home: string): { name: string; from: NameSource } | null {
  if (!chosen.length) return null;
  if (chosen.length === 1) return { name: chosen[0].name, from: "repo" };
  const owners = new Set(chosen.map(owner));
  const [one] = owners;
  if (owners.size === 1 && one) return { name: one, from: "owner" };
  const parents = new Set(chosen.map((c) => dir(c.path)));
  const [parent] = parents;
  if (parents.size === 1 && parent && parent !== home.replace(/[\\/]+$/, "")) return { name: base(parent), from: "folder" };
  return { name: chosen[0].name, from: "first" };
}

/** `name`, or `name 2`, `name 3`… when a project already has it (whatever the case). */
export function unique(name: string, taken: string[]): string {
  const used = new Set(taken.map((t) => t.trim().toLowerCase()));
  if (!used.has(name.trim().toLowerCase())) return name;
  let n = 2;
  while (used.has(`${name} ${n}`.toLowerCase())) n++;
  return `${name} ${n}`;
}

/** The id the project gets, as `termf_core::save_project` picks it: its name in lowercase words, then `-2`, `-3`… */
export function projectId(name: string, ids: string[]): string {
  const words = name.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean);
  const slug = words.join("-") || "project";
  if (!ids.includes(slug)) return slug;
  let n = 2;
  while (ids.includes(`${slug}-${n}`)) n++;
  return `${slug}-${n}`;
}

export type Group = { kind: "owner" | "folder"; label: string; paths: string[] };

/** Repositories worth taking together: those of one GitHub owner, then those of one folder, two or more each. */
export function groups(cards: Card[], chosen: string[], home: string): Group[] {
  const found: Group[] = [];
  const collect = (kind: Group["kind"], key: (c: Card) => string | null, label: (k: string) => string) => {
    const by = new Map<string, string[]>();
    for (const c of cards) {
      const k = key(c);
      if (k) by.set(k, [...(by.get(k) ?? []), c.path]);
    }
    for (const [k, paths] of by) {
      const same = found.some((g) => g.paths.length === paths.length && g.paths.every((p) => paths.includes(p)));
      if (paths.length > 1 && !same && !paths.every((p) => chosen.includes(p))) found.push({ kind, label: label(k), paths });
    }
  };
  collect("owner", owner, (k) => k);
  const root = home.replace(/[\\/]+$/, "");
  collect("folder", (c) => (dir(c.path) === root ? null : dir(c.path)), (k) => k);
  return found.sort((a, b) => b.paths.length - a.paths.length);
}

/** The GitLab host of the chosen repositories (the first one's) and their projects on it: what GitLab would read. */
export function gitlabRepos(cards: Card[]): { host: string; slugs: string[] } | null {
  const on = cards.filter((c) => forge(c.remote?.host) === "gitlab").map((c) => c.remote!);
  if (!on.length) return null;
  const host = on[0].host;
  return { host, slugs: [...new Set(on.filter((r) => r.host === host).map((r) => r.slug.toLowerCase()))] };
}

/** The GitHub repositories (`owner/name`) of the chosen ones: what GitHub Issues would read. */
export function githubSlugs(cards: Card[]): string[] {
  return [...new Set(cards.filter((c) => c.remote?.host === "github.com").map((c) => c.remote!.slug.toLowerCase()))];
}

/** The forge a host is: its mark is drawn for the known ones. */
export function forge(host: string | undefined): "github" | "gitlab" | "bitbucket" | "other" | "none" {
  if (!host) return "none";
  if (host === "github.com" || host.endsWith(".github.com")) return "github";
  if (host.includes("gitlab")) return "gitlab";
  if (host.includes("bitbucket")) return "bitbucket";
  return "other";
}
