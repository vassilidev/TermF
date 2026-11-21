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
type Session = { id: number; running: boolean; cwd: string; status: string | null };
type Cleanup = { run: Run; cleaned: boolean; error: string | null };
type Refusal = { kind: "uncommitted" | "unpushed" | "error"; message: string };

export function liveSession<S extends Session>(run: Pick<Run, "sessionId" | "worktree">, sessions: S[]): S | undefined {
  return (
    sessions.find((s) => s.id === run.sessionId && s.running) ??
    // A Claude Code session opened by hand in the worktree may hold the run's conversation: resuming would be refused.
    sessions.find((s) => s.running && s.status !== null && s.cwd === run.worktree)
  );
}

export function readyToClean(run: Pick<Run, "status" | "prState">): boolean {
  return run.status === "delivered" && !!run.prState;
}

// A run started before conversations were kept has none to resume.
export function resumable(run: Pick<Run, "status" | "worktree" | "claudeSession">): boolean {
  return (run.status === "running" || run.status === "delivered") && !!run.claudeSession && !!run.worktree;
}

// Only a `CloseError` has a kind; an IPC failure rejects with a string.
export function refusal(e: unknown): Refusal {
  const error = e as { kind?: unknown; message?: unknown } | null;
  if (error && typeof error === "object" && typeof error.message === "string") {
    const kind = error.kind === "uncommitted" || error.kind === "unpushed" ? error.kind : "error";
    return { kind, message: error.message };
  }
  return { kind: "error", message: String(e) };
}

export function cleanupNotice(c: Cleanup): { title: Key; body: Key; vars: Record<string, string> } {
  return {
    title: c.run.prState === "closed" ? "ticketsUiNotifyClosed" : "ticketsUiNotifyMerged",
    body: c.cleaned ? "ticketsUiNotifyCleaned" : c.error ? "ticketsUiNotifyFailed" : "ticketsUiNotifyReady",
    vars: { key: c.run.ticket.key, title: c.run.title, error: c.error ?? "" },
  };
}
