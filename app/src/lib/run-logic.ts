// No imports but types, so `pnpm test` runs it in plain Node.
import type { t } from "$lib/i18n";
import type { Ask } from "$lib/team-run-logic";

type Key = Parameters<typeof t>[0];
type Ticket = { status: string; prUrl: string | null; prState: string | null };
type Step = { key: string; kind: string; state: string; escalates?: string | null };
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
  if (step.kind === "connector" || step.kind === "update") return "deliver";
  return folded[step.key] ?? step.key;
}

/** The team is over, the ticket is not delivered yet: delivering is the user's. */
export function awaitsDelivery(ticket: Ticket, team: Team | null | undefined): boolean {
  return !!team && team.state === "done" && ticket.status === "running";
}

/** A team no longer at work leaves no stage working: where it stopped or ran out of budget failed, a pause only waits. */
function stageState(steps: Step[], team: Team): StageState {
  const on = working(team);
  if (steps.every((s) => s.state === "done" || s.state === "skipped")) return "done";
  if (on && steps.some((s) => s.state === "waiting" || s.state === "blocked")) return "waiting";
  if (on) return steps.some((s) => s.state === "running" || s.state === "done") ? "working" : "pending";
  if (steps.some((s) => s.state === "blocked")) return "failed";
  const halted = team.state === "stopped" || team.state === "budget";
  return halted && steps.some((s) => s.state === "running") ? "failed" : "pending";
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
  for (const step of team.steps) {
    // The CTO's escalation belongs to the stage of the step it decides on.
    const id = stageOf(team.steps.find((s) => s.key === step.escalates) ?? step);
    groups.set(id, [...(groups.get(id) ?? []), step]);
  }
  groups.delete("deliver");
  const deliver = team.steps.filter((s) => stageOf(team.steps.find((t) => t.key === s.escalates) ?? s) === "deliver");
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

/** The shipped workflows' steps by name; a stage's own name where the step is the stage. */
export const stepNames: Record<string, Key> = {
  triage: "ux3t2StageTriage",
  "approve-route": "ux3t2StageRoute",
  prd: "ux3t2StepPrd",
  frame: "ux3t2StageFrame",
  architecture: "ux3t2StepArchitecture",
  ux: "ux3t2StepUx",
  plan: "ux3t2StagePlan",
  "approve-plan": "ux3t2StepApprovePlan",
  implement: "ux3t2StageTasks",
  checks: "ux3t2StepChecks",
  rules: "ux3t2StepRules",
  review: "ux3t2StepCodeReview",
  security: "ux3t2StepSecurity",
  seo: "ux3t2StepSeo",
  visual: "ux3t2StepVisual",
  migrations: "roles2StepMigrations",
  "update-base": "integratorStep",
  deliver: "ux3t2StageDeliver",
};

/** A name of `names` by its own key only: a step called `constructor` is not Object's. */
export const named = (names: Record<string, Key>, id: string): Key | null => (Object.hasOwn(names, id) ? names[id] : null);

/**
 * A step as the user reads it: a task by its id (`<foreach>:<task>` when only the key is known), a shipped step by
 * its name, any other by its id.
 */
export function stepLabel(key: string, task: string | null = null): { name: Key } | { text: string } {
  if (key.startsWith("escalate-")) return { name: "disciplineEscalation" };
  const id = task ?? (key.includes(":") ? key.slice(key.indexOf(":") + 1) : null);
  if (id) return { text: id };
  const name = named(stepNames, key);
  return name ? { name } : { text: key };
}

/**
 * What a wait is called, on its card and in a list of waits: the title's key, and whether it names the step (as
 * `{step}`).
 */
export function askTitle(ask: Pick<Ask, "kind"> & { step: Pick<Ask["step"], "key" | "kind" | "plan"> }): { key: Key; step: boolean } {
  const fixed: Partial<Record<Ask["kind"], Key>> = {
    route: "ctoEntryAskRoute",
    plan: "teamRunsAskPlan",
    visual: "teamRunsAskVisual",
    deliver: "teamRunsAskDeliver",
  };
  const own = fixed[ask.kind];
  if (own) return { key: own, step: false };
  if (ask.kind === "blocked") return { key: "teamRunsAskBlocked", step: true };
  if (ask.kind === "gate") return { key: "teamRunsAskGate", step: true };
  if (ask.step.key === "triage") return { key: "ctoEntryAskQuestions", step: false };
  if (ask.step.plan) return { key: "teamRunsAskPlanQuestions", step: false };
  // Another review than the code review, such as the migrations', is named by its step.
  if (ask.step.kind === "review") return ask.step.key === "review" ? { key: "reviewLoopAsk", step: false } : { key: "roles2ReviewAsk", step: true };
  return { key: "teamRunsAskQuestions", step: true };
}

export type Shown = { status: string; label: Key | null };

/**
 * One state for a ticket and its team, the same wherever the run shows: closed, failed or delivered as the ticket
 * says; else the team's (waiting for the user, working, paused, stopped, out of budget, or done and waiting for the
 * delivery); else the ticket's. Without its ticket run (not read yet), the team's alone.
 */
export function shown(ticket: Ticket | null | undefined, team?: Team | null): Shown {
  if (ticket && (ticket.status === "closed" || ticket.status === "failed")) return { status: ticket.status, label: null };
  if (ticket?.status === "delivered") return { status: "delivered", label: ticket.prState ? "ticketsUiReady" : null };
  if (!team) return { status: ticket?.status ?? "running", label: null };
  if (asking(team) || (ticket && awaitsDelivery(ticket, team))) return { status: "waiting", label: null };
  const states: Record<string, Shown> = {
    running: { status: "working", label: null },
    waiting: { status: "working", label: null },
    paused: { status: "idle", label: "teamRunsPaused" },
    stopped: { status: "stopped", label: null },
    budget: { status: "budget", label: null },
    done: { status: "done", label: null },
  };
  return states[team.state] ?? { status: ticket?.status ?? "running", label: null };
}

/** What the run expects of the user: answers to the team, the delivery, or the cleanup once its pull request is merged or closed. */
export function expected(ticket: Ticket, team?: Team | null): "answer" | "deliver" | "clean" | null {
  if (ticket.status === "delivered" && ticket.prState) return "clean";
  if (ticket.status !== "running" && ticket.status !== "delivered") return null;
  if (team && asking(team)) return "answer";
  return awaitsDelivery(ticket, team) ? "deliver" : null;
}

/**
 * What the Run page says of a delivered run's open pull request against its base: in conflict, or behind it, and
 * whether the team can take the update up (it is done and its workflow has the step). A team still at work, paused or
 * stopped shows its own state instead.
 */
export function baseUpdate(
  ticket: Ticket,
  merge: { conflict: boolean; behind: boolean } | null,
  team?: Team | null,
): { state: "conflict" | "behind"; action: boolean } | null {
  if (ticket.status !== "delivered" || !ticket.prUrl || ticket.prState || !merge) return null;
  const state = merge.conflict ? "conflict" : merge.behind ? "behind" : null;
  if (!state || (team && team.state !== "done")) return null;
  return { state, action: !!team?.steps.some((s) => s.kind === "update") };
}

/** A GitHub pull request as `owner/repo#n`; any other address as it is. */
export function prName(url: string): string {
  const m = /^https:\/\/github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)/.exec(url);
  return m ? `${m[1]}#${m[2]}` : url;
}
