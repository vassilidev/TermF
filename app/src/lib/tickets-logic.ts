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

/** Where people sign up with any address: their domain says nothing of a company's Jira. */
const MAILBOXES = ["gmail.com", "googlemail.com", "outlook.com", "hotmail.com", "live.com", "yahoo.com", "icloud.com", "me.com", "proton.me", "protonmail.com"];

/**
 * The Jira site to propose: one the commits link, else the company's name in the e-mail's domain
 * (alex@acme.io → acme.atlassian.net); `from` says which, for the hint under the field.
 */
export function guessSite(hints: { sites: string[]; email: string | null }): { site: string; from: "commits" | "email" | null } {
  if (hints.sites[0]) return { site: hints.sites[0], from: "commits" };
  const domain = hints.email?.split("@")[1]?.toLowerCase() ?? "";
  const name = domain.split(".").slice(-2)[0] ?? "";
  if (!domain || MAILBOXES.includes(domain) || !/^[a-z0-9-]+$/.test(name)) return { site: "", from: null };
  return { site: `${name}.atlassian.net`, from: "email" };
}

/** The projects checked from the start: those whose keys the commits cite, else the only one. */
export function preChecked(projects: { key: string }[], cited: string[]): string[] {
  const found = projects.filter((p) => cited.includes(p.key)).map((p) => p.key);
  return found.length || projects.length !== 1 ? found : [projects[0].key];
}

/** The inbox's JQL for the chosen projects: the user's open tickets there; none chosen, Jira's default. */
export function inboxJql(keys: string[]): string | null {
  return keys.length ? `project in (${keys.join(", ")}) AND assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC` : null;
}

/** What an MCP source lists, in the words Claude Code reads. */
export function mcpPrompt(keys: string[], site: string): string {
  const where = site ? ` on ${site}` : "";
  return keys.length ? `The open Jira issues assigned to me in the projects ${keys.join(", ")}${where}.` : `The open Jira issues assigned to me${where}.`;
}

/** The name of a new MCP source for a server, unique among `taken`, as the sources editor makes it. */
export function mcpName(server: string, taken: string[]): string {
  const base =
    server
      .toLowerCase()
      .replace(/^claude\.ai /, "")
      .replace(/[^a-z0-9_-]+/g, "-")
      .replace(/^-+|-+$/g, "") || "mcp";
  let name = base;
  for (let n = 2; taken.includes(name); n++) name = `${base}-${n}`;
  return name;
}

/** The sources a browse reads: Jira, GitHub Issues, and the MCP ones. */
export const browsable = (sources: string[]) => sources.filter((s) => s === "jira" || s === "github" || s.startsWith("mcp:"));
