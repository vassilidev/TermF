// No imports, so `pnpm test` runs it in plain Node.

/** The setup done in several goes, over several screens, in its order. */
export type SetupId = "claude" | "project" | "tickets" | "run" | "deliver";

export type Facts = { claude: boolean; projects: number; tickets: boolean; runs: number; delivered: boolean };

/** Each item of the setup and whether it is done; a run or a delivery made says the earlier items were done too. */
export function setup(f: Facts): { id: SetupId; done: boolean }[] {
  const delivered = f.delivered;
  const ran = delivered || f.runs > 0;
  return [
    { id: "claude", done: f.claude || ran },
    { id: "project", done: f.projects > 0 },
    { id: "tickets", done: f.tickets || ran },
    { id: "run", done: ran },
    { id: "deliver", done: delivered },
  ];
}

/**
 * The one next step of the Run page, what happens once the user acts: a blocked step goes to the lead, a question
 * waits for its answer, the delivery for its review, a team at work tells the user it will call. The cleanup has its
 * own card, and a run with nothing left says nothing.
 */
export function runNext(r: { expected: "answer" | "deliver" | "clean" | null; blocked: boolean; working: boolean }):
  | "failed"
  | "answer"
  | "deliver"
  | "working"
  | null {
  if (r.blocked) return "failed";
  if (r.expected === "answer") return "answer";
  if (r.expected === "deliver") return "deliver";
  if (r.expected === "clean") return null;
  return r.working ? "working" : null;
}

type CiRun = { status: string; conclusion: string | null; workflow: string; branch: string | null };

/** The run that failed last on a branch, when it is the latest finished one there: the moment to ask devops why. */
export function ciFailed<R extends CiRun>(runs: R[], branch: string | null): R | null {
  const done = runs.filter((r) => r.status === "completed" && (!branch || r.branch === branch));
  const last = done[0];
  return last && (last.conclusion === "failure" || last.conclusion === "timed_out") ? last : null;
}
