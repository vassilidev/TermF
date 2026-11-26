// No imports, so `pnpm test` runs it in plain Node.

const parts = (path: string) => path.split(/[\\/]+/).filter(Boolean);

function separator(path: string): string {
  return path.includes("\\") && !path.includes("/") ? "\\" : "/";
}

/** `~/x` as a full path under `home`; "" when home is not known yet. */
export function expandHome(path: string, home: string): string {
  if (path !== "~" && !/^~[\\/]/.test(path)) return path;
  if (!home) return "";
  return home.replace(/[\\/]+$/, "") + separator(home) + path.slice(2).replace(/[\\/]/g, separator(home));
}

/** The repository's own folder name inside `folder`. */
export function inside(folder: string, path: string): string {
  const name = parts(path).pop() ?? "";
  return folder.replace(/[\\/]+$/, "") + separator(folder) + name;
}

/**
 * Where to clone a missing repository: beside the repositories of its project that are on this machine, else
 * where the other machine has it. "" when that needs the home folder and it is not known yet.
 */
export function cloneTarget(path: string, present: string[], home: string): string {
  const sibling = present.map((p) => expandHome(p, home)).find(Boolean);
  if (!sibling) return expandHome(path, home);
  const parent = sibling.replace(/[\\/]+[^\\/]+[\\/]*$/, "");
  return inside(parent, path);
}

/** The scope of a synced path, as `termf-sync` files it (`local::scope_of`). */
export function scopeOf(path: string): string {
  const [first, , third] = path.split("/");
  if (first === "projects") return third === "conventions" ? "conventions" : "projects";
  if (first === "vault") return "vault";
  if (first === "ssh") return "sshKeys";
  if (first === "dev") return "devConfig";
  return "config";
}

/** A synced path as the user knows it: the file of the home folder, a key by its name. */
export function shownPath(path: string): string {
  if (path.startsWith("ssh/keys/")) return path.slice("ssh/keys/".length);
  if (path.startsWith("ssh/")) return "~/.ssh/" + path.slice("ssh/".length);
  if (path === "dev/gitconfig") return "~/.gitconfig";
  if (path === "dev/claude.json") return "~/.claude.json";
  if (path.startsWith("dev/claude/") && !path.startsWith("dev/claude/memory/")) {
    return "~/.claude/" + path.slice("dev/claude/".length);
  }
  return path;
}

export type Change = "conflict" | "merged" | "kept" | "added" | "sent";
export type PreviewItem = { path: string; change: Change; keys: string[]; to?: string };
type Plan = {
  pulled: string[];
  pushed: string[];
  merged: string[];
  renamed: { path: string; to: string }[];
  conflicts: { path: string; keys: string[] }[];
};

const rank: Record<Change, number> = { conflict: 0, merged: 1, kept: 2, added: 3, sent: 4 };

/** What joining a sync would do here, by scope in the order given, what needs a look first. */
export function previewGroups(plan: Plan, order: string[]): { scope: string; items: PreviewItem[] }[] {
  const items = new Map<string, PreviewItem>();
  const set = (path: string, change: Change, keys: string[] = [], to?: string) =>
    items.set(path, { path, change, keys, to });
  for (const path of plan.pushed) set(path, "sent");
  for (const path of plan.pulled) set(path, "added");
  for (const path of plan.merged) set(path, "merged");
  for (const r of plan.renamed) {
    items.delete(r.to);
    set(r.path, "kept", [], r.to);
  }
  for (const c of plan.conflicts) set(c.path, "conflict", c.keys);
  return order
    .map((scope) => ({
      scope,
      items: [...items.values()]
        .filter((i) => scopeOf(i.path) === scope)
        .sort((a, b) => rank[a.change] - rank[b.change] || a.path.localeCompare(b.path)),
    }))
    .filter((g) => g.items.length);
}

export type Counts = { conflict: number; merge: number; receive: number; send: number };

/** Each synced scope, in the order given, with what a sync would do to it now; no item: the same as the sync. */
export function compareRows(plan: Plan, synced: string[]): { scope: string; items: PreviewItem[]; counts: Counts }[] {
  const groups = previewGroups(plan, synced);
  return synced.map((scope) => {
    const items = groups.find((g) => g.scope === scope)?.items ?? [];
    const n = (...of: Change[]) => items.filter((i) => of.includes(i.change)).length;
    return { scope, items, counts: { conflict: n("conflict"), merge: n("merged"), receive: n("added", "kept"), send: n("sent") } };
  });
}
