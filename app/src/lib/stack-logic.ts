// No imports, so `pnpm test` runs it in plain Node.

export type Tag = { name: string; version: string | null; source: string };
export type Dep = {
  name: string;
  ecosystem: string;
  manager: string;
  range: string;
  version: string | null;
  locked: boolean;
  dev: boolean;
  manifest: string;
};
export type Service = { name: string; image: string | null; ports: number[] };
export type Database = { engine: string; name: string; host: string; port: number | null; source: string };
export type Workflow = { file: string; name: string; jobs: number };
export type RepoMap = {
  name: string;
  path: string;
  tags: Tag[];
  deps: Dep[];
  services: Service[];
  databases: Database[];
  ports: number[];
  env: string[];
  envFile: string | null;
  workflows: Workflow[];
  publishes: string[];
};
export type LinkKind = "calls" | "package" | "database";
export type Link = { from: string; to: string; kind: LinkKind; via: string[] };
export type StackMap = { repos: RepoMap[]; links: Link[] };
export type Severity = "unknown" | "low" | "moderate" | "high" | "critical";
export type Bump = "patch" | "minor" | "major";
export type Vuln = { id: string; aliases: string[]; summary: string; severity: Severity; fixed: string | null; url: string };
export type DepCheck = {
  name: string;
  ecosystem: string;
  version: string | null;
  latest: string | null;
  safe: string | null;
  bump: Bump | null;
  safeBump: Bump | null;
  vulns: Vuln[];
  /** The vulnerabilities the safe update fixes, by id. */
  safeFixes: string[];
  command: string | null;
};
export type Support = {
  tag: string;
  product: string;
  dep: string | null;
  version: string;
  cycle: string | null;
  eol: string | null;
  ended: boolean;
  lts: boolean;
  latestInCycle: string | null;
  latest: string | null;
  currentLts: string | null;
};
export type Check = { deps: DepCheck[]; support: Support[]; errors: string[]; offline: boolean; checked: number };

export type Filter = "all" | "updates" | "vulnerable" | "eol";

/** A line of a repository's table: a dependency, or a runtime that is none (Node, PHP, Postgres). */
export type Row = { key: string; name: string; version: string | null; dep: Dep | null; check: DepCheck | null; support: Support | null };

export const SEVERITIES: Severity[] = ["critical", "high", "moderate", "low", "unknown"];
const rank = (s: Severity) => SEVERITIES.length - SEVERITIES.indexOf(s);

/** The worst level among `vulns`, null without any. */
export function worst(vulns: Vuln[]): Severity | null {
  return vulns.reduce<Severity | null>((w, v) => (!w || rank(v.severity) > rank(w) ? v.severity : w), null);
}

/** How far a support's end is on `today` (`YYYY-MM-DD`): ended, within six months, or not yet. */
export function ending(s: Support | null, today: string): "ended" | "soon" | null {
  if (!s) return null;
  if (s.ended) return "ended";
  if (!s.eol) return null;
  const days = (Date.parse(s.eol) - Date.parse(today)) / 86_400_000;
  return days <= 183 ? "soon" : null;
}

/** The repository's lines: runtimes first, then the worst first (vulnerable, out of support, behind), by name. */
export function rows(repo: RepoMap, check: Check | null, today: string): Row[] {
  const support = check?.support ?? [];
  const runtimes: Row[] = support
    .filter((s) => !s.dep || !repo.deps.some((d) => d.name === s.dep))
    .map((s) => ({ key: `runtime:${s.tag}`, name: s.tag, version: s.version, dep: null, check: null, support: s }));
  const deps: Row[] = repo.deps.map((d) => ({
    key: `${d.ecosystem}:${d.name}:${d.version ?? ""}:${d.manifest}`,
    name: d.name,
    version: d.version,
    dep: d,
    check: check?.deps.find((c) => c.ecosystem === d.ecosystem && c.name === d.name && c.version === d.version) ?? null,
    support: support.find((s) => s.dep === d.name) ?? null,
  }));
  const weight = (r: Row) => {
    const w = worst(r.check?.vulns ?? []);
    const end = ending(r.support, today);
    return (w ? 100 + rank(w) * 10 : 0) + (end === "ended" ? 50 : end === "soon" ? 20 : 0) + (r.check?.bump === "major" ? 4 : r.check?.safe ? 2 : 0);
  };
  deps.sort((a, b) => weight(b) - weight(a) || Number(a.dep?.dev) - Number(b.dep?.dev) || a.name.localeCompare(b.name));
  return [...runtimes, ...deps];
}

export function matches(row: Row, filter: Filter, query: string, today: string): boolean {
  if (query && !row.name.toLowerCase().includes(query.trim().toLowerCase())) return false;
  switch (filter) {
    case "updates":
      return !!(row.check?.safe || row.check?.bump);
    case "vulnerable":
      return !!row.check?.vulns.length;
    case "eol":
      return ending(row.support, today) !== null;
    default:
      return true;
  }
}

/** What a repository's lines add up to: for its filters and its node on the map. */
export function tally(list: Row[], today: string) {
  const bySeverity = Object.fromEntries(SEVERITIES.map((s) => [s, 0])) as Record<Severity, number>;
  let updates = 0;
  let safe = 0;
  let major = 0;
  let vulnerable = 0;
  let eol = 0;
  for (const r of list) {
    if (r.check?.safe || r.check?.bump) updates++;
    if (r.check?.safe) safe++;
    if (r.check?.bump === "major") major++;
    if (r.check?.vulns.length) vulnerable++;
    for (const v of r.check?.vulns ?? []) bySeverity[v.severity]++;
    if (ending(r.support, today)) eol++;
  }
  return { all: list.length, updates, safe, major, vulnerable, bySeverity, eol, worst: SEVERITIES.find((s) => bySeverity[s]) ?? null };
}

/** The update a line proposes: the safe one when there is one (a vulnerability's fix included), else the major. */
export function proposal(row: Row): { to: string; major: boolean } | null {
  const c = row.check;
  if (!c?.latest) return null;
  if (c.safe) return { to: c.safe, major: false };
  return c.bump ? { to: c.latest, major: c.bump === "major" } : null;
}

export type Box = { x: number; y: number; w: number; h: number };
export type DbNode = Box & { key: string; db: Database; repos: string[] };
export type Layout = { repos: Box[]; dbs: DbNode[]; width: number; height: number; top: number };

const NODE = { w: 196, h: 96, gap: 28 };
const DB = { w: 164, h: 46, gap: 18 };
const LOCAL = ["localhost", "127.0.0.1", "0.0.0.0", "host.docker.internal", "::1", ""];

/** A database's identity across repositories, as the links read it; a SQLite file is its repository's own. */
export function dbKey(db: Database, repo: string): string {
  const host = LOCAL.includes(db.host) ? "local" : db.host.toLowerCase();
  return db.engine === "sqlite" || !db.name ? `${repo}|${db.source}` : `${db.engine}|${host}|${db.port ?? ""}|${db.name}`;
}

/** How far an arc between two nodes of a row rises: the farther apart, the higher, so that arcs nest. */
export const rise = (slots: number) => 30 + 20 * slots;

/**
 * Where the map draws the repositories (rows of up to as many as fit `width`) and, under them, each database once
 * with the repositories that use it. `top` leaves room for the arcs between the repositories of a row.
 */
export function layout(map: StackMap, width: number): Layout {
  const n = map.repos.length;
  const per = Math.max(1, Math.min(n, Math.floor((width + NODE.gap) / (NODE.w + NODE.gap))));
  const rowsOf = Math.ceil(n / per);
  const linked = map.links.filter((l) => l.kind !== "database");
  const index = new Map(map.repos.map((r, i) => [r.path, i]));
  const spread = linked.reduce((m, l) => Math.max(m, Math.abs((index.get(l.from) ?? 0) - (index.get(l.to) ?? 0))), 0);
  const top = linked.length ? rise(Math.min(spread, per)) + 12 : 12;
  const repos: Box[] = map.repos.map((_, i) => {
    const row = Math.floor(i / per);
    const inRow = Math.min(per, n - row * per);
    const span = inRow * NODE.w + (inRow - 1) * NODE.gap;
    const x = Math.max(0, (width - span) / 2) + (i % per) * (NODE.w + NODE.gap);
    return { x, y: top + row * (NODE.h + 56), w: NODE.w, h: NODE.h };
  });
  const dbs = new Map<string, { db: Database; repos: string[] }>();
  for (const repo of map.repos) {
    for (const db of repo.databases) {
      const key = dbKey(db, repo.path);
      const known = dbs.get(key);
      if (known) {
        if (!known.repos.includes(repo.path)) known.repos.push(repo.path);
      } else dbs.set(key, { db, repos: [repo.path] });
    }
  }
  const below = top + rowsOf * (NODE.h + 56) + 8;
  const dbPer = Math.max(1, Math.floor((width + DB.gap) / (DB.w + DB.gap)));
  const list = [...dbs.entries()];
  const placed: DbNode[] = list.map(([key, { db, repos: users }], i) => {
    const row = Math.floor(i / dbPer);
    const inRow = Math.min(dbPer, list.length - row * dbPer);
    const span = inRow * DB.w + (inRow - 1) * DB.gap;
    const x = Math.max(0, (width - span) / 2) + (i % dbPer) * (DB.w + DB.gap);
    return { key, db, repos: users, x, y: below + row * (DB.h + DB.gap), w: DB.w, h: DB.h };
  });
  const height = placed.length ? Math.max(...placed.map((d) => d.y + d.h)) + 4 : below - 48;
  return { repos, dbs: placed, width, height, top };
}

/**
 * The SVG path of a link between two repositories: an arc over a row (`lane` spreads the links of one pair), or a
 * curve between rows.
 */
export function arc(a: Box, b: Box, lane = 0): string {
  const offset = (lane - 0.5) * 10;
  const ax = a.x + a.w / 2 + offset;
  const bx = b.x + b.w / 2 + offset;
  if (a.y === b.y) {
    const slots = Math.round(Math.abs(bx - ax) / (a.w + 28));
    const h = rise(slots) - Math.abs(offset) / 2;
    return `M ${ax} ${a.y} C ${ax} ${a.y - h}, ${bx} ${b.y - h}, ${bx} ${b.y}`;
  }
  const down = a.y < b.y;
  const y1 = down ? a.y + a.h : a.y;
  const y2 = down ? b.y : b.y + b.h;
  const mid = (y1 + y2) / 2;
  return `M ${ax} ${y1} C ${ax} ${mid}, ${bx} ${mid}, ${bx} ${y2}`;
}

/** From a repository down to a database it uses. */
export function drop(repo: Box, db: Box): string {
  const x1 = repo.x + repo.w / 2;
  const y1 = repo.y + repo.h;
  const x2 = db.x + db.w / 2;
  const mid = (y1 + db.y) / 2;
  return `M ${x1} ${y1} C ${x1} ${mid}, ${x2} ${mid}, ${x2} ${db.y}`;
}

/** The main tags a node shows: languages and frameworks with a version first, tools after. */
export function headline(tags: Tag[], max = 3): Tag[] {
  const minor = ["web", "docker", "compose", "github-actions", "typescript", "sql"];
  return [...tags].sort((a, b) => Number(minor.includes(a.name)) - Number(minor.includes(b.name)) || Number(!a.version) - Number(!b.version)).slice(0, max);
}

/** A version as a node shows it: its major and minor. */
export const short = (v: string | null) => (v ? v.replace(/^v/, "").split(".").slice(0, 2).join(".") : "");
