import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { files } from "$lib/files.svelte";
import { load } from "$lib/state.svelte";

export type Kind = "skill" | "command" | "role" | "preference" | "rule" | "workflow";
export type Layer = "shipped" | "global" | "project" | "repo" | "run";
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
};
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
};

export const kinds: Kind[] = ["skill", "command", "role", "preference", "rule", "workflow"];

type Panel =
  | null
  | { kind: "claude" }
  | { kind: "exported"; files: number; skippedLinks: string[] }
  | { kind: "import"; path: string; preview: Preview; replace: string[]; done: boolean };

export const library = $state({
  /** Loaded once first shown, then kept mounted, like the Files view. */
  loaded: false,
  /** A project id, or "" for the global library. */
  scope: "",
  tab: "skill" as Kind | "conventions",
  panel: null as Panel,
  /** Bumped when the library changed on disk, to reload it. */
  version: 0,
  busy: false,
  error: "",
});

export function showLibrary() {
  files.view = "library";
  library.loaded = true;
}

const archiveFilter = [{ name: "TermF", extensions: ["gz", "tgz"] }];

export async function exportConfig() {
  showLibrary();
  const path = await save({ defaultPath: "termf-config.tar.gz", filters: archiveFilter });
  if (!path) return;
  await busy(async () => {
    const result = await invoke<{ files: number; skippedLinks: string[] }>("config_export", { path });
    library.panel = { kind: "exported", ...result };
  });
}

export async function importConfig() {
  showLibrary();
  const path = await open({ multiple: false, directory: false, filters: archiveFilter });
  if (!path) return;
  await busy(async () => {
    const preview = await invoke<Preview>("config_import_preview", { path });
    library.panel = { kind: "import", path, preview, replace: [], done: false };
  });
}

export async function applyImport() {
  const panel = library.panel;
  if (panel?.kind !== "import") return;
  await busy(async () => {
    panel.preview = await invoke<Preview>("config_import_apply", { path: panel.path, replace: panel.replace });
    panel.done = true;
    await load();
  });
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
