// No imports, so `pnpm test` runs it in plain Node.

export type RunState = "running" | "waiting" | "paused" | "stopped" | "budget" | "done";
export type StepState = "pending" | "running" | "waiting" | "blocked" | "done" | "skipped";
export type StepKind = "agent" | "gate" | "foreach" | "shell" | "rules" | "visual" | "connector" | "task" | "review";

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
export type QuestionKind = "choice" | "multi" | "bool" | "text";
/** A question of a step's questionnaire; one stored before the questionnaire is a bare sentence with no id. */
export type Question = {
  id: string;
  theme: string;
  question: string;
  /** What the answer changes, or the side effect the agent found. */
  why: string;
  kind: QuestionKind;
  options: string[];
  /** The proposed answer, in the shape of an answer. */
  proposed: string[];
  /** What each option implies, in the order of `options`; empty when nothing to say. */
  implications: string[];
};
export type Plan = {
  summary: string;
  acceptance: { id: string; text: string }[];
  out_of_scope: string[];
  questions: Question[];
  tasks: Task[];
};
/** The CTO's routing of a run (its triage). */
export type Routing = {
  kind: string;
  size: "S" | "M" | "L";
  reason: string;
  workflow: string;
  skip: string[];
  add: string[];
  efforts: { step: string; effort: string }[];
  squads: string[];
  questions: Question[];
  budget_usd: number;
  /** The user changed it, or set it. */
  changed: boolean;
};
/** A workflow as the triage sees it (`team_workflows`). */
export type WorkflowView = {
  name: string;
  description: string;
  steps: { id: string; kind: StepKind; role: string | null; optional: boolean; on: boolean }[];
};
export type Answer = { step: string; question: Question; answer: string[]; at: number };
/** What a step's agent reports (appendix B). */
export type Report = {
  status: "done" | "blocked" | "failed";
  summary: string;
  covers: string[];
  text_changes: { file: string; text: string; justified_by: string }[];
  affected_views: { url: string; state: string; viewports: string[] }[];
  out_of_scope_findings: string[];
  questions?: Question[];
};
export type Violation = { rule: string; file: string | null; message: string; blocking: boolean };
/** What became of a finding of the code review: back to a task, for the user to decide, only listed, left as it is. */
export type Fate = "sent" | "asked" | "reported" | "left";
export type Finding = {
  /** F1, F2… within its round. */
  id: string;
  severity: "important" | "nit";
  file: string;
  line: number;
  scenario: string;
  fix: string;
  /** The task as the reviewer named it. */
  task: string;
  fate: Fate;
  /** The task that fixes it, or would. */
  to: string | null;
  /** Why it stays as it is, when someone said. */
  reason: string | null;
};
/** A round of a review step. */
export type Review = { step: string; round: number; summary: string; findings: Finding[]; at: number };
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
  /** Absent for a run that had no triage. */
  routing?: Routing | null;
  answers?: Answer[];
  /** Every round of the code review, in order; absent from a run made before the review. */
  reviews?: Review[];
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
export type AskKind = "route" | "plan" | "questions" | "visual" | "deliver" | "gate" | "blocked";
/** `accept`: approving it as it is can work (never a plan step or the triage: they have no answer yet, or questions). */
export type Ask = { kind: AskKind; step: Step; questions: Question[]; accept: boolean };

/** The CTO's first step of every run, and the gate where the user approves its routing. */
export const TRIAGE = "triage";
export const ROUTE = "approve-route";

const working = (run: TeamRun) => run.state === "running" || run.state === "waiting";

/** What waits for the user in a team at work; a paused, stopped or finished one only waits to be started again. */
export function asks(run: TeamRun): Ask[] {
  if (!working(run)) return [];
  const plans = new Set(run.steps.filter((s) => s.plan).map((s) => s.key));
  return run.steps
    .filter((step) => step.state === "waiting" || step.state === "blocked")
    .map((step) => {
      if (step.state === "blocked") {
        const triage = step.key === TRIAGE;
        const questions = (triage ? run.routing?.questions : step.plan ? run.plan?.questions : step.report?.questions) ?? [];
        return { kind: questions.length ? "questions" : "blocked", step, questions, accept: !step.plan && !triage };
      }
      const kind: AskKind =
        step.key === ROUTE
          ? "route"
          : step.kind === "visual"
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

/**
 * The runs with `run` in them, unless they hold a copy updated later: a command's answer may arrive after a newer
 * `team-updated`. At equal times the one received last wins; a new run goes first. The same list when nothing changes.
 */
export function upsert<R extends { run: number; updated: number }>(runs: R[], run: R): R[] {
  const i = runs.findIndex((r) => r.run === run.run);
  if (i < 0) return [run, ...runs];
  return runs[i].updated > run.updated ? runs : runs.map((r, j) => (j === i ? run : r));
}

/**
 * A budget typed for a team: none keeps the workflow's or the run's, unless one is `required` (its budget is spent);
 * one must be above what the run spent.
 */
export function budgetError(amount: number | null | undefined, spent = 0, required = false): "missing" | "invalid" | "spent" | null {
  if (amount === null || amount === undefined) return required ? "missing" : null;
  if (!Number.isFinite(amount) || amount <= 0) return "invalid";
  return amount > spent ? null : "spent";
}

/** The themes of a questionnaire, in the order the form shows them. */
export const themes = ["product", "scope", "interface", "technical", "security", "delivery", "other"];

/** The questions by theme, in a fixed order, a theme it does not know last, empty themes left out. */
export function byTheme(questions: Question[]): [string, Question[]][] {
  const order = [...themes, ...new Set(questions.map((q) => q.theme).filter((t) => !themes.includes(t)))];
  return order
    .map((theme) => [theme, questions.filter((q) => q.theme === theme)] as [string, Question[]])
    .filter(([, list]) => list.length);
}

/** Whether the questions make a form: each has an id (one stored before the questionnaire is a bare sentence). */
export function isForm(questions: Question[]): boolean {
  return questions.length > 0 && questions.every((q) => q.id);
}

/** The answers the form starts with: the proposed ones. */
export function proposed(questions: Question[]): Record<string, string[]> {
  return Object.fromEntries(questions.map((q) => [q.id, [...q.proposed]]));
}

/**
 * The questions left without an answer: one value for a choice (an option, or an answer of one's own), yes or no,
 * a text; a multiple choice may tick nothing.
 */
export function unanswered(questions: Question[], answers: Record<string, string[]>): string[] {
  return questions
    .filter((q) => {
      const answer = answers[q.id] ?? [];
      if (answer.some((a) => !a.trim())) return true;
      if (q.kind === "multi") return false;
      if (answer.length !== 1) return true;
      return q.kind === "bool" && !["yes", "no"].includes(answer[0]);
    })
    .map((q) => q.id);
}

/** The answers once the user's own are in: for a choice it replaces the option, for a multiple choice it adds to them. */
export function withOthers(
  questions: Question[],
  answers: Record<string, string[]>,
  others: Record<string, string>,
): Record<string, string[]> {
  return Object.fromEntries(
    questions.map((q) => {
      const own = (others[q.id] ?? "").trim();
      const answer = answers[q.id] ?? [];
      if (!own || (q.kind !== "choice" && q.kind !== "multi")) return [q.id, answer];
      return [q.id, q.kind === "choice" ? [own] : [...answer, own]];
    }),
  );
}

/** The run's budget a change of the routing starts from: never above the run's own, nor above the CTO's advice. */
export function routeBudget(budget: number | null, advice: number): number | null {
  const amounts = [budget, advice > 0 ? advice : null].filter((b): b is number => b !== null);
  return amounts.length ? Math.min(...amounts) : null;
}

/** Whether an optional step runs under `routing`: on by default unless skipped, off unless added. */
export function runs(step: { id: string; on: boolean }, routing: Pick<Routing, "skip" | "add">): boolean {
  return routing.add.includes(step.id) || (step.on && !routing.skip.includes(step.id));
}

/**
 * The routing once the user picks `workflow`, the optional steps that run and the squads: skip and add rebuilt
 * against that workflow's defaults, the efforts of the steps it does not run dropped. The run's budget goes apart.
 */
export function editRouting(routing: Routing, workflow: WorkflowView, running: string[], squads: string[]): Routing {
  const optional = workflow.steps.filter((s) => s.optional);
  const agents = workflow.steps
    .filter((s) => (s.kind === "agent" || s.kind === "review") && (!s.optional || running.includes(s.id)))
    .map((s) => s.id);
  return {
    ...routing,
    workflow: workflow.name,
    skip: optional.filter((s) => s.on && !running.includes(s.id)).map((s) => s.id),
    add: optional.filter((s) => !s.on && running.includes(s.id)).map((s) => s.id),
    efforts: routing.efforts.filter((e) => agents.includes(e.step)),
    squads,
    questions: [],
  };
}

/** The rounds of a review step, oldest first. */
export function rounds(run: TeamRun, step: string): Review[] {
  return (run.reviews ?? []).filter((r) => r.step === step);
}

/** How a round ended: its findings, the tasks they go back to, and how many wait for the user, are only listed, or are left. */
export function tally(review: Review): { count: number; to: string[]; asked: number; reported: number; left: number } {
  const count = (fate: Fate) => review.findings.filter((f) => f.fate === fate).length;
  const to = review.findings.filter((f) => f.fate === "sent" && f.to).map((f) => f.to as string);
  return { count: review.findings.length, to: [...new Set(to)].sort(), asked: count("asked"), reported: count("reported"), left: count("left") };
}
