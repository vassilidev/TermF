// No imports, so `pnpm test` runs it in plain Node.

export type Kind = "login" | "note";
export type ItemInfo = {
  id: string;
  kind: Kind;
  project: string | null;
  title: string;
  urls: string[];
  username: string;
  hasPassword: boolean;
  totp: { algorithm: "SHA1" | "SHA256" | "SHA512"; digits: number; period: number } | null;
  hasNotes: boolean;
  agents: boolean;
  created: number;
  updated: number;
  /** Other versions a merge kept beside it, for the user to choose from. */
  conflicts: number;
};
export type Scope = "dev" | "staging" | "prod";
export type SecretInfo = {
  name: string;
  scope: Scope;
  project: string | null;
  description: string;
  created: number;
  updated: number;
  reference: string | null;
  conflicts: number;
};
export type Entry = { key: string; project: string | null } & (
  | { type: "item"; item: ItemInfo }
  | { type: "secret"; secret: SecretInfo }
);
export type Filter = "all" | "login" | "note" | "secret";

const fold = (s: string) =>
  s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();

/** As termf-vault's `SecretId`: letters, digits, _ . and -, 128 at most, starting with a letter or _. */
export const secretNameOk = (name: string) => /^[A-Za-z_][\w.-]{0,127}$/.test(name);

/** One line of the vault's audit log, as `vault_audit` gives it (newest first); never a value. */
export type AuditEntry = {
  id: number;
  at: number;
  actor: { kind: "human" | "agent" | "termf"; run: number | null; session: string | null };
  action: string;
  target: string | null;
  scope: Scope | null;
  project: string | null;
  detail: string | null;
};
export type AuditCheck = { ok: boolean; entries: number; broken: number | null; error: string | null };

export const secretKey = (s: SecretInfo) => `secret:${s.scope}:${s.project ?? ""}:${s.name}`;

export function entries(items: ItemInfo[], secrets: SecretInfo[]): Entry[] {
  return [
    ...items.map((item): Entry => ({ type: "item", key: item.id, project: item.project, item })),
    ...secrets.map((secret): Entry => ({ type: "secret", key: secretKey(secret), project: secret.project, secret })),
  ];
}

function words(entry: Entry, projectName: (id: string) => string): string {
  const project = entry.project ? projectName(entry.project) : "";
  if (entry.type === "secret") {
    const s = entry.secret;
    return [s.name, s.scope, s.description, s.reference ?? "", project].join(" ");
  }
  const i = entry.item;
  return [i.title, i.username, ...i.urls, project].join(" ");
}

export function visible(
  list: Entry[],
  project: string | null,
  filter: Filter,
  query: string,
  projectName: (id: string) => string = (id) => id,
): Entry[] {
  const asked = fold(query).split(/\s+/).filter(Boolean);
  return list
    .filter((e) => project === null || e.project === null || e.project === project)
    .filter((e) => filter === "all" || (e.type === "secret" ? filter === "secret" : e.item.kind === filter))
    .filter((e) => {
      const text = fold(words(e, projectName));
      return asked.every((w) => text.includes(w));
    })
    .sort(
      (a, b) =>
        Number(a.type === "secret") - Number(b.type === "secret") ||
        fold(title(a)).localeCompare(fold(title(b))) ||
        a.key.localeCompare(b.key),
    );
}

export const title = (e: Entry) => (e.type === "item" ? e.item.title : e.secret.name);

export function sections(
  list: Entry[],
  current: string | null,
  projectName: (id: string) => string = (id) => id,
): { project: string | null; entries: Entry[] }[] {
  const ids = [...new Set(list.map((e) => e.project))];
  const rank = (id: string | null) => (id === null ? 2 : id === current ? 0 : 1);
  ids.sort((a, b) => rank(a) - rank(b) || projectName(a ?? "").localeCompare(projectName(b ?? "")));
  return ids.map((project) => ({ project, entries: list.filter((e) => e.project === project) }));
}

/** Two halves are easier to read out and type. */
export function spaced(code: string): string {
  const half = Math.ceil(code.length / 2);
  return code.length >= 6 ? `${code.slice(0, half)} ${code.slice(half)}` : code;
}

export function secondsLeft(period: number, nowMs: number): number {
  return period - (Math.floor(nowMs / 1000) % period);
}

export function host(url: string): string {
  try {
    return new URL(/^[a-z][a-z0-9+.-]*:\/\//i.test(url) ? url : `https://${url}`).host || url;
  } catch {
    return url;
  }
}

/** http(s) only: any other scheme could run something or open a local file. */
export function openable(url: string): string | null {
  const full = /^[a-z][a-z0-9+.-]*:\/\//i.test(url) ? url : `https://${url}`;
  try {
    const parsed = new URL(full);
    return parsed.protocol === "https:" || parsed.protocol === "http:" ? parsed.href : null;
  } catch {
    return null;
  }
}

/** Not on commas: an address may hold some (a map's coordinates). */
export const urlsOf = (text: string) => text.split(/\s+/).filter(Boolean);

export type Recipe =
  | { kind: "password"; length: number; lower: boolean; upper: boolean; digits: boolean; symbols: boolean; unambiguous: boolean }
  | { kind: "passphrase"; words: number; separator: string };

/** Of the recipe, not of the password drawn: what makes it strong, whatever it looks like. */
export function strength(recipe: Recipe): number {
  if (recipe.kind === "passphrase") return Math.floor(recipe.words * Math.log2(1296));
  const ambiguous = recipe.unambiguous;
  const size =
    (recipe.lower ? (ambiguous ? 24 : 26) : 0) +
    (recipe.upper ? (ambiguous ? 24 : 26) : 0) +
    (recipe.digits ? (ambiguous ? 8 : 10) : 0) +
    (recipe.symbols ? 17 : 0);
  return size ? Math.floor(recipe.length * Math.log2(size)) : 0;
}

/** What a merge (the sync, an import) does to one element: its title, never a value. */
export type Change = "added" | "changed" | "deleted" | "conflict" | "unchanged";
export type Merged = { key: string; kind: Kind | null; scope: Scope | null; title: string; project: string | null; change: Change };
export type ConflictInfo = {
  id: string;
  at: number;
  item: ItemInfo | null;
  secret: SecretInfo | null;
  differs: string[];
};
export type ForeignRow = {
  index: number;
  kind: Kind;
  title: string;
  username: string;
  urls: string[];
  folder: string;
  hasPassword: boolean;
  hasTotp: boolean;
  hasNotes: boolean;
  /** The title of the item it duplicates; "" for a line above it in the file. */
  duplicate: string | null;
  problem: "trash" | "totp" | null;
};
export type ForeignPreview = { source: string; digest: string; rows: ForeignRow[]; folders: string[]; unsupported: number };

/** How many elements each change touched, the unchanged ones left out. */
export function counts(rows: Merged[]): [Exclude<Change, "unchanged">, number][] {
  const order = ["added", "changed", "deleted", "conflict"] as const;
  return order
    .map((c): [Exclude<Change, "unchanged">, number] => [c, rows.filter((r) => r.change === c).length])
    .filter(([, n]) => n > 0);
}

/** Lines left out unless the user takes them back: a duplicate, or what sat in the other manager's trash. */
export const leftOut = (rows: ForeignRow[]) =>
  rows.filter((r) => r.duplicate !== null || r.problem === "trash").map((r) => r.index);

/** A folder goes into the project of the same id or name, else the only one whose name starts like it, else stays
 * global. */
export function folderProject(folder: string, projects: { id: string; name: string }[]): string | null {
  const name = fold(folder.split("/").pop() ?? "").trim();
  if (!name) return null;
  const same = projects.find((p) => fold(p.id) === name || fold(p.name) === name);
  if (same) return same.id;
  const like = projects.filter((p) => fold(p.id).startsWith(name) || fold(p.name).startsWith(name));
  return name.length >= 3 && like.length === 1 ? like[0].id : null;
}
