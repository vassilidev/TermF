import { Timers } from "$lib/ui/toast-logic";

/** What an action did, said in a line for a few seconds; with `undo`, the way back ("Undo", ⌘Z) for 10 s. */
export type Toast = { id: number; text: string; kind: "ok" | "error" | "info"; undo?: () => void };

export const toasts = $state<Toast[]>([]);

const timers = new Timers((id) => dismiss(id));
let next = 0;

/** The reversible actions, newest last: ⌘Z undoes the newest one still there once its toast is gone. */
const done: { id: number; undo: () => void }[] = [];
const KEPT = 20;

export function toast(text: string, options: { undo?: () => void; kind?: Toast["kind"]; ms?: number } = {}): number {
  const id = ++next;
  toasts.push({ id, text, kind: options.kind ?? "ok", undo: options.undo });
  if (options.undo) {
    done.push({ id, undo: options.undo });
    if (done.length > KEPT) done.shift();
  }
  timers.start(id, options.ms ?? (options.undo ? 10_000 : 5_000));
  // Three at most: the oldest goes.
  if (toasts.length > 3) dismiss(toasts[0].id);
  return id;
}

export function dismiss(id: number) {
  timers.remove(id);
  const at = toasts.findIndex((item) => item.id === id);
  if (at >= 0) toasts.splice(at, 1);
}

export function undo(id: number) {
  const item = toasts.find((entry) => entry.id === id);
  const at = done.findIndex((entry) => entry.id === id);
  if (at >= 0) done.splice(at, 1);
  dismiss(id);
  item?.undo?.();
}

/** ⌘Z with no toast to undo: the newest reversible action left; false when there is none. */
export function undoLast(): boolean {
  const last = done.pop();
  last?.undo();
  return !!last;
}

/** The pointer on a toast stops every timer; leaving it, or that toast going, starts them again. */
export const hold = (id: number) => timers.hold(id);
export const release = (id: number) => timers.release(id);
