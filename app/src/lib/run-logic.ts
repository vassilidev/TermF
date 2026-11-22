// No imports but types, so `pnpm test` runs it in plain Node.
import type { t } from "$lib/i18n";

type Key = Parameters<typeof t>[0];
type Ticket = { status: string; prUrl: string | null; prState: string | null };
type Step = { key: string; kind: string; state: string };
type Team = { state: string; steps: Step[] };

export type StageState = "done" | "working" | "waiting" | "failed" | "pending";
export type Stage = { id: string; state: StageState };

/** The shipped workflows' steps the timeline folds into a neighbour: one stage each for framing, the plan and the review. */
const folded: Record<string, string> = {
  prd: "frame",
  architecture: "frame",
  ux: "frame",
  "approve-plan": "plan",
  checks: "review",
  rules: "review",
  security: "review",
  seo: "review",
  visual: "review",
};

const working = (team: Team) => team.state === "running" || team.state === "waiting";
const asking = (team: Team) => working(team) && team.steps.some((s) => s.state === "waiting" || s.state === "blocked");

/** The stage a step belongs to: a shipped one, else the step's own id. */
export function stageOf(step: Pick<Step, "key" | "kind">): string {
  if (step.kind === "foreach" || step.kind === "task") return "tasks";
  if (step.kind === "review") return "review";
  if (step.kind === "connector") return "deliver";
  return folded[step.key] ?? step.key;
}

/** The team is over, the ticket is not delivered yet: delivering is the user's. */
export function awaitsDelivery(ticket: Ticket, team: Team | null | undefined): boolean {
  return !!team && team.state === "done" && ticket.status === "running";
}

function stageState(steps: Step[], team: Team): StageState {
  const on = working(team);
  if (on && steps.some((s) => s.state === "waiting" || s.state === "blocked")) return "waiting";
  if (steps.some((s) => s.state === "running")) return "working";
  if (steps.every((s) => s.state === "done" || s.state === "skipped")) return "done";
  if (steps.some((s) => s.state === "blocked")) return "failed";
  return on && steps.some((s) => s.state === "done") ? "working" : "pending";
}

/**
 * The run's timeline: the routed workflow's stages in order, a stage every step of which was skipped left out, and the
 * delivery last, done once a pull request is open; a run without a team goes launched → pull request → delivered (merged).
 */
export function stages(ticket: Ticket, team?: Team | null): Stage[] {
  if (!team) {
    const pr = !!ticket.prUrl;
    return [
      { id: "launched", state: ticket.status === "failed" ? "failed" : "done" },
      { id: "pr", state: pr ? "done" : ticket.status === "running" ? "working" : "pending" },
      { id: "delivered", state: ticket.prState === "merged" ? "done" : pr && ticket.status === "delivered" && !ticket.prState ? "working" : "pending" },
    ];
  }
  const groups = new Map<string, Step[]>();
  for (const step of team.steps) groups.set(stageOf(step), [...(groups.get(stageOf(step)) ?? []), step]);
  groups.delete("deliver");
  const deliver = team.steps.filter((s) => stageOf(s) === "deliver");
  const out: Stage[] = [...groups]
    .filter(([, steps]) => steps.some((s) => s.state !== "skipped"))
    .map(([id, steps]) => ({ id, state: stageState(steps, team) }));
  const delivered = !!ticket.prUrl;
  const last: StageState = delivered
    ? "done"
    : awaitsDelivery(ticket, team)
      ? "waiting"
      : deliver.length
        ? stageState(deliver, team)
        : "pending";
  return [...out, { id: "deliver", state: last }];
}

/** The shipped stages' names; any other is shown as its id. */
export const stageNames: Record<string, Key> = {
  triage: "ux3t2StageTriage",
  "approve-route": "ux3t2StageRoute",
  frame: "ux3t2StageFrame",
  plan: "ux3t2StagePlan",
  tasks: "ux3t2StageTasks",
  review: "ux3t2StageReview",
  deliver: "ux3t2StageDeliver",
};

/** A run without a team: launched, its pull request, delivered once merged. */
export const soloNames: Record<string, Key> = { launched: "runMetaCreated", pr: "runMetaPr", delivered: "runStatusDelivered" };

export type Shown = { status: string; label: Key | null };

/**
 * One state for a ticket and its team, the same wherever the run shows: closed, failed or delivered as the ticket
 * says; else the team's (waiting for the user, working, paused, stopped, out of budget, or done and waiting for the
 * delivery); else the ticket's.
 */
export function shown(ticket: Ticket, team?: Team | null): Shown {
  if (ticket.status === "closed" || ticket.status === "failed") return { status: ticket.status, label: null };
  if (ticket.status === "delivered") return { status: "delivered", label: ticket.prState ? "ticketsUiReady" : null };
  if (!team) return { status: ticket.status, label: null };
  if (asking(team) || awaitsDelivery(ticket, team)) return { status: "waiting", label: null };
  const states: Record<string, Shown> = {
    running: { status: "working", label: null },
    waiting: { status: "working", label: null },
    paused: { status: "idle", label: "teamRunsPaused" },
    stopped: { status: "stopped", label: null },
    budget: { status: "budget", label: null },
    done: { status: "done", label: null },
  };
  return states[team.state] ?? { status: ticket.status, label: null };
}

/** What the run expects of the user: answers to the team, the delivery, or the cleanup once its pull request is merged or closed. */
export function expected(ticket: Ticket, team?: Team | null): "answer" | "deliver" | "clean" | null {
  if (ticket.status === "delivered" && ticket.prState) return "clean";
  if (ticket.status !== "running" && ticket.status !== "delivered") return null;
  if (team && asking(team)) return "answer";
  return awaitsDelivery(ticket, team) ? "deliver" : null;
}

/** A GitHub pull request as `owner/repo#n`; any other address as it is. */
export function prName(url: string): string {
  const m = /^https:\/\/github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)/.exec(url);
  return m ? `${m[1]}#${m[2]}` : url;
}
