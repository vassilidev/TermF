/** What an action did, said in a line for a few seconds; with `undo`, the way back ("Undo", ⌘Z) for 10 s. */
export type Toast = { id: number; text: string; kind: "ok" | "error" | "info"; undo?: () => void };

export const toasts = $state<Toast[]>([]);

const timers = new Map<number, { timer: ReturnType<typeof setTimeout>; left: number; since: number }>();
let next = 0;

function arm(id: number, left: number) {
  timers.set(id, { timer: setTimeout(() => dismiss(id), left), left, since: Date.now() });
}

export function toast(text: string, options: { undo?: () => void; kind?: Toast["kind"]; ms?: number } = {}): number {
  const id = ++next;
  toasts.push({ id, text, kind: options.kind ?? "ok", undo: options.undo });
  arm(id, options.ms ?? (options.undo ? 10_000 : 5_000));
  // Three at most: the oldest goes.
  if (toasts.length > 3) dismiss(toasts[0].id);
  return id;
}

export function dismiss(id: number) {
  clearTimeout(timers.get(id)?.timer);
  timers.delete(id);
  const at = toasts.findIndex((item) => item.id === id);
  if (at >= 0) toasts.splice(at, 1);
}

export function undo(id: number) {
  const item = toasts.find((entry) => entry.id === id);
  dismiss(id);
  item?.undo?.();
}

/** Read under the pointer: the time stops, and starts again where it was. */
let held = false;
export function hold(on: boolean) {
  if (on === held) return;
  held = on;
  for (const [id, entry] of timers) {
    clearTimeout(entry.timer);
    if (on) timers.set(id, { ...entry, left: Math.max(0, entry.left - (Date.now() - entry.since)) });
    else arm(id, Math.max(1_000, entry.left));
  }
}
