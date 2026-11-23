// No imports but types, so `pnpm test` runs it in plain Node.
import type { AskKind } from "$lib/team-run-logic";

/** Where a decision waits: a tile, a headless session, the CTO's own permission, a step of a team run, an Ops request or
 * an agent's request for a key. */
export type WaitKind = "tile" | "agent" | "cto" | "ops" | "secret" | AskKind;

/**
 * One thing waiting for the user. `key`: `tile:<id>`, `agent:<id>` (the CTO's too) or `ask:<run>:<step>`. `at` orders
 * them; a tile's wait has no date, and goes first: a terminal is held up right now.
 */
export type Wait =
  | { kind: "tile"; key: string; at: null; id: number }
  | { kind: "agent" | "cto"; key: string; at: number; id: number }
  | { kind: AskKind; key: string; at: number; run: number; step: string }
  | { kind: "ops" | "secret"; key: string; at: number; id: number; project: string };

export const askKey = (run: number, step: string) => `ask:${run}:${step}`;

/** The waits of a project, the most recent first. */
export function waits(
  tiles: { id: number }[],
  agents: { id: number; updated: number; cto: boolean }[],
  asks: { run: number; step: string; kind: AskKind; updated: number }[],
  ops: { id: number; at: number; project: string }[] = [],
  secrets: { id: number; at: number; project: string }[] = [],
): Wait[] {
  const dated: Wait[] = [
    ...agents.map((a) => ({ kind: a.cto ? ("cto" as const) : ("agent" as const), key: `agent:${a.id}`, at: a.updated, id: a.id })),
    ...asks.map((a) => ({ kind: a.kind, key: askKey(a.run, a.step), at: a.updated, run: a.run, step: a.step })),
    ...ops.map((o) => ({ kind: "ops" as const, key: `ops:${o.id}`, at: o.at, id: o.id, project: o.project })),
    ...secrets.map((s) => ({ kind: "secret" as const, key: `secret:${s.id}`, at: s.at, id: s.id, project: s.project })),
  ];
  dated.sort((a, b) => (b.at ?? 0) - (a.at ?? 0));
  return [...tiles.map((t) => ({ kind: "tile" as const, key: `tile:${t.id}`, at: null, id: t.id })), ...dated];
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
