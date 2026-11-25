// No imports, so `pnpm test` runs it in plain Node.

/** In the install script's order, as `termf_core::devenv::Manager` lists them. */
export const MANAGERS = [
  "tap",
  "brew",
  "cask",
  "mas",
  "vscode",
  "shell",
  "mise",
  "asdf",
  "fnm",
  "nvm",
  "volta",
  "rustup",
  "npm",
  "pnpm",
  "bun",
  "cargo",
  "pipx",
  "uv",
  "gh",
  "composer",
  "manual",
] as const;
export type Manager = (typeof MANAGERS)[number];

export const GROUPS = ["languages", "cli", "apps", "editors"] as const;
export type Group = (typeof GROUPS)[number];

export type Entry = {
  group: Group;
  manager: Manager;
  name: string;
  version: string | null;
  /** The App Store id of a `mas` app. */
  id: string | null;
  description: string | null;
  /** The command that installs a `shell` entry, or a `manual` one once known. */
  install: string | null;
  path: string | null;
  active: boolean;
};

/** What the detection found, as the manifest would keep it, with the version installed. */
export type Found = Entry & { installed: string | null };

export type Key = { manager: Manager; name: string };

/** `brew:ripgrep`: how an agent names an entry. */
export const keyOf = (e: Key) => `${e.manager}:${e.name}`;

/** `brew:ripgrep` back to its manager and name; null when it names no manager. */
export function parseKey(text: string): Key | null {
  const at = text.indexOf(":");
  const manager = text.slice(0, at) as Manager;
  const name = text.slice(at + 1).trim();
  return at > 0 && MANAGERS.includes(manager) && name ? { manager, name } : null;
}

/** The group a manager's entries go to, as the backend's detection puts them. */
export function groupOf(manager: Manager): Group {
  if (manager === "cask" || manager === "mas") return "apps";
  if (manager === "vscode") return "editors";
  if (["mise", "asdf", "fnm", "nvm", "rustup"].includes(manager)) return "languages";
  return "cli";
}

export const blank = (manager: Manager = "brew"): Entry => ({
  group: groupOf(manager),
  manager,
  name: "",
  version: null,
  id: null,
  description: null,
  install: null,
  path: null,
  active: true,
});

/** What the detection found that the manifest does not list, once each. */
export function fresh(found: Found[], entries: Entry[]): Found[] {
  const seen = new Set(entries.map(keyOf));
  return found.filter((f) => {
    const key = keyOf(f);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/** An entry on this machine: found by the detection; missing once a detection ended without it; null before. */
export function stateOf(entry: Entry, found: Found[], scanned: boolean): "there" | "missing" | null {
  if (found.some((f) => keyOf(f) === keyOf(entry))) return "there";
  return scanned ? "missing" : null;
}

/** What the install script would install here: the active entries a finished detection did not find. */
export function missing(entries: Entry[], found: Found[], scanned: boolean): Entry[] {
  return scanned ? entries.filter((e) => e.active && stateOf(e, found, true) === "missing") : [];
}

/** Kept by default: what a manager installed. Programs and apps installed by hand wait for a look. */
export const keptByDefault = (f: Found) => f.manager !== "manual";

/** The form's entry as saved: texts trimmed, the empty ones null, the fields its manager has no use for dropped. */
export function clean(e: Entry): Entry {
  const text = (v: string | null) => v?.trim() || null;
  const own = e.manager === "shell" || e.manager === "manual";
  return {
    ...e,
    name: e.name.trim(),
    version: text(e.version),
    id: e.manager === "mas" ? text(e.id) : null,
    description: text(e.description),
    install: own ? text(e.install) : null,
    path: own ? text(e.path) : null,
  };
}

/** One line per entry for the agent's context: `brew:ripgrep 14.1.0 (Search tool) ~/…`. */
export function lines(list: (Entry & { installed?: string | null })[]): string {
  return list
    .map((e) =>
      [keyOf(e), e.version ?? e.installed ?? "", e.description ? `(${e.description})` : "", e.path ?? "", e.active ? "" : "[off]"]
        .filter(Boolean)
        .join(" "),
    )
    .join("\n");
}
