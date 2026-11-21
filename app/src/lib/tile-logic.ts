// No imports, so `pnpm test` runs it in plain Node.

/** "api · claude" opened with claude → api; a title that does not end with its tool stays whole. */
export function tileName(title: string, tool: string): string {
  const suffix = ` · ${tool}`;
  return title.endsWith(suffix) && title.length > suffix.length ? title.slice(0, -suffix.length) : title;
}

/** The tile that takes the keyboard once `id` closes: the next one shown, else the previous one. */
export function neighbour(ids: number[], id: number): number | null {
  const i = ids.indexOf(id);
  return i < 0 ? null : (ids[i + 1] ?? ids[i - 1] ?? null);
}
