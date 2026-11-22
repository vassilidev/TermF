import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { t } from "$lib/i18n";
import { app, ui } from "$lib/state.svelte";
import { cleanupNotice } from "$lib/tickets-logic";
import type { Done } from "$lib/team-run-logic";

export type TicketRef = { provider: string; key: string; url: string };
export type TicketSummary = TicketRef & {
  title: string;
  state: string;
  labels: string[];
  /** GitHub logins or display names. */
  assignees: string[];
  /** ISO 8601. */
  updated: string;
  project: string | null;
  /** `github`, `githubProject`, `jira` or `mcp:<name>`. */
  source: string;
};
/** `source`: `github`, `githubProject`, `jira` or `mcp:<name>`. */
export type SourceError = { source: string; message: string };
export type Inbox = { tickets: TicketSummary[]; errors: SourceError[] };
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
  /** The Claude Code conversation a resume takes up; none for a run started before conversations were kept. */
  claudeSession: string | null;
  /** Known once its pull request is merged or closed: the run is then ready to clean up while delivered. */
  prState: "merged" | "closed" | null;
  /** The run it works for in another repository of the project, when the team's plan spans several. */
  parent?: number | null;
  /** Its terminal session while it exists, for this launch of the app. */
  sessionId: number | null;
};
export type RunEvent = { id: number; run: number; at: number } & (
  | { kind: "worktree_created"; path: string; branch: string; base: string }
  | { kind: "session_opened"; session: number }
  | { kind: "session_resumed"; session: number; conversation: string }
  | { kind: "committed"; commit: string; message: string }
  | { kind: "pushed"; branch: string; commit: string }
  | { kind: "pr_opened"; url: string }
  | { kind: "pr_merged"; url: string }
  | { kind: "pr_closed"; url: string }
  | { kind: "ticket_commented"; url: string }
  | { kind: "ticket_moved"; to: string }
  | { kind: "closed"; dropped: string[] }
  | { kind: "failed"; error: string }
  | { kind: "setup"; steps: { outcome: string }[]; saved: boolean }
  | { kind: "teardown"; steps: { outcome: string }[] }
);
export type DeliveryPreview = {
  files: { path: string; change: "added" | "modified" | "deleted" | "renamed"; from: string | null }[];
  commits: { commit: string; subject: string }[];
  fingerprint: string;
  /** What the definition of done misses, when the run's team stopped short of its delivery gate. */
  done?: Done | null;
};
export type Delivery = { run: Run; warning: string | null };
export type Resumed = { run: Run; warning: string | null };
export type Cleanup = { run: Run; cleaned: boolean; error: string | null };

export const tickets = $state({
  /** Loaded once first shown, then kept mounted, like the Library view. */
  loaded: false,
  /** The current project's id, or "" for every ticket assigned to the user. */
  get scope(): string {
    return app.project ?? "";
  },
  tab: "inbox" as "inbox" | "runs",
  /** null while the first load of the scope runs. */
  inbox: null as TicketSummary[] | null,
  inboxError: "",
  /** The sources that could not be read; the others' tickets are in `inbox`. */
  inboxErrors: [] as SourceError[],
  /** Whether the current project has a ticket source; null for every project, or while unknown. */
  configured: null as boolean | null,
  loading: false,
  runs: [] as Run[],
  /** The run its page shows, by id. */
  selected: null as number | null,
  /** Asks the paste field for the keyboard. */
  paste: false,
  error: "",
  done: "",
});

export function showTickets() {
  ui.view = "tickets";
  tickets.loaded = true;
  refresh();
  // What it finds comes back as `run-updated` and `run-cleanup`.
  invoke("tickets_check").catch(() => {});
}

export function launchTicket() {
  showTickets();
  tickets.paste = true;
}

export function refresh() {
  return Promise.all([loadInbox(), loadRuns()]);
}

let inboxRequest = 0;

/** Only the latest request updates the inbox: an older answer may come last. */
export async function loadInbox() {
  const request = ++inboxRequest;
  tickets.inboxError = "";
  tickets.configured = null;
  tickets.loading = true;
  try {
    const project = tickets.scope || null;
    const [inbox, configured] = await Promise.all([
      invoke<Inbox>("tickets_inbox", { project }),
      project
        ? invoke<{ configured: boolean }[]>("tickets_configured", { project }).catch(() => [])
        : Promise.resolve([]),
    ]);
    if (request === inboxRequest) {
      tickets.inbox = inbox.tickets;
      tickets.inboxErrors = inbox.errors;
      tickets.configured = configured[0]?.configured ?? null;
    }
  } catch (e) {
    if (request === inboxRequest) tickets.inboxError = String(e);
  } finally {
    if (request === inboxRequest) tickets.loading = false;
  }
}

export async function loadRuns() {
  try {
    tickets.runs = await invoke<Run[]>("tickets_runs");
  } catch (e) {
    tickets.error = String(e);
  }
}

export function openUrl(url: string) {
  invoke("open_url", { url }).catch((e) => (tickets.error = String(e)));
}

/** Takes a run as `run-updated` or a command gives it, newest first. */
export function upsert(run: Run) {
  const i = tickets.runs.findIndex((r) => r.id === run.id);
  if (i >= 0) tickets.runs[i] = run;
  else tickets.runs.unshift(run);
}

/** Runs the whole app: keeps the runs current, and notifies the pull requests found merged or closed. */
export function watchTickets(): () => void {
  // The runs tell which project a ticket's session belongs to.
  loadRuns();
  const updated = listen<Run>("run-updated", ({ payload }) => upsert(payload));
  const cleanup = listen<Cleanup>("run-cleanup", async ({ payload }) => {
    const notice = cleanupNotice(payload);
    const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
    if (granted) sendNotification({ title: t(notice.title, notice.vars), body: t(notice.body, notice.vars) });
  });
  return () => {
    updated.then((stop) => stop());
    cleanup.then((stop) => stop());
  };
}
