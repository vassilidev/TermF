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
