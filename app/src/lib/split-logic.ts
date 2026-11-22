// No imports, so `pnpm test` runs it in plain Node.

/** `row`: side by side (split right), `col`: stacked (split down). */
export type Dir = "row" | "col";
export type Side = "left" | "right" | "up" | "down";
/** A session id, or a split whose `sizes` (fractions summing to 1) follow its children. */
export type Pane = number | Split;
export type Split = { dir: Dir; sizes: number[]; children: Pane[] };
export type Rect = { x: number; y: number; w: number; h: number };
/** The border between `children[i]` and `children[i + 1]` of the split at `path`, inside `box`. */
export type Divider = { path: number[]; i: number; dir: Dir; at: number; box: Rect };

const MIN = 0.1;
const FULL: Rect = { x: 0, y: 0, w: 1, h: 1 };
const dirOf = (side: Side): Dir => (side === "left" || side === "right" ? "row" : "col");
const forward = (side: Side) => side === "right" || side === "down";
const even = (n: number) => Array<number>(n).fill(1 / n);

export function leaves(p: Pane): number[] {
  return typeof p === "number" ? [p] : p.children.flatMap(leaves);
}

/** Merges a child split into its parent when both go the same way, so each split alternates direction. */
function flat(dir: Dir, sizes: number[], children: Pane[]): Pane {
  if (children.length === 1) return children[0];
  const s: number[] = [];
  const c: Pane[] = [];
  children.forEach((child, i) => {
    if (typeof child !== "number" && child.dir === dir) {
      child.sizes.forEach((size) => s.push(size * sizes[i]));
      c.push(...child.children);
    } else {
      s.push(sizes[i]);
      c.push(child);
    }
  });
  const total = s.reduce((a, b) => a + b, 0);
  return { dir, sizes: s.map((size) => size / total), children: c };
}

/** `p` with `id` next to `at`, on `side`; the new pane takes half of `at`'s room. */
export function insert(p: Pane, at: number, id: number, side: Side): Pane {
  const dir = dirOf(side);
  if (typeof p === "number") {
    if (p !== at) return p;
    return { dir, sizes: [0.5, 0.5], children: forward(side) ? [at, id] : [id, at] };
  }
  const i = p.children.indexOf(at);
  if (i >= 0 && p.dir === dir) {
    const sizes = [...p.sizes];
    sizes.splice(i, 1, sizes[i] / 2, sizes[i] / 2);
    const children = [...p.children];
    children.splice(forward(side) ? i + 1 : i, 0, id);
    return { dir, sizes, children };
  }
  return { ...p, children: p.children.map((c) => insert(c, at, id, side)) };
}

/** `p` without `id`; its room goes to its siblings. Null once nothing is left. */
export function remove(p: Pane, id: number): Pane | null {
  return relabel(p, (x) => (x === id ? null : x));
}

/** Where each session sits, as fractions of `box`. */
export function rects(p: Pane, box: Rect = FULL, out = new Map<number, Rect>()): Map<number, Rect> {
  if (typeof p === "number") return out.set(p, box);
  let at = 0;
  p.children.forEach((child, i) => {
    const size = p.sizes[i];
    rects(child, p.dir === "row" ? { ...box, x: box.x + at * box.w, w: size * box.w } : { ...box, y: box.y + at * box.h, h: size * box.h }, out);
    at += size;
  });
  return out;
}

export function dividers(p: Pane, box: Rect = FULL, path: number[] = [], out: Divider[] = []): Divider[] {
  if (typeof p === "number") return out;
  let at = 0;
  p.children.forEach((child, i) => {
    const size = p.sizes[i];
    const inner = p.dir === "row" ? { ...box, x: box.x + at * box.w, w: size * box.w } : { ...box, y: box.y + at * box.h, h: size * box.h };
    at += size;
    if (i < p.children.length - 1) out.push({ path, i, dir: p.dir, at: p.dir === "row" ? box.x + at * box.w : box.y + at * box.h, box });
    dividers(child, inner, [...path, i], out);
  });
  return out;
}

/** The session next to `id` on `side`: the one sharing the most of that border, null at the edge. */
export function neighbour(p: Pane, id: number, side: Side): number | null {
  const all = rects(p);
  const r = all.get(id);
  if (!r) return null;
  const near = (a: number, b: number) => Math.abs(a - b) < 1e-6;
  const overlap = (a0: number, a1: number, b0: number, b1: number) => Math.min(a1, b1) - Math.max(a0, b0);
  let best: number | null = null;
  let most = 1e-6;
  for (const [other, o] of all) {
    const touches = {
      left: near(o.x + o.w, r.x),
      right: near(o.x, r.x + r.w),
      up: near(o.y + o.h, r.y),
      down: near(o.y, r.y + r.h),
    }[side];
    const shared = dirOf(side) === "row" ? overlap(r.y, r.y + r.h, o.y, o.y + o.h) : overlap(r.x, r.x + r.w, o.x, o.x + o.w);
    if (other !== id && touches && shared > most) {
      best = other;
      most = shared;
    }
  }
  return best;
}

/** Moves the border between `sizes[i]` and `sizes[i + 1]` by `delta`, keeping both at least `MIN`. */
function nudge(sizes: number[], i: number, delta: number): number[] {
  const d = Math.max(MIN - sizes[i], Math.min(sizes[i + 1] - MIN, delta));
  const next = [...sizes];
  next[i] += d;
  next[i + 1] -= d;
  return next;
}

function update(p: Pane, path: number[], change: (s: Split) => Split): Pane {
  if (typeof p === "number") return p;
  if (!path.length) return change(p);
  const [head, ...rest] = path;
  return { ...p, children: p.children.map((c, i) => (i === head ? update(c, rest, change) : c)) };
}

/** Drags a divider: `delta` is a fraction of its split. */
export function resizeAt(p: Pane, path: number[], i: number, delta: number): Pane {
  return update(p, path, (s) => ({ ...s, sizes: nudge(s.sizes, i, delta) }));
}

/**
 * Moves a border of `id`'s pane towards `side` by `step` (a fraction of the whole tab), like tmux: the border on that
 * side when there is one (the pane grows), else the opposite one (it shrinks).
 */
export function resize(p: Pane, id: number, side: Side, step: number): Pane {
  const dir = dirOf(side);
  const sign = forward(side) ? 1 : -1;
  // The splits of that direction on the way to `id`, deepest first, with the child it is in and their extent.
  const chain: { path: number[]; i: number; n: number; extent: number }[] = [];
  let node = p;
  let box = FULL;
  const path: number[] = [];
  while (typeof node !== "number") {
    const split: Split = node;
    const i = split.children.findIndex((c) => leaves(c).includes(id));
    if (i < 0) return p;
    if (split.dir === dir) chain.unshift({ path: [...path], i, n: split.children.length, extent: dir === "row" ? box.w : box.h });
    const before = split.sizes.slice(0, i).reduce((a, b) => a + b, 0);
    box = split.dir === "row"
      ? { ...box, x: box.x + before * box.w, w: split.sizes[i] * box.w }
      : { ...box, y: box.y + before * box.h, h: split.sizes[i] * box.h };
    path.push(i);
    node = split.children[i];
  }
  const grow = chain.find((c) => (sign > 0 ? c.i < c.n - 1 : c.i > 0));
  const shrink = chain.find((c) => (sign > 0 ? c.i > 0 : c.i < c.n - 1));
  const at = grow ?? shrink;
  if (!at) return p;
  const border = grow ? (sign > 0 ? at.i : at.i - 1) : sign > 0 ? at.i - 1 : at.i;
  return resizeAt(p, at.path, border, (sign * step) / at.extent);
}

export function equalize(p: Pane): Pane {
  return typeof p === "number" ? p : { ...p, sizes: even(p.children.length), children: p.children.map(equalize) };
}

/** Rows of `cols` sessions, the last one stretched. */
export function grid(ids: number[], cols = Math.ceil(Math.sqrt(ids.length))): Pane | null {
  const rows: Pane[] = [];
  for (let i = 0; i < ids.length; i += cols) {
    const row = ids.slice(i, i + cols);
    rows.push(row.length === 1 ? row[0] : { dir: "row", sizes: even(row.length), children: row });
  }
  if (!rows.length) return null;
  return rows.length === 1 ? rows[0] : { dir: "col", sizes: even(rows.length), children: rows };
}

/** `main` large on the left, the others stacked on the right. */
export function focus(ids: number[], main: number): Pane | null {
  const others = ids.filter((id) => id !== main);
  if (!ids.includes(main)) return grid(ids);
  if (!others.length) return main;
  const side: Pane = others.length === 1 ? others[0] : { dir: "col", sizes: even(others.length), children: others };
  return { dir: "row", sizes: [0.7, 0.3], children: [main, side] };
}

/** For a saved layout: sessions as their index in `ids`. */
export function encode(p: Pane, ids: number[]): string {
  const walk = (q: Pane): unknown => (typeof q === "number" ? ids.indexOf(q) : { dir: q.dir, sizes: q.sizes, children: q.children.map(walk) });
  return JSON.stringify(walk(p));
}

/** A saved tree over `count` sessions, null when it is not one (each index once, splits of at least two). */
export function decode(text: string, count: number): Pane | null {
  const seen = new Set<number>();
  const walk = (q: unknown): Pane | null => {
    if (typeof q === "number") {
      if (!Number.isInteger(q) || q < 0 || q >= count || seen.has(q)) return null;
      seen.add(q);
      return q;
    }
    const s = q as Partial<Split> | null;
    if (!s || (s.dir !== "row" && s.dir !== "col") || !Array.isArray(s.children) || !Array.isArray(s.sizes)) return null;
    if (s.children.length < 2 || s.sizes.length !== s.children.length) return null;
    if (!s.sizes.every((n) => typeof n === "number" && Number.isFinite(n) && n > 0)) return null;
    const children = s.children.map(walk);
    if (children.some((c) => c === null)) return null;
    const total = s.sizes.reduce((a, b) => a + b, 0);
    return { dir: s.dir, sizes: s.sizes.map((n) => n / total), children: children as Pane[] };
  };
  try {
    return walk(JSON.parse(text));
  } catch {
    return null;
  }
}

/** Replaces each session id through `map`; those mapped to null leave the tree. */
export function relabel(p: Pane, map: (id: number) => number | null): Pane | null {
  if (typeof p === "number") return map(p);
  const sizes: number[] = [];
  const children: Pane[] = [];
  p.children.forEach((child, i) => {
    const kept = relabel(child, map);
    if (kept !== null) {
      sizes.push(p.sizes[i]);
      children.push(kept);
    }
  });
  return children.length ? flat(p.dir, sizes, children) : null;
}

/** The split to go to in a tab: `active` when `shown`, else the first shown one (else `active` all the same). */
export function entry(p: Pane, active: number, shown: (id: number) => boolean): number {
  return shown(active) ? active : (leaves(p).find(shown) ?? active);
}

/** `list` with the item at `from` moved to `to`. */
export function move<T>(list: T[], from: number, to: number): T[] {
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}
