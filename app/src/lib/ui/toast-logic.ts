/**
 * The toasts' timers: each one counts down to `expire(id)`; while one toast is under the pointer every timer stops,
 * and none starts, so nothing goes from under the reader. Leaving it, or that toast going away, starts them again.
 */
export class Timers {
  #entries = new Map<number, { timer?: ReturnType<typeof setTimeout>; left: number; since: number }>();
  #held: number | null = null;
  #expire: (id: number) => void;

  constructor(expire: (id: number) => void) {
    this.#expire = expire;
  }

  start(id: number, ms: number) {
    this.#entries.set(id, { left: ms, since: 0 });
    if (this.#held === null) this.#run(id);
  }

  remove(id: number) {
    clearTimeout(this.#entries.get(id)?.timer);
    this.#entries.delete(id);
    if (this.#held === id) this.release(id);
  }

  /** The pointer is on toast `id`. */
  hold(id: number) {
    const was = this.#held;
    this.#held = id;
    if (was === null) for (const key of this.#entries.keys()) this.#stop(key);
  }

  /** The pointer left toast `id` (another one may have taken it already). */
  release(id: number) {
    if (this.#held !== id) return;
    this.#held = null;
    // A second at least, to read what was under the pointer.
    for (const [key, entry] of this.#entries) {
      entry.left = Math.max(1_000, entry.left);
      this.#run(key);
    }
  }

  #run(id: number) {
    const entry = this.#entries.get(id)!;
    entry.since = Date.now();
    entry.timer = setTimeout(() => this.#expire(id), entry.left);
  }

  #stop(id: number) {
    const entry = this.#entries.get(id)!;
    clearTimeout(entry.timer);
    entry.timer = undefined;
    entry.left = Math.max(0, entry.left - (Date.now() - entry.since));
  }
}
