import { invoke } from "@tauri-apps/api/core";
import { files } from "$lib/files.svelte";

export type TicketRef = { provider: string; key: string; url: string };
export type TicketSummary = TicketRef & {
  title: string;
  state: string;
  labels: string[];
  /** ISO 8601. */
  updated: string;
  project: string | null;
};
export type Resolution = {
  ticket: TicketRef;
  project: string;
  repos: { name: string; path: string }[];
  /** The repository's path when it is clear, else one of `repos` must be chosen. */
  repo: string | null;
};
export type RunStatus = "running" | "delivered" | "closed" | "failed";
export type Run = {
  id: number;
  project: string;
  ticket: TicketRef;
  title: string;
  repo: string;
  worktree: string | null;
  branch: string | null;
  base: string | null;
  status: RunStatus;
  prUrl: string | null;
  created: number;
  updated: number;
};
export type RunEvent = { id: number; run: number; at: number } & (
  | { kind: "worktree_created"; path: string; branch: string; base: string }
  | { kind: "session_opened"; session: number }
  | { kind: "committed"; commit: string; message: string }
  | { kind: "pushed"; branch: string; commit: string }
  | { kind: "pr_opened"; url: string }
  | { kind: "ticket_commented"; url: string }
  | { kind: "closed"; dropped: string[] }
  | { kind: "failed"; error: string }
);
export type DeliveryPreview = {
  files: { path: string; change: "added" | "modified" | "deleted" | "renamed"; from: string | null }[];
  commits: { commit: string; subject: string }[];
  fingerprint: string;
};
export type Delivery = { run: Run; warning: string | null };

export const tickets = $state({
  /** Loaded once first shown, then kept mounted, like the Library view. */
  loaded: false,
  /** A project id, or "" for every ticket assigned to the user. */
  scope: "",
  tab: "inbox" as "inbox" | "runs",
  /** null while the first load of the scope runs. */
  inbox: null as TicketSummary[] | null,
  inboxError: "",
  loading: false,
  runs: [] as Run[],
  /** The run shown in the detail pane, by id. */
  selected: null as number | null,
  /** Asks the paste field for the keyboard. */
  paste: false,
  error: "",
  done: "",
});

export function showTickets() {
  files.view = "tickets";
  tickets.loaded = true;
  refresh();
}

export function launchTicket() {
  showTickets();
  tickets.paste = true;
}

export function refresh() {
  return Promise.all([loadInbox(), loadRuns()]);
}

export async function loadInbox() {
  const scope = tickets.scope;
  tickets.inboxError = "";
  tickets.loading = true;
  try {
    const list = await invoke<TicketSummary[]>("tickets_inbox", { project: scope || null });
    if (scope === tickets.scope) tickets.inbox = list;
  } catch (e) {
    if (scope === tickets.scope) tickets.inboxError = String(e);
  } finally {
    if (scope === tickets.scope) tickets.loading = false;
  }
}

export async function loadRuns() {
  try {
    tickets.runs = await invoke<Run[]>("tickets_runs");
  } catch (e) {
    tickets.error = String(e);
  }
}

/** Takes a run as `run-updated` or a command gives it, newest first. */
export function upsert(run: Run) {
  const i = tickets.runs.findIndex((r) => r.id === run.id);
  if (i >= 0) tickets.runs[i] = run;
  else tickets.runs.unshift(run);
}
