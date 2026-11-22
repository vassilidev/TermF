// No imports but types, so `pnpm test` runs it in plain Node.
import type { AskKind } from "$lib/team-run-logic";

/** Where a decision waits: a tile, a headless session, the CTO's own permission, or a step of a team run. */
export type WaitKind = "tile" | "agent" | "cto" | AskKind;

/**
 * One thing waiting for the user. `key`: `tile:<id>`, `agent:<id>` (the CTO's too) or `ask:<run>:<step>`. `at` orders
 * them; a tile's wait has no date, and goes first: a terminal is held up right now.
 */
export type Wait =
  | { kind: "tile"; key: string; at: null; id: number }
  | { kind: "agent" | "cto"; key: string; at: number; id: number }
  | { kind: AskKind; key: string; at: number; run: number; step: string };

export const askKey = (run: number, step: string) => `ask:${run}:${step}`;

/** The waits of a project, the most recent first. */
export function waits(
  tiles: { id: number }[],
  agents: { id: number; updated: number; cto: boolean }[],
  asks: { run: number; step: string; kind: AskKind; updated: number }[],
): Wait[] {
  const dated: Wait[] = [
    ...agents.map((a) => ({ kind: a.cto ? ("cto" as const) : ("agent" as const), key: `agent:${a.id}`, at: a.updated, id: a.id })),
    ...asks.map((a) => ({ kind: a.kind, key: askKey(a.run, a.step), at: a.updated, run: a.run, step: a.step })),
  ];
  dated.sort((a, b) => (b.at ?? 0) - (a.at ?? 0));
  return [...tiles.map((t) => ({ kind: "tile" as const, key: `tile:${t.id}`, at: null, id: t.id })), ...dated];
}

type NoticeRef = { key: number; about: string; run?: number; step?: string };

/**
 * Where each step waiting for the user shows its card in the CTO's thread: in place of the latest notice that told of
 * it, else at the head of the thread (`rest`), when the step waited before the conversation could be told.
 */
export function place(notices: NoticeRef[], asks: { run: number; step: string }[]): { at: Map<number, string>; rest: string[] } {
  const open = new Set(asks.map((a) => askKey(a.run, a.step)));
  const latest = new Map<string, number>();
  for (const n of notices) {
    if ((n.about !== "waiting" && n.about !== "blocked") || n.run === undefined || !n.step) continue;
    const key = askKey(n.run, n.step);
    if (open.has(key) && n.key > (latest.get(key) ?? -1)) latest.set(key, n.key);
  }
  return {
    at: new Map([...latest].map(([ask, notice]) => [notice, ask])),
    rest: asks.map((a) => askKey(a.run, a.step)).filter((k) => !latest.has(k)),
  };
}

/**
 * The card whose action is the screen's primary one: the most recent open decision told in the thread, else the last
 * of those at its head. `done`: cards still shown that no longer ask anything (a delivery made). None while the CTO
 * itself waits for an answer, its permission being the last thing it said.
 */
export function primary(placed: { at: Map<number, string>; rest: string[] }, ctoAsks: boolean, done: string[] = []): string | null {
  if (ctoAsks) return null;
  const told = [...placed.at].filter(([, key]) => !done.includes(key)).sort((a, b) => a[0] - b[0]);
  return told.at(-1)?.[1] ?? placed.rest.filter((key) => !done.includes(key)).at(-1) ?? null;
}

type RunIn = { id: number; project: string; status: string; updated: number; parent?: number | null };
type TeamIn = { run: number; target: { project: string }; state: string; updated: number };

/**
 * A project's open runs, null for every project's: a ticket run until it is closed or cleaned up, with its team when it
 * has one, and a team at work whose ticket run is not known yet. The most recently changed first.
 */
export function openRuns<R extends RunIn, T extends TeamIn>(runs: R[], teams: T[], project: string | null): { run?: R; team?: T; at: number }[] {
  const mine = (p: string) => project === null || p === project;
  const out: { run?: R; team?: T; at: number }[] = runs
    // The run of another repository of a change shows on its change's page.
    .filter((r) => !r.parent && mine(r.project) && (r.status === "running" || r.status === "delivered"))
    .map((run) => {
      const team = teams.find((t) => t.run === run.id);
      return { run, team, at: Math.max(run.updated, team?.updated ?? 0) };
    });
  for (const team of teams) {
    const working = team.state === "running" || team.state === "waiting";
    if (working && mine(team.target.project) && !runs.some((r) => r.id === team.run)) out.push({ team, at: team.updated });
  }
  return out.sort((a, b) => b.at - a.at);
}
