// No imports but types, so `pnpm test` runs it in plain Node.
import type { t } from "$lib/i18n";

type Key = Parameters<typeof t>[0];
type Run = {
  status: string;
  worktree: string | null;
  claudeSession: string | null;
  prState: string | null;
  sessionId: number | null;
  ticket: { key: string };
  title: string;
};
type Session = { id: number; running: boolean };
type Cleanup = { run: Run; cleaned: boolean; error: string | null };
type Refusal ={ kind: "uncommitted" | "unpushed" | "error"; message: string };

/** The run's terminal session, while it runs. */
export function liveSession<S extends Session>(run: Pick<Run, "sessionId">, sessions: S[]): S | undefined {
  return sessions.find((s) => s.id === run.sessionId && s.running);
}

/** A delivered run whose pull request was merged or closed: its worktree may go. */
export function readyToClean(run: Pick<Run, "status" | "prState">): boolean {
  return run.status === "delivered" && !!run.prState;
}

/** Once its session is gone; a run started before conversations were kept cannot be resumed. */
export function resumable(run: Pick<Run, "status" | "worktree" | "claudeSession">): boolean {
  return (run.status === "running" || run.status === "delivered") && !!run.claudeSession && !!run.worktree;
}

/** `tickets_close` and `tickets_cleanup` reject with a `CloseError`; any other rejection is an error. */
export function refusal(e: unknown): Refusal {
  const error = e as { kind?: unknown; message?: unknown } | null;
  if (error && typeof error === "object" && typeof error.message === "string") {
    const kind = error.kind === "uncommitted" || error.kind === "unpushed" ? error.kind : "error";
    return { kind, message: error.message };
  }
  return { kind: "error", message: String(e) };
}

/** The system notification of a pull request found merged or closed. */
export function cleanupNotice(c: Cleanup): { title: Key; body: Key; vars: Record<string, string> } {
  return {
    title: c.run.prState === "closed" ? "ticketsUiNotifyClosed" : "ticketsUiNotifyMerged",
    body: c.cleaned ? "ticketsUiNotifyCleaned" : c.error ? "ticketsUiNotifyFailed" : "ticketsUiNotifyReady",
    vars: { key: c.run.ticket.key, title: c.run.title, error: c.error ?? "" },
  };
}
