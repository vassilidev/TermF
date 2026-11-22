// No imports, so `pnpm test` runs it in plain Node.

type Row = {
  id: string;
  url: string;
  title: string | null;
  owner: "me" | "shared" | null;
  updated: string | null;
  cwd: string | null;
  seen: number;
};

export type SortKey = "title" | "origin" | "owner" | "updated";
export type Sort = { key: SortKey; dir: "asc" | "desc" };
export type Filter = { text: string; owner: "" | "me" | "shared"; project: string };

/** When it was last updated, in ms: the account's date when it reads as one, else when TermF last saw it. */
export function updatedAt(a: Row): number {
  const at = a.updated ? Date.parse(a.updated) : NaN;
  return Number.isNaN(at) ? a.seen * 1000 : at;
}

/** The account's date as a day, or `null` when it does not read as one. A date without a time is a calendar day:
 *  read as midnight UTC, it is shown in UTC too, or it would fall a day early west of Greenwich. */
export function updatedDay(updated: string, locale: string): { text: string; at: number } | null {
  const at = Date.parse(updated);
  if (Number.isNaN(at)) return null;
  const timeZone = /^\d{4}-\d{2}-\d{2}$/.test(updated.trim()) ? "UTC" : undefined;
  return { text: new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeZone }).format(at), at };
}

/** `projectsOf`: the projects a folder belongs to. */
export function filterArtifacts<T extends Row>(list: T[], filter: Filter, projectsOf: (cwd: string) => string[]): T[] {
  const words = filter.text.toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return list.filter((a) => {
    if (filter.owner && a.owner !== filter.owner) return false;
    if (filter.project && !(a.cwd && projectsOf(a.cwd).includes(filter.project))) return false;
    const text = [a.title, a.url, a.cwd].join(" ").toLocaleLowerCase();
    return words.every((w) => text.includes(w));
  });
}

/** Clicking the sorted column turns it around; another column starts with the order it is read in. */
export function nextSort(sort: Sort, key: SortKey): Sort {
  if (sort.key === key) return { key, dir: sort.dir === "asc" ? "desc" : "asc" };
  return { key, dir: key === "updated" ? "desc" : "asc" };
}

/** Rows without the sorted value come last either way; ties go to the most recently updated. `origin`: the
 * label shown for its folder, "" without one. */
export function sortArtifacts<T extends Row>(list: T[], sort: Sort, origin: (a: T) => string): T[] {
  const value = (a: T) => (sort.key === "title" ? a.title : sort.key === "origin" ? origin(a) || null : a.owner);
  const sign = sort.dir === "asc" ? 1 : -1;
  return [...list].sort((a, b) => {
    if (sort.key === "updated") return sign * (updatedAt(a) - updatedAt(b)) || a.id.localeCompare(b.id);
    const [x, y] = [value(a), value(b)];
    if (x === null || y === null) {
      if (x !== y) return x === null ? 1 : -1;
    } else {
      const by = x.localeCompare(y, undefined, { sensitivity: "base", numeric: true });
      if (by) return sign * by;
    }
    return updatedAt(b) - updatedAt(a) || a.id.localeCompare(b.id);
  });
}

/** The header checkbox: every shown row selected, some, or none. */
export function selection(selected: string[], shown: { id: string }[]): "all" | "some" | "none" {
  const n = shown.filter((a) => selected.includes(a.id)).length;
  return n === 0 ? "none" : n === shown.length ? "all" : "some";
}

/** The header checkbox selects every shown row, or clears them, keeping what filters hide. */
export function toggleAll(selected: string[], shown: { id: string }[]): string[] {
  const ids = shown.map((a) => a.id);
  if (selection(selected, shown) === "all") return selected.filter((id) => !ids.includes(id));
  return [...new Set([...selected, ...ids])];
}

type Overview = {
  settings: { files: { path: string; state: string; error: string | null }[] };
  mcp: { servers: { name: string; health: string }[]; error: string | null };
  plugins: { error: string | null };
  account: { auth: Record<string, unknown> | null; authError: string | null };
};

export type Alert =
  | { section: "mcp"; tone: "error" | "warn"; server: string; health: string }
  | { section: "mcp" | "account"; tone: "error"; text: string }
  | { section: "settings"; tone: "error"; file: string; error: string | null }
  | { section: "account"; tone: "warn"; loggedOut: true };

/** What needs a look in the machine's config, worst first. */
export function alerts(o: Overview): Alert[] {
  const out: Alert[] = [];
  for (const f of o.settings.files.filter((f) => f.state === "invalid")) {
    out.push({ section: "settings", tone: "error", file: f.path, error: f.error });
  }
  if (o.mcp.error) out.push({ section: "mcp", tone: "error", text: o.mcp.error });
  if (o.plugins.error) out.push({ section: "mcp", tone: "error", text: o.plugins.error });
  if (o.account.authError) out.push({ section: "account", tone: "error", text: o.account.authError });
  else if (o.account.auth && o.account.auth.loggedIn === false) out.push({ section: "account", tone: "warn", loggedOut: true });
  for (const s of o.mcp.servers) {
    if (s.health === "failed") out.push({ section: "mcp", tone: "error", server: s.name, health: s.health });
  }
  for (const s of o.mcp.servers) {
    if (s.health === "needsauth" || s.health === "pending") {
      out.push({ section: "mcp", tone: "warn", server: s.name, health: s.health });
    }
  }
  return out;
}

type Schema = Record<string, unknown>;

/** How the settings editor shows a key: a form control for what the schema describes simply, JSON for the rest
 * (objects, lists, mixed forms, keys the schema does not know). */
export type Field = { kind: "text" | "number" | "bool" | "enum" | "json"; values: string[]; description: string };

export function field(schema: Schema, key: string): Field {
  const defs = (schema.$defs ?? {}) as Record<string, Schema>;
  const resolve = (n: Schema | undefined) => {
    const ref = typeof n?.$ref === "string" ? n.$ref.replace("#/$defs/", "") : null;
    return ref ? defs[ref] : n;
  };
  let node: Schema | undefined = schema;
  for (const part of key.split(".")) {
    const props = node?.properties as Record<string, Schema> | undefined;
    const extra = node?.additionalProperties;
    node = resolve(props?.[part] ?? (extra && typeof extra === "object" ? (extra as Schema) : undefined));
  }
  const description = typeof node?.description === "string" ? node.description.split("\n")[0] : "";
  const json: Field = { kind: "json", values: [], description };
  if (!node) return json;
  const branches = (node.anyOf ?? node.oneOf) as Schema[] | undefined;
  const type = node.type ?? (branches?.every((b) => b.type === "string") ? "string" : undefined);
  if (Array.isArray(node.enum) && node.enum.every((v) => typeof v === "string")) {
    return { kind: "enum", values: node.enum as string[], description };
  }
  if (type === "boolean") return { kind: "bool", values: [], description };
  if (type === "integer" || type === "number") return { kind: "number", values: [], description };
  if (type === "string") return { kind: "text", values: [], description };
  return json;
}
