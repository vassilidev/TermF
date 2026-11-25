// No imports but types, so `pnpm test` runs it in plain Node.
import type { Job, RunDetail } from "./ci.svelte";

/** `running`, `queued`, `passed`, `failed` or `stopped` (cancelled, skipped…): how a run or a job is shown. */
export function outcome(item: { status: string; conclusion: string | null }) {
  if (item.status !== "completed") return item.status === "in_progress" ? "running" : "queued";
  if (item.conclusion === "success") return "passed";
  return ["failure", "timed_out", "startup_failure", "action_required"].includes(item.conclusion ?? "")
    ? "failed"
    : "stopped";
}

/** The next index among `count`, wrapping; from none yet (-1), going back starts at the end. */
export function cycle(at: number, direction: 1 | -1, count: number): number {
  if (!count) return -1;
  if (at < 0) return direction > 0 ? 0 : count - 1;
  return (at + direction + count) % count;
}

/** Each CI run → its failed jobs, from the details just read only: nothing of an earlier attempt stays. */
export function failedJobs(details: RunDetail[]): Record<number, Job[]> {
  return Object.fromEntries(details.map((d) => [d.run.id, d.jobs.filter((j) => outcome(j) === "failed")]));
}

/** A job of the workflow file: its key, the start of the name GitHub shows, the keys it needs. */
export type Node = { key: string; name: string; partial: boolean; needs: string[] };
type JobLike = { id: number; name: string; status: string; conclusion: string | null };
/** A box of the graph: a job of the file and the jobs of the run it became (several for a matrix). */
export type Cell<J extends JobLike = JobLike> = { key: string; name: string; needs: string[]; jobs: J[] };
export type CellState = "running" | "queued" | "passed" | "failed" | "stopped" | "skipped" | "waiting";

/**
 * Whether GitHub's job name is the file's job: its name, a matrix `name (values)`, a reusable `name / job`; `loose`,
 * for a name that had an expression, its start followed by a separator (`test ubuntu-latest` for `test ${{ … }}`).
 */
export function matches(node: Node, name: string, loose = false): boolean {
  if (name === node.name || name.startsWith(`${node.name} (`) || name.startsWith(`${node.name} / `)) return true;
  return loose && node.partial && name.startsWith(node.name) && /[\s(\-_:/]/.test(name.charAt(node.name.length));
}

/**
 * The run's jobs in columns, each one right of what it needs: the file's jobs (those not started yet included), then
 * any job of the run the file does not tell, in the first column. Exact names go first, so that a loose start never
 * takes another job's (`test` and `test-e2e`).
 */
export function columns<J extends JobLike>(nodes: Node[], jobs: J[]): Cell<J>[][] {
  const taken = new Set<number>();
  const cells: Cell<J>[] = nodes.map((n) => ({ key: n.key, name: n.name, needs: n.needs.filter((k) => nodes.some((o) => o.key === k)), jobs: [] }));
  for (const loose of [false, true])
    nodes.forEach((n, i) => {
      const own = jobs.filter((j) => !taken.has(j.id) && matches(n, j.name, loose));
      own.forEach((j) => taken.add(j.id));
      cells[i].jobs.push(...own);
    });
  for (const j of jobs) if (!taken.has(j.id)) cells.push({ key: `job:${j.id}`, name: j.name, needs: [], jobs: [j] });
  const level = new Map<string, number>();
  const depth = (cell: Cell<J>, seen: string[]): number => {
    const known = level.get(cell.key);
    if (known !== undefined) return known;
    // A cycle in a broken file stops at the job seen twice.
    const parents = cell.needs.filter((k) => !seen.includes(k)).map((k) => cells.find((c) => c.key === k) as Cell<J>);
    const d = parents.length ? 1 + Math.max(...parents.map((p) => depth(p, [...seen, cell.key]))) : 0;
    level.set(cell.key, d);
    return d;
  };
  const out: Cell<J>[][] = [];
  for (const cell of cells) (out[depth(cell, [cell.key])] ??= []).push(cell);
  return out.filter(Boolean);
}

/** The box's state: its jobs' worst, `waiting` before any of them exists, `skipped` when all were. */
export function cellState(cell: { jobs: JobLike[] }): CellState {
  if (!cell.jobs.length) return "waiting";
  const all = cell.jobs.map(outcome);
  if (all.includes("failed")) return "failed";
  if (all.includes("running")) return "running";
  if (all.includes("queued")) return "queued";
  if (cell.jobs.every((j) => j.conclusion === "skipped")) return "skipped";
  return all.includes("stopped") ? "stopped" : "passed";
}

type RunLike = { id: number; status: string; conclusion: string | null; branch: string | null; pullRequests: number[]; created: string };

const newestFirst = <R extends { created: string }>(runs: R[]) =>
  [...runs].sort((a, b) => Date.parse(b.created) - Date.parse(a.created));

/** A branch's last runs, oldest first, for the row of squares; and how many failed in a row up to the newest. */
export function history<R extends RunLike>(runs: R[], count = 20): { squares: R[]; failing: number } {
  const newest = newestFirst(runs).slice(0, count);
  const done = newest.filter((r) => r.status === "completed");
  const firstOk = done.findIndex((r) => outcome(r) !== "failed");
  return { squares: newest.reverse(), failing: firstOk < 0 ? done.length : firstOk };
}

/** The branches the runs are on, the most recent first, with their pull requests and runs (newest first). */
export function branches<R extends RunLike>(runs: R[]): { branch: string; prs: number[]; runs: R[] }[] {
  const by = new Map<string, R[]>();
  for (const r of newestFirst(runs)) by.set(r.branch ?? "", [...(by.get(r.branch ?? "") ?? []), r]);
  return [...by].map(([branch, list]) => ({ branch, prs: [...new Set(list.flatMap((r) => r.pullRequests))], runs: list }));
}

type StepLike = { name: string; status: string; conclusion: string | null };

/** The first job that failed, in the graph's order, and its first failed step. */
export function firstFailure<J extends JobLike & { steps: StepLike[] }>(cols: Cell<J>[][]): { job: J; step: string | null } | null {
  for (const col of cols)
    for (const cell of col)
      for (const job of cell.jobs)
        if (outcome(job) === "failed") return { job, step: job.steps.find((s) => outcome(s) === "failed")?.name ?? null };
  return null;
}

/** The lines around a log's first error: `start` indexes `lines`; `at`, the error, indexes the excerpt. */
export function excerpt(log: { lines: string[]; errors: number[] }, before = 8, after = 3): { start: number; lines: string[]; at: number } | null {
  // The first error a tool printed, not GitHub's closing "Process completed with exit code".
  const first = log.errors.find((i) => !log.lines[i]?.startsWith("##[error]Process completed")) ?? log.errors[0];
  if (first === undefined) return null;
  const start = Math.max(0, first - before);
  return { start, lines: log.lines.slice(start, first + after + 1), at: first - start };
}

type DeploymentLike = { sha: string | null; branch: string | null; pr: number | null };
type TicketRunLike = { id: number; branch: string | null; prUrl: string | null };

/**
 * What a deployment comes from: its pull request (told by the tool, else that of a CI run on its commit or branch),
 * and the TermF run that made that branch or pull request.
 */
export function origin(d: DeploymentLike, runs: (RunLike & { sha: string })[], tickets: TicketRunLike[]): { pr: number | null; run: number | null } {
  const ci = runs.find((r) => (d.sha && r.sha.startsWith(d.sha)) || (d.branch && r.branch === d.branch));
  const pr = d.pr ?? ci?.pullRequests[0] ?? null;
  const branch = d.branch ?? ci?.branch ?? null;
  const run = tickets.find((t) => (branch && t.branch === branch) || (pr !== null && t.prUrl?.endsWith(`/pull/${pr}`)));
  return { pr, run: run?.id ?? null };
}

/**
 * `origin` for a target's deployment seen from the repository shown (`repo`, whose CI runs are `runs`): only when the
 * target deploys that repository, else its pull request numbers and branches are another repository's. `here` false:
 * the pull request the tool told, without a link.
 */
export function originIn(
  target: { repo?: string | null },
  repo: string,
  d: DeploymentLike,
  runs: (RunLike & { sha: string })[],
  tickets: (TicketRunLike & { repo: string })[],
): { pr: number | null; run: number | null; here: boolean } {
  if (!target.repo || target.repo !== repo) return { pr: d.pr, run: null, here: false };
  return { ...origin(d, runs, tickets.filter((t) => t.repo === repo)), here: true };
}

/** The Webhooks tab's choices of events, as GitHub names them. */
export const HOOK_EVENTS: Record<string, string[]> = { push: ["push"], prs: ["push", "pull_request"], all: ["*"] };

/** The choice a webhook's events match, in any order; `keep` for any other set, which an edit leaves as it is. */
export function hookEventsKind(events: string[]): string {
  const key = (list: string[]) => [...list].sort().join();
  return Object.keys(HOOK_EVENTS).find((k) => key(HOOK_EVENTS[k]) === key(events)) ?? "keep";
}
