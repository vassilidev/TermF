// No imports, so `pnpm test` runs it in plain Node.

/** "api · claude" opened with claude → api and claude; a title that does not end with its tool stays whole. */
export function tileTitle(title: string, tool: string): { name: string; tool: string } {
  const suffix = ` · ${tool}`;
  const named = title.endsWith(suffix) && title.length > suffix.length;
  return { name: named ? title.slice(0, -suffix.length) : title, tool };
}

/** The tile that takes the keyboard once `id` closes: the next one shown, else the previous one. */
export function neighbour(ids: number[], id: number): number | null {
  const i = ids.indexOf(id);
  return i < 0 ? null : (ids[i + 1] ?? ids[i - 1] ?? null);
}
