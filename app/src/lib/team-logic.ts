// No imports, so `pnpm test` runs it in plain Node.

/** Two letters for a role's mark: `front` → FR, `dev-ops` → DO. */
export function initials(name: string): string {
  const words = name.split(/[-_\s]+/).filter(Boolean);
  const letters = words.length > 1 ? words[0][0] + words[1][0] : [...(words[0] ?? "?")].slice(0, 2).join("");
  return letters.toUpperCase();
}

/**
 * What a role may write: its `writes` globs, "readOnly" when it has none or cannot edit files at all, null when the role
 * declares no limit.
 */
export function access(meta: Record<string, unknown>): string[] | "readOnly" | null {
  const writes = meta.writes;
  if (Array.isArray(writes)) return writes.length ? writes.map(String) : "readOnly";
  const denied = meta.disallowedTools;
  const tools = Array.isArray(denied) ? denied.map(String) : typeof denied === "string" ? denied.split(",") : [];
  const names = tools.map((s) => s.trim());
  return names.includes("Edit") && names.includes("Write") ? "readOnly" : null;
}

/** The project a save just created: the one missing before. */
export function added(before: { id: string }[], after: { id: string }[]): string | null {
  return after.find((p) => !before.some((b) => b.id === p.id))?.id ?? null;
}
