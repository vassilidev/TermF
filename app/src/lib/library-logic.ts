// No imports but types, so `pnpm test` runs it in plain Node.
import type { Item, Layer } from "$lib/library.svelte";

/** An item as the project's sessions get it, merged from what each repository's sessions resolve. */
export type Entry = Item & {
  /** The repositories whose sessions get it, when the project has some. */
  activeIn: string[];
  /** The repository that offers it, for the repository layer. */
  repo: string | null;
  /** Each stack tag that switched it on, and the repository it was found in. */
  found: { tag: string; repo: string }[];
};

const layers: Layer[] = ["shipped", "package", "global", "project", "repo", "run"];

/**
 * `lists`: `library_list` once per repository of the project (`repo` named), or once without any (`repo` null). Where
 * repositories resolve a name in different layers (a package's skill in one, the shipped one in another), the most
 * specific one is shown.
 */
export function merge(lists: { repo: string | null; items: Item[] }[]): Entry[] {
  const byKey = new Map<string, Entry>();
  for (const { repo, items } of lists) {
    for (const item of items) {
      const key = `${item.kind}/${item.name}`;
      let entry = byKey.get(key);
      if (!entry || layers.indexOf(item.layer) > layers.indexOf(entry.layer)) {
        const seen = entry;
        entry = { ...item, activeIn: seen?.activeIn ?? [], repo: item.layer === "repo" ? repo : null, found: seen?.found ?? [] };
        entry.active ||= seen?.active ?? false;
        byKey.set(key, entry);
      }
      entry.active ||= item.active;
      if (item.active && repo !== null) entry.activeIn.push(repo);
      for (const tag of item.activatedBy) {
        if (tag !== "*" && repo !== null) entry.found.push({ tag, repo });
      }
    }
  }
  return [...byKey.values()];
}

/** Answers that come back out of order: `ask()` before a request, then its `current()` says it is still the last. */
export function latestOnly(): () => () => boolean {
  let asked = 0;
  return () => {
    const id = ++asked;
    return () => id === asked;
  };
}

export type Group = "on" | "offered" | "off";

/** The team plays a workflow of its own layers whatever its switch (`termf-team`): there is nothing to switch. */
export const switchable = (e: Pick<Item, "kind" | "layer">) => e.kind !== "workflow" || e.layer === "repo";

/** Off until you switch it on, and nobody did: what a repository offers. */
export function groupOf(e: Entry): Group {
  if (e.active || !switchable(e)) return "on";
  if (e.layer === "repo" && e.toggles.project === undefined && e.toggles.global === undefined) return "offered";
  return "off";
}

const order = ["skill", "command", "role", "preference", "rule", "workflow"];

/** Cut on purpose first, then by kind and name. */
export function sorted(list: Entry[]): Entry[] {
  const cut = (e: Entry) => (e.toggles.project === false || e.toggles.global === false ? 0 : 1);
  return [...list].sort((a, b) => cut(a) - cut(b) || order.indexOf(a.kind) - order.indexOf(b.kind) || a.name.localeCompare(b.name));
}

/** The layer whose word stands: the most specific toggle, else the layer the item comes from (`resolve` in Rust). */
export function decider(e: Entry): Layer {
  if (e.toggles.run !== undefined) return "run";
  if (e.toggles.project !== undefined) return "project";
  if (e.toggles.global !== undefined) return "global";
  return e.layer;
}

/** Why it is on or off, as one short reason: a key of the reasons and what fills it. */
export type Why =
  | { key: "onHere" | "offHere" | "offHereOnlyHere" | "onEverywhere" | "offEverywhere" | "project" | "global" | "always" | "shipped" | "run" | "team" | "workflow" }
  | { key: "found"; tags: string; repos: string }
  | { key: "offered"; repo: string }
  | { key: "package"; name: string; on: boolean };

export function why(e: Entry): Why {
  if (!switchable(e)) return { key: "workflow" };
  if (e.toggles.run !== undefined) return { key: "run" };
  if (e.toggles.project !== undefined) {
    if (e.toggles.project) return { key: "onHere" };
    // Off here only: on by default, or switched on everywhere.
    return e.toggles.global === true || (e.toggles.global === undefined && startsOn(e)) ? { key: "offHereOnlyHere" } : { key: "offHere" };
  }
  if (e.toggles.global !== undefined) return { key: e.toggles.global ? "onEverywhere" : "offEverywhere" };
  switch (e.layer) {
    case "project":
    case "global":
      return { key: e.layer };
    case "repo":
      return { key: "offered", repo: e.repo ?? "" };
    case "package":
      return { key: "package", name: e.package ? (e.package.version ? `${e.package.name} ${e.package.version}` : e.package.name) : "", on: e.active };
  }
  if (e.activatedBy.includes("*")) return { key: "always" };
  if (e.found.length) {
    const unique = (xs: string[]) => [...new Set(xs)].join(", ");
    return { key: "found", tags: unique(e.found.map((f) => f.tag)), repos: unique(e.found.map((f) => f.repo)) };
  }
  // The team plays its roles whatever their switch, which only gives them to the sessions as subagents.
  return { key: e.kind === "role" ? "team" : "shipped" };
}

/** On before any toggle, as `starts_on` in Rust decides it. */
function startsOn(e: Entry): boolean {
  if (e.layer === "global" || e.layer === "project") return true;
  return e.layer === "shipped" && (e.activatedBy.length > 0 || e.found.length > 0);
}

/** What each layer says about the item, the most specific first; `decides` marks the one that stands. */
export type Rung = { layer: "repo" | "project" | "global" | "base"; says: Says; decides: boolean };
export type Says = "on" | "off" | "wrote" | "offered" | "nothing" | "shipped" | "package" | "notShipped";

export function ladder(e: Entry): Rung[] {
  const winner = decider(e);
  const toggle = (on: boolean | undefined): Says | null => (on === undefined ? null : on ? "on" : "off");
  const base: Says = e.layer === "package" ? "package" : e.layer === "shipped" || e.shadows.includes("shipped") ? "shipped" : "notShipped";
  return [
    { layer: "repo", says: e.layer === "repo" ? "offered" : "nothing", decides: winner === "repo" },
    { layer: "project", says: toggle(e.toggles.project) ?? (e.layer === "project" ? "wrote" : "nothing"), decides: winner === "project" },
    { layer: "global", says: toggle(e.toggles.global) ?? (e.layer === "global" ? "wrote" : "nothing"), decides: winner === "global" },
    { layer: "base", says: base, decides: winner === "shipped" || winner === "package" },
  ];
}

/** Roughly what a text costs in context: about four characters a token. */
export const tokens = (text: string | null | undefined) => Math.ceil((text ?? "").length / 4);

/** What it costs in context: `always` at each session or turn, `used` once it serves. */
export function cost(e: Pick<Item, "kind" | "description" | "body">): { always: number; used: number; each: "session" | "turn" | "none" } {
  switch (e.kind) {
    case "preference":
    case "rule":
      return { always: tokens(e.body), used: 0, each: "turn" };
    case "workflow":
      return { always: 0, used: 0, each: "none" };
    default:
      return { always: tokens(e.description), used: tokens(e.body), each: "session" };
  }
}

/** The roles an item names: a skill's `metadata.termf-roles`, a preference's or a rule's `applies_to`. */
export function rolesOf(e: Pick<Item, "kind" | "meta" | "appliesTo">): string[] {
  if (e.kind === "skill") {
    const meta = e.meta.metadata;
    const value = meta && typeof meta === "object" ? (meta as Record<string, unknown>)["termf-roles"] : undefined;
    return typeof value === "string" ? value.split(/[\s,]+/).filter(Boolean) : [];
  }
  return e.appliesTo.filter((r) => r !== "all" && !r.startsWith("squad:"));
}

/** The roles that always bring it, whatever its switch: their `skills` or `preferences` name it. */
export function broughtBy(e: Pick<Item, "kind" | "name">, roles: Pick<Item, "name" | "meta">[]): string[] {
  if (e.kind !== "skill" && e.kind !== "preference") return [];
  const key = e.kind === "skill" ? "skills" : "preferences";
  return roles
    .filter((r) => {
      const list = r.meta[key];
      return (Array.isArray(list) ? list : typeof list === "string" ? list.split(/[\s,]+/) : []).includes(e.name);
    })
    .map((r) => r.name);
}

/** The switches to write so that an item is `on` in the project (`everywhere`: all projects), and back. */
export type Write = { project: string | null; enabled: boolean | null };

export function switchTo(e: Pick<Item, "toggles">, project: string | null, on: boolean, everywhere: boolean): { apply: Write[]; undo: Write[] } {
  const was = (layer: "project" | "global") => e.toggles[layer] ?? null;
  if (project === null || !everywhere) {
    const layer = project === null ? "global" : "project";
    return { apply: [{ project, enabled: on }], undo: [{ project, enabled: was(layer) }] };
  }
  // Everywhere: the global switch, and this project's own one removed so that it follows.
  const apply: Write[] = [{ project: null, enabled: on }];
  const undo: Write[] = [{ project: null, enabled: was("global") }];
  if (e.toggles.project !== undefined) {
    apply.push({ project, enabled: null });
    undo.push({ project, enabled: was("project") });
  }
  return { apply, undo };
}

/** A copy of a shipped item in the global or the project's library: what "Remove this copy" takes back. */
export const isCopy = (e: Pick<Item, "path" | "layer" | "shadows">) =>
  !!e.path && (e.layer === "global" || e.layer === "project") && e.shadows.includes("shipped");
