// No imports, so `pnpm test` runs it in plain Node.

export type Kind = "login" | "note";
/** What the vault says about an item, never a value. */
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
};
/** One row of the list: an item, or one of the vault's secrets (dev, staging, prod). */
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

export const secretKey = (s: SecretInfo) => `secret:${s.scope}:${s.project ?? ""}:${s.name}`;

export function entries(items: ItemInfo[], secrets: SecretInfo[]): Entry[] {
  return [
    ...items.map((item): Entry => ({ type: "item", key: item.id, project: item.project, item })),
    ...secrets.map((secret): Entry => ({ type: "secret", key: secretKey(secret), project: secret.project, secret })),
  ];
}

/** What the search looks in: never a value, the vault gives none. */
function words(entry: Entry, projectName: (id: string) => string): string {
  const project = entry.project ? projectName(entry.project) : "";
  if (entry.type === "secret") {
    const s = entry.secret;
    return [s.name, s.scope, s.description, s.reference ?? "", project].join(" ");
  }
  const i = entry.item;
  return [i.title, i.username, ...i.urls, project].join(" ");
}

/**
 * The entries `project` sees (its own and the global ones; every one for `null`) that match the filter and every word
 * of the query, case and accents aside; by title.
 */
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
    .sort((a, b) => fold(title(a)).localeCompare(fold(title(b))) || a.key.localeCompare(b.key));
}

export const title = (e: Entry) => (e.type === "item" ? e.item.title : e.secret.name);

/**
 * The list cut by project: the current one first, then the others by name, the global entries last. Within each, the
 * order of `list`.
 */
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

/** "123 456", "1234 5678": two halves are easier to read out and type. */
export function spaced(code: string): string {
  const half = Math.ceil(code.length / 2);
  return code.length >= 6 ? `${code.slice(0, half)} ${code.slice(half)}` : code;
}

/** Seconds until the code valid at `nowMs` changes. */
export function secondsLeft(period: number, nowMs: number): number {
  return period - (Math.floor(nowMs / 1000) % period);
}

/** The host of an address, for the list; the address itself when it has none. */
export function host(url: string): string {
  try {
    return new URL(/^[a-z][a-z0-9+.-]*:\/\//i.test(url) ? url : `https://${url}`).host || url;
  } catch {
    return url;
  }
}

/** An address the system can open: http(s) only, a bare host taken as https. */
export function openable(url: string): string | null {
  const full = /^[a-z][a-z0-9+.-]*:/i.test(url) ? url : `https://${url}`;
  try {
    const parsed = new URL(full);
    return parsed.protocol === "https:" || parsed.protocol === "http:" ? parsed.href : null;
  } catch {
    return null;
  }
}

/** The addresses typed one per line (or separated by spaces or commas), blanks dropped. */
export const urlsOf = (text: string) => text.split(/[\s,]+/).filter(Boolean);

export type Recipe =
  | { kind: "password"; length: number; lower: boolean; upper: boolean; digits: boolean; symbols: boolean; unambiguous: boolean }
  | { kind: "passphrase"; words: number; separator: string };

/** Bits of entropy of what the recipe makes, rounded down: what makes a password strong, whatever it looks like. */
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
