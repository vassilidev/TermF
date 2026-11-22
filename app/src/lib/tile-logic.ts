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

/** A `/loop` or scheduled task of a Claude Code session, as `session-loops` gives it. Times in epoch milliseconds. */
export type Loop = {
  id: string;
  kind: "cron" | "wakeup" | "monitor";
  cron: string | null;
  /** Seconds: the interval typed after `/loop`, or the last delay a self-paced loop chose. */
  every: number | null;
  recurring: boolean;
  prompt: string;
  fromLoop: boolean;
  created: number;
  nextRun: number | null;
  ends: number | null;
};

/** Seconds between two runs of a cron that repeats evenly (every 5 minutes, every 2 hours, daily), else null. */
export function period(cron: string): number | null {
  const f = cron.trim().split(/\s+/);
  if (f.length !== 5 || f.slice(2).some((x) => x !== "*")) return null;
  const [minute, hour] = f;
  // `*/7` runs at :00, :07 … :56, then :00 again: even only when the step divides its unit.
  const step = (x: string, unit: number) => {
    const n = x === "*" ? 1 : Number(/^\*\/(\d+)$/.exec(x)?.[1] ?? 0);
    return n && unit % n === 0 ? n : 0;
  };
  const fixed = (x: string) => /^\d+$/.test(x);
  if (hour === "*") return step(minute, 60) ? step(minute, 60) * 60 : fixed(minute) ? 3600 : null;
  if (!fixed(minute)) return null;
  return step(hour, 24) ? step(hour, 24) * 3600 : fixed(hour) ? 86400 : null;
}

/** Seconds a loop waits between runs: its cron's period, else the interval it was given or chose. */
export const loopEvery = (l: Loop): number | null =>
  (l.kind === "cron" && l.cron && l.recurring ? period(l.cron) : null) ?? l.every;

/** An interval in the largest unit that keeps it whole: 300 → 5 min, 7200 → 2 h, 172800 → 2 d. */
export function interval(seconds: number): { n: number; unit: "min" | "h" | "d" } {
  const min = Math.max(1, Math.round(seconds / 60));
  if (min % 60) return { n: min, unit: "min" };
  const h = min / 60;
  return h % 24 ? { n: h, unit: "h" } : { n: h / 24, unit: "d" };
}

/** Still on at `now`: a recurring task ends 7 days after its creation, a watch at its deadline. */
export const liveLoops = (loops: Loop[] | undefined, now: number) =>
  (loops ?? []).filter((l) => l.ends === null || l.ends > now);

/** When it runs next, rolled past `now` for a loop that repeats evenly (each task keeps its own offset). */
export function nextRun(l: Loop, now: number): number | null {
  if (l.nextRun === null || l.nextRun > now) return l.nextRun;
  const every = l.kind === "cron" && l.recurring && l.cron ? period(l.cron) : null;
  if (!every) return null;
  const ms = every * 1000;
  return l.nextRun + (Math.floor((now - l.nextRun) / ms) + 1) * ms;
}

/** The loop the tile's pill names (a loop before a watch), and how many more there are. */
export function pill(loops: Loop[]): { lead: Loop; more: number } | null {
  const lead = loops.find((l) => l.kind !== "monitor") ?? loops[0];
  return lead ? { lead, more: loops.length - 1 } : null;
}

const nearest = (n: number, steps: number[]) => steps.reduce((a, b) => (Math.abs(b - n) < Math.abs(a - n) ? b : a));

/** A cron that repeats about every `seconds`, rounded as `/loop` rounds an interval: to a step that divides its unit. */
export function intervalCron(seconds: number): string {
  const min = Math.max(1, Math.ceil(seconds / 60));
  if (min < 60) {
    const m = nearest(min, [1, 2, 3, 4, 5, 6, 10, 12, 15, 20, 30]);
    return m === 1 ? "* * * * *" : `*/${m} * * * *`;
  }
  const h = Math.round(min / 60);
  if (h < 24) {
    const n = nearest(h, [1, 2, 3, 4, 6, 8, 12]);
    return n === 1 ? "0 * * * *" : `0 */${n} * * *`;
  }
  const d = Math.round(h / 24);
  return d === 1 ? "0 0 * * *" : `0 0 */${d} * *`;
}

/** The schedule of a Scheduler task that does what the loop does; null for a one-shot reminder or a watch. */
export function toCron(l: Loop): string | null {
  if (l.kind === "monitor" || !l.recurring) return null;
  if (l.kind === "cron" && l.cron) return l.cron;
  return l.every ? intervalCron(l.every) : null;
}

/** A Scheduler task doing what the loop does, or null without a schedule or the whole prompt (none for `/loop`'s
 * built-in one, or one Claude Code clipped). */
export function taskOf(l: Loop): { prompt: string; cron: string } | null {
  const cron = toCron(l);
  return cron && l.prompt.trim() && !/… \[\+\d+ chars\]$/.test(l.prompt) ? { prompt: l.prompt, cron } : null;
}
