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
/** Where a name comes from: the only repository, the start of their names, their GitHub owner, their folder, or the first one. */
export type NameSource = "repo" | "prefix" | "owner" | "folder" | "first";

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

const TAGS: Record<string, string> = {
  nextjs: "Next.js",
  react: "React",
  typescript: "TypeScript",
  javascript: "JavaScript",
  node: "Node.js",
  tailwind: "Tailwind",
  vitest: "Vitest",
  jest: "Jest",
  playwright: "Playwright",
  postgres: "Postgres",
  mysql: "MySQL",
  prisma: "Prisma",
  redis: "Redis",
  laravel: "Laravel",
  php: "PHP",
  phpunit: "PHPUnit",
  rust: "Rust",
  go: "Go",
  python: "Python",
  django: "Django",
  svelte: "Svelte",
  sveltekit: "SvelteKit",
  vue: "Vue",
  nuxt: "Nuxt",
  astro: "Astro",
  vite: "Vite",
  tauri: "Tauri",
  terraform: "Terraform",
  kubernetes: "Kubernetes",
  helm: "Helm",
  aws: "AWS",
  docker: "Docker",
  compose: "Docker Compose",
  github: "GitHub",
  "github-actions": "GitHub Actions",
  ios: "iOS",
  android: "Android",
};

/** A stack tag as people write it, with its major version: `Next.js 16`; an unknown one keeps its name. */
export function tagText(tag: { name: string; version?: string | null }): string {
  const major = tag.version?.match(/^\D*(\d+)/)?.[1];
  const name = TAGS[tag.name] ?? tag.name;
  return major ? `${name} ${major}` : name;
}

/** What one analysis by Claude Code may spend, in USD: `termf_onboarding::BUDGET`. */
export const ANALYSIS_BUDGET = 2;

type Landed = { chosen: string[]; analysis: unknown; analyzed: string; failed: string; answers: Record<string, string[]>; question: number };
type Result = { analysis: { questions: { id: string; default: string[] }[] }; error: string | null } | { failure: string };

/**
 * The draft once the analysis of `repos` came back, or null when it is a draft of other repositories now. A failure
 * leaves no analysis, never an older one of other repositories that would pass for this one.
 */
export function withAnalysis<T extends Landed>(draft: T, repos: string[], result: Result): T | null {
  const analyzed = repos.join("\n");
  if (draft.chosen.join("\n") !== analyzed) return null;
  if ("failure" in result) return { ...draft, analysis: null, analyzed, failed: result.failure, question: 0 };
  const answers = Object.fromEntries(result.analysis.questions.map((q) => [q.id, q.default]));
  return { ...draft, analysis: result.analysis, analyzed, failed: result.error ?? "", answers, question: 0 };
}

/**
 * The check commands to write: each repository's (`checkOf`), and an empty one for a command the project had and the
 * owner emptied, which removes it.
 */
export function checksToWrite(paths: string[], typed: Record<string, string>, saved: Record<string, string> | undefined, found: Record<string, Scripts | string | undefined>): Record<string, string> {
  const out: Record<string, string> = {};
  for (const p of paths) {
    const run = checkOf(p, typed, saved, found[p]).trim();
    if (run) out[p] = run;
    else if (saved?.[p]) out[p] = "";
  }
  return out;
}

/**
 * Whether every chosen repository was read (or failed to be): its test command is the check the team runs before
 * delivering, so the setup goes on, and creates, only once each one is known.
 */
export function detectedAll(paths: string[], found: Record<string, unknown>): boolean {
  return paths.length > 0 && paths.every((p) => found[p] !== undefined);
}

type Scripts = { scripts: { kind: string; run: string }[] };

/** A repository's check command: typed by the owner, else the project's, else the test command found in it. */
export function checkOf(path: string, typed: Record<string, string>, saved: Record<string, string> | undefined, found: Scripts | string | undefined): string {
  if (path in typed) return typed[path];
  if (saved?.[path] !== undefined) return saved[path];
  return typeof found === "object" ? (found.scripts.find((s) => s.kind === "test")?.run ?? "") : "";
}

/** The words all the names start with: `acme` for `acme-api` and `acme-web`; "" when they share none. */
export function sharedPrefix(names: string[]): string {
  const words = names.map((n) => n.split(/[-_.\s]+/).filter(Boolean));
  const shared: string[] = [];
  for (let i = 0; words.every((w) => w[i] && w[i].toLowerCase() === words[0][i].toLowerCase()); i++) shared.push(words[0][i]);
  const prefix = shared.join("-");
  return prefix.length > 1 ? prefix : "";
}

/**
 * The project's name, from what was chosen: one repository gives its name; several, the words their names start
 * with, else their GitHub owner when they share one, else the folder they share (never the home folder), else the
 * first one's name.
 */
export function derive(chosen: Card[], home: string): { name: string; from: NameSource } | null {
  if (!chosen.length) return null;
  if (chosen.length === 1) return { name: chosen[0].name, from: "repo" };
  const prefix = sharedPrefix(chosen.map((c) => c.name));
  if (prefix) return { name: prefix, from: "prefix" };
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
