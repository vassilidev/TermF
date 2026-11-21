// No imports, so `pnpm test` runs it in plain Node.

/** Two letters for a role's mark: `front` → FR, `dev-ops` → DO. */
export function initials(name: string): string {
  const words = name.split(/[-_\s]+/).filter(Boolean);
  const first = (word: string) => [...word][0];
  const letters = words.length > 1 ? first(words[0]) + first(words[1]) : [...(words[0] ?? "?")].slice(0, 2).join("");
  return letters.toUpperCase();
}

/** A frontmatter list, written as a YAML list or as a comma-separated string. */
function list(value: unknown): string[] | null {
  const items = Array.isArray(value) ? value.map(String) : typeof value === "string" ? value.split(",") : null;
  return items && items.map((s) => s.trim()).filter(Boolean);
}

/**
 * What a role may write: its `writes` globs; "readOnly" when it has none, when its `tools` leave out both Edit and
 * Write, or when its `disallowedTools` deny both; null when the role declares no limit.
 */
export function access(meta: Record<string, unknown>): string[] | "readOnly" | null {
  const writes = list(meta.writes);
  if (writes) return writes.length ? writes : "readOnly";
  const tools = list(meta.tools);
  if (tools && !tools.includes("Edit") && !tools.includes("Write")) return "readOnly";
  const denied = list(meta.disallowedTools) ?? [];
  return denied.includes("Edit") && denied.includes("Write") ? "readOnly" : null;
}

/** The project a save just created: the one missing before. */
export function added(before: { id: string }[], after: { id: string }[]): string | null {
  return after.find((p) => !before.some((b) => b.id === p.id))?.id ?? null;
}
