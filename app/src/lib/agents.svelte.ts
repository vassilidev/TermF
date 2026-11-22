import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import type { AgentState } from "$lib/conversation-logic";
import { lang, t } from "$lib/i18n";
import { projectsOf } from "$lib/nav-logic";
import { app, setProject, ui, type Gauge } from "$lib/state.svelte";
import { tickets } from "$lib/tickets.svelte";

/** A headless session, as `agent_sessions` and `agent-updated` give it. */
export type AgentSession = {
  id: number;
  runtime: string;
  sessionId: string;
  cwd: string;
  title: string;
  project: string | null;
  agent: string | null;
  model: string | null;
  effort: string | null;
  permissionMode: string | null;
  budget: number | null;
  /** The library role it plays. */
  role?: string | null;
  state: AgentState;
  cost: number;
  context: Gauge | null;
  tile: number | null;
  created: number;
  updated: number;
};

export const agents = $state({
  /** The view is mounted once first shown, then kept. */
  loaded: false,
  /** Most recent first. */
  sessions: [] as AgentSession[],
  /** The conversation picked; null: the most recent of the current project, until the view fixes it. */
  selected: null as number | null,
  /** The new session form is shown. */
  creating: false,
});

const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD", minimumFractionDigits: 2, maximumFractionDigits: 3 });
export const usd = (amount: number) => money.format(amount);

export function agentProjects(session: AgentSession): string[] {
  const ids = projectsOf(session.cwd, app.workspace?.projects ?? [], tickets.runs);
  return session.project && !ids.includes(session.project) ? [session.project, ...ids] : ids;
}

/** Under the current project, or every one. */
export function inScope(session: AgentSession): boolean {
  return app.project === null || agentProjects(session).includes(app.project);
}

/** A permission waits for the user. */
export function waitingAgents(): AgentSession[] {
  return agents.sessions.filter((a) => a.state === "waiting");
}

/** The sessions the view lists: the current project's, most recent first. */
export function listed(): AgentSession[] {
  return agents.sessions.filter(inScope).sort((a, b) => b.id - a.id);
}

/** The conversation the view shows, or keeps behind the new session form. */
export function conversation(): AgentSession | undefined {
  const list = listed();
  return list.find((a) => a.id === agents.selected) ?? list[0];
}

export function showConversations() {
  ui.view = "conversations";
  agents.loaded = true;
}

/** Shows a session's conversation, switching to its project if the current one does not hold it. */
export function showConversation(id: number) {
  const session = agents.sessions.find((a) => a.id === id);
  if (session && !inScope(session)) setProject(agentProjects(session)[0] ?? null);
  agents.selected = id;
  agents.creating = false;
  showConversations();
}

export function upsert(session: AgentSession) {
  const i = agents.sessions.findIndex((a) => a.id === session.id);
  if (i >= 0) agents.sessions[i] = session;
  else agents.sessions.unshift(session);
}

const notices: Partial<Record<AgentState, Parameters<typeof t>[0]>> = {
  waiting: "chatNotifyPermission",
  error: "chatNotifyError",
  budget: "chatNotifyBudget",
};

/** Keeps the sessions current whatever view is shown, and notifies as for tiles, while the window is in the background. */
export function watchAgents(): () => void {
  // An update heard while the list loads is newer than the list's copy, unless the list says otherwise.
  invoke<AgentSession[]>("agent_sessions").then(
    (list) => {
      const heard = new Map(agents.sessions.map((a) => [a.id, a]));
      const read = list.map((s) => {
        const known = heard.get(s.id);
        return known && known.updated >= s.updated ? known : s;
      });
      agents.sessions = [...agents.sessions.filter((a) => !list.some((s) => s.id === a.id)), ...read];
    },
    (e) => (app.error = String(e)),
  );
  const stop = listen<AgentSession>("agent-updated", async ({ payload }) => {
    const before = agents.sessions.find((a) => a.id === payload.id)?.state;
    upsert(payload);
    const notice = notices[payload.state];
    if (!notice || before === undefined || before === payload.state || document.hasFocus()) return;
    const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
    if (granted) sendNotification({ title: payload.title, body: t(notice) });
  });
  return () => void stop.then((f) => f());
}
