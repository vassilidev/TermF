// No imports, so `pnpm test` runs it in plain Node.

/** Several items picked in a list: `keys` in the list's order, `anchor` where a Shift-click range starts. */
export type Picked<K> = { keys: K[]; anchor: K | null };

export const none = <K>(): Picked<K> => ({ keys: [], anchor: null });

/** Those of `keys` still shown, in the order shown: a filter or a project change leaves the others out. */
export const within = <K>(keys: K[], shown: K[]): K[] => shown.filter((k) => keys.includes(k));

/**
 * A click on `key` among `shown`: with `range` (Shift), everything from the anchor to it is added; otherwise it is
 * toggled and becomes the anchor.
 */
export function pick<K>(p: Picked<K>, shown: K[], key: K, range: boolean): Picked<K> {
  const keys = within(p.keys, shown);
  const from = p.anchor === null ? -1 : shown.indexOf(p.anchor);
  const to = shown.indexOf(key);
  if (to < 0) return { keys, anchor: p.anchor };
  if (range && from >= 0) {
    const span = shown.slice(Math.min(from, to), Math.max(from, to) + 1);
    return { keys: within([...keys, ...span], shown), anchor: p.anchor };
  }
  return { keys: keys.includes(key) ? keys.filter((k) => k !== key) : within([...keys, key], shown), anchor: key };
}

/** Select all: every item shown, or none when they all already are. */
export function all<K>(p: Picked<K>, shown: K[]): Picked<K> {
  const every = shown.length > 0 && shown.every((k) => p.keys.includes(k));
  return { keys: every ? [] : [...shown], anchor: p.anchor };
}
