// No imports, so `pnpm test` runs it in plain Node.

/**
 * The main area: `view` in the active panel, `other` in the second one (null: one panel). `active` is the active
 * panel's place, 0 on the left; `size` the left panel's share of the width.
 */
export type Panes<V extends string = string> = { view: V; other: V | null; active: 0 | 1; size: number };

export const MIN = 0.25;
export const MAX = 0.75;

export const clamp = (size: number) => (Number.isFinite(size) ? Math.min(MAX, Math.max(MIN, size)) : 0.5);

export function slots<V extends string>(p: Panes<V>): [V, V | null] {
  return p.other === null ? [p.view, null] : p.active === 0 ? [p.view, p.other] : [p.other, p.view];
}

/** Showing `next`: in the active panel, unless the other panel shows it already, which then becomes the active one. */
export function go<V extends string>(p: Panes<V>, next: V): Panes<V> {
  if (next === p.view) return p;
  if (next === p.other) return { ...p, view: next, other: p.view, active: p.active === 0 ? 1 : 0 };
  return { ...p, view: next };
}

/** `next` in the other panel, opened on the right if there was none, and the keyboard there. */
export function beside<V extends string>(p: Panes<V>, next: V): Panes<V> {
  return next === p.view ? p : { ...p, view: next, other: p.view, active: p.active === 0 ? 1 : 0 };
}

export function close<V extends string>(p: Panes<V>, view: V): Panes<V> {
  if (p.other === null || (view !== p.view && view !== p.other)) return p;
  return { ...p, view: view === p.view ? p.other : p.view, other: null, active: 0 };
}

type Shown<V> = { view: V; other: V | null };

/** Whether `page`, in one of the panels `before`, is in neither `after`: it goes off screen. */
export function leaves<V extends string>(page: V, before: Shown<V>, after: Shown<V>): boolean {
  return (before.view === page || before.other === page) && after.view !== page && after.other !== page;
}

export function encode<V extends string>(p: Panes<V>, known: readonly string[]): string | null {
  const [left, right] = slots(p);
  if (right === null || !known.includes(left) || !known.includes(right)) return null;
  return JSON.stringify({ left, right, size: Math.round(clamp(p.size) * 1000) / 1000 });
}

/** What `encode` kept, if it still names two different views of `known`. */
export function decode<V extends string>(text: string | null, known: readonly V[]): { left: V; right: V; size: number } | null {
  let value: unknown;
  try {
    value = JSON.parse(text ?? "");
  } catch {
    return null;
  }
  const { left, right, size } = (value ?? {}) as Record<string, unknown>;
  const valid = (v: unknown): v is V => typeof v === "string" && (known as readonly string[]).includes(v);
  if (!valid(left) || !valid(right) || left === right) return null;
  return { left, right, size: clamp(typeof size === "number" ? size : 0.5) };
}
