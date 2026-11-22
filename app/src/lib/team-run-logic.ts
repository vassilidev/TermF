// No imports, so `pnpm test` runs it in plain Node.

export type RunState = "running" | "waiting" | "paused" | "stopped" | "budget" | "done";
export type StepState = "pending" | "running" | "waiting" | "blocked" | "done" | "skipped";
export type StepKind = "agent" | "gate" | "foreach" | "shell" | "rules" | "visual" | "connector" | "task";

/** A task of the orchestrator's plan (ARCHITECTURE appendix A). */
export type Task = {
  id: string;
  role: string;
  repo: string;
  size: "S" | "M" | "L";
  covers: string[];
  goal: string;
  files_hint: string[];
  depends_on: string[];
  ui_impact: boolean;
  /** Empty: the only squad, or the staff; absent from a plan made before squads. */
  squad?: string;
};
export type Plan = {
  summary: string;
  acceptance: { id: string; text: string }[];
  out_of_scope: string[];
  questions: string[];
  tasks: Task[];
};
/** What a step's agent reports (appendix B). */
export type Report = {
  status: "done" | "blocked" | "failed";
  summary: string;
  covers: string[];
  text_changes: { file: string; text: string; justified_by: string }[];
  affected_views: { url: string; state: string; viewports: string[] }[];
  out_of_scope_findings: string[];
  questions?: string[];
};
export type Violation = { rule: string; file: string | null; message: string; blocking: boolean };
export type Step = {
  /** The workflow's step name; `<foreach>:<task>` for a task of the plan. */
  key: string;
  kind: StepKind;
  role: string | null;
  task: string | null;
  needs: string[];
  state: StepState;
  /** Its answer is the plan a `foreach` runs. */
  plan: boolean;
  session: number | null;
  sessions: number[];
  attempt: number;
  feedback: string | null;
  files: string[];
  report: Report | null;
  violations: Violation[];
  cost: number;
  /** Why it waits, is blocked or skipped, or a shell step's output. */
  note: string | null;
  started: number | null;
  finished: number | null;
};
export type TeamRun = {
  /** The ticket run's id. */
  run: number;
  target: { run: number; project: string; repo: string; worktree: string; ticket: string; title: string };
  workflow: string;
  state: RunState;
  budget: number | null;
  cost: number;
  plan: Plan | null;
  steps: Step[];
  /** Why it waits, is paused, stopped or out of budget. */
  note: string | null;
  created: number;
  updated: number;
};

/**
 * The plan's tasks in levels: a task comes after every task it depends on, so those of one level may run together.
 * A dependency the plan does not have is ignored; tasks caught in a cycle come last, together.
 */
export function levels(tasks: Task[]): Task[][] {
  const ids = new Set(tasks.map((task) => task.id));
  const placed = new Set<string>();
  const out: Task[][] = [];
  let left = tasks;
  while (left.length) {
    const ready = left.filter((task) => task.depends_on.every((d) => !ids.has(d) || placed.has(d)));
    if (!ready.length) {
      out.push(left);
      break;
    }
    out.push(ready);
    for (const task of ready) placed.add(task.id);
    left = left.filter((task) => !placed.has(task.id));
  }
  return out;
}

/**
 * What a step waiting for the user asks: approve the plan, answer questions, review the interface, let the delivery
 * through, pass another gate, or decide on a step that failed.
 */
export type AskKind = "plan" | "questions" | "visual" | "deliver" | "gate" | "blocked";
/** `accept`: approving it as it is can work (never a plan step: it has no plan, or questions). */
export type Ask = { kind: AskKind; step: Step; questions: string[]; accept: boolean };

const working = (run: TeamRun) => run.state === "running" || run.state === "waiting";

/** What waits for the user in a team at work; a paused, stopped or finished one only waits to be started again. */
export function asks(run: TeamRun): Ask[] {
  if (!working(run)) return [];
  const plans = new Set(run.steps.filter((s) => s.plan).map((s) => s.key));
  return run.steps
    .filter((step) => step.state === "waiting" || step.state === "blocked")
    .map((step) => {
      if (step.state === "blocked") {
        const questions = (step.plan ? run.plan?.questions : step.report?.questions) ?? [];
        return { kind: questions.length ? "questions" : "blocked", step, questions, accept: !step.plan };
      }
      const kind: AskKind =
        step.kind === "visual"
          ? "visual"
          : step.kind === "connector"
            ? "deliver"
            : step.needs.some((n) => plans.has(n))
              ? "plan"
              : "gate";
      return { kind, step, questions: [], accept: true };
    });
}

/** The steps to name for a run: what waits for the user, else what works (a `foreach` through its tasks). */
export function current(run: TeamRun): Step[] {
  const waiting = asks(run).map((a) => a.step);
  return waiting.length ? waiting : run.steps.filter((s) => s.state === "running" && s.kind !== "foreach");
}

/** How far a run is: its steps over (done or skipped) of all of them, and what its budget has left. */
export function progress(run: TeamRun): { over: number; total: number; left: number | null; spent: number | null } {
  const counted = run.steps.filter((s) => s.kind !== "foreach" || !run.steps.some((t) => t.kind === "task"));
  return {
    over: counted.filter((s) => s.state === "done" || s.state === "skipped").length,
    total: counted.length,
    left: run.budget === null ? null : Math.max(0, run.budget - run.cost),
    spent: run.budget ? Math.min(1, run.cost / run.budget) : null,
  };
}

export type Notice = { kind: "gate" | "blocked" | "done" | "budget"; step?: string };

/** What to tell the user of a run's change: a gate that starts waiting, a step newly blocked, the end of the run or of its budget. */
export function notices(before: TeamRun | undefined, after: TeamRun): Notice[] {
  if (!before) return [];
  const was = new Map(before.steps.map((s) => [s.key, s.state]));
  const out: Notice[] = [];
  for (const step of after.steps) {
    if (step.state === was.get(step.key)) continue;
    if (step.state === "waiting") out.push({ kind: "gate", step: step.key });
    if (step.state === "blocked") out.push({ kind: "blocked", step: step.key });
  }
  if (after.state !== before.state && (after.state === "done" || after.state === "budget")) out.push({ kind: after.state });
  return out;
}

/** A budget typed for a team: none keeps the workflow's or the run's; one must be above what the run spent. */
export function budgetError(amount: number | null | undefined, spent = 0): "invalid" | "spent" | null {
  if (amount === null || amount === undefined) return null;
  if (!Number.isFinite(amount) || amount <= 0) return "invalid";
  return amount > spent ? null : "spent";
}
