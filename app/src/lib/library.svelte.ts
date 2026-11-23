import { app, ui } from "$lib/state.svelte";

export type Kind = "skill" | "command" | "role" | "preference" | "rule" | "workflow";
export type Layer = "shipped" | "package" | "global" | "project" | "repo" | "run";
export type Package = { name: string; version: string | null };
export type Item = {
  kind: Kind;
  name: string;
  description: string | null;
  appliesTo: string[];
  tags: string[];
  runtime: string | null;
  meta: Record<string, unknown>;
  body: string;
  error: string | null;
  layer: Layer;
  path: string | null;
  shadows: Layer[];
  toggles: Partial<Record<Layer, boolean>>;
  active: boolean;
  /** The stack's tags its `termf-stacks` names (`*`: every stack), which switch a shipped skill on. */
  activatedBy: string[];
  /** The installed package that ships it, for the package layer. */
  package: Package | null;
};
/** A repository of the project, as `library_stack` reads it. */
export type RepoStack = {
  name: string;
  path: string;
  tags: { name: string; version: string | null; source: string }[];
  skills: { name: string; path: string; package: Package }[];
  guides: { package: Package; path: string }[];
  trusted: boolean;
  /** Installed ones go to the project's sessions, unless `library_lsp` turned them off. */
  servers: { name: string; for: string; command: string; installed: boolean; docs: string }[];
  mcp: { name: string; for: string; server: Record<string, unknown>; docs: string; runsCode: boolean }[];
  sources: { repo: string; for: string; url: string; command: string; license: string }[];
};

export const packageLabel = (p: Package) => (p.version ? `${p.name} ${p.version}` : p.name);
export type Imported = {
  kind: Kind;
  name: string;
  outcome: "imported" | "replaced" | "skipped" | "unsupported";
};
export type Change = {
  item: string;
  status: "added" | "unchanged" | "changed" | "conflict";
  addedKeys: string[];
  conflicts: string[];
  linked: boolean;
};
export type Preview = {
  changes: Change[];
  missingRepos: { project: string; path: string; remote: string | null }[];
  /** The archive holds the vault, sealed by its own export password. */
  vault: boolean;
};

export const kinds: Kind[] = ["skill", "command", "role", "preference", "rule", "workflow"];

type Panel = null | { kind: "claude" } | { kind: "scout" };

export const library = $state({
  /** Loaded once first shown, then kept mounted, like the Files view. */
  loaded: false,
  /** The global layer, even with a current project. */
  global: false,
  /** A project id, or "" for the global library: the current project's layer unless `global`. */
  get scope(): string {
    return this.global ? "" : (app.project ?? "");
  },
  tab: "items" as "items" | "conventions" | "worktrees",
  /** The kind the list shows, or all of them. */
  kind: "all" as Kind | "plugin" | "all",
  /** The details' switch acts on every project instead of the current one. */
  everywhere: false,
  panel: null as Panel,
  /** Bumped when the library changed on disk, to reload it. */
  version: 0,
  busy: false,
  error: "",
});

export function showLibrary() {
  ui.view = "library";
  library.loaded = true;
}

/** What importing does with a change, as `Change::action` decides it. */
export function action(change: Change, replace: string[]): "write" | "replace" | "merge" | "keep" {
  if (change.linked || change.status === "unchanged") return "keep";
  if (change.status === "added") return "write";
  if (replace.includes(change.item)) return "replace";
  return change.addedKeys.length ? "merge" : "keep";
}

export async function busy(run: () => Promise<void>) {
  library.busy = true;
  library.error = "";
  try {
    await run();
  } catch (e) {
    library.error = String(e);
  } finally {
    library.busy = false;
  }
}

export type Line = { op: " " | "+" | "-"; text: string };

/** Line diff of `a` → `b` (longest common subsequence), for the short files a proposal changes. */
export function lineDiff(a: string, b: string): Line[] {
  const x = a ? a.replace(/\n$/, "").split("\n") : [];
  const y = b ? b.replace(/\n$/, "").split("\n") : [];
  if (x.length * y.length > 4_000_000) {
    return [...x.map((text) => ({ op: "-" as const, text })), ...y.map((text) => ({ op: "+" as const, text }))];
  }
  const lcs = Array.from({ length: x.length + 1 }, () => new Uint32Array(y.length + 1));
  for (let i = x.length - 1; i >= 0; i--) {
    for (let j = y.length - 1; j >= 0; j--) {
      lcs[i][j] = x[i] === y[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }
  const out: Line[] = [];
  let i = 0;
  let j = 0;
  while (i < x.length || j < y.length) {
    if (i < x.length && j < y.length && x[i] === y[j]) {
      out.push({ op: " ", text: x[i++] });
      j++;
    } else if (i < x.length && (j === y.length || lcs[i + 1][j] >= lcs[i][j + 1])) {
      out.push({ op: "-", text: x[i++] });
    } else {
      out.push({ op: "+", text: y[j++] });
    }
  }
  return out;
}

const fold = (s: string) =>
  s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();

/** A convention name typed by the owner, as the backend accepts it: `[a-z0-9-]`. */
export function slugify(name: string): string {
  return fold(name)
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}

export function matches(item: Item, query: string): boolean {
  const q = fold(query.trim());
  if (!q) return true;
  return [item.name, item.description ?? "", ...item.tags].some((s) => fold(s).includes(q));
}
