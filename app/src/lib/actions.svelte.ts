import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { forAgent, gate, refused, secretMissing, takeWait, vaultBlock, withoutCard, type Args, type Spec } from "$lib/actions-logic";
import { agents, type AgentSession } from "$lib/agents.svelte";
import { ask, wide } from "$lib/ask.svelte";
import { ciActions, ciHere } from "$lib/ci-actions";
import { conversationActions } from "$lib/conversation-actions";
import { designActions } from "$lib/design-actions";
import { envActions, envHere } from "$lib/env-actions";
import { gitActions } from "$lib/git-actions";
import { t } from "$lib/i18n";
import { libraryActions, pluginActions } from "$lib/library-actions";
import { opsActions, opsHere } from "$lib/ops-actions";
import { opsMoreActions } from "$lib/ops-more-actions";
import { projectActions } from "$lib/project-actions";
import { schedulerActions, schedulerHere } from "$lib/scheduler-actions";
import { settingsActions, settingsRead, TERMF_EXPERT } from "$lib/settings-actions";
import { setupActions } from "$lib/setup-actions";
import { stackActions, stackHere } from "$lib/stack-actions";
import { syncFacts } from "$lib/sync.svelte";
import { ticketActions, ticketsHere } from "$lib/tickets-actions";
import { toast } from "$lib/ui/toast.svelte";
import { vaultActions, vaultAnywhere, vaultHere } from "$lib/vault-actions";

type Key = Parameters<typeof t>[0];

/**
 * An action of a screen that an agent may ask for (`do_action` of `termf mcp`): what the agent reads (`Spec`), what
 * TermF's card says, and its run with the app's own commands.
 */
export type Action = Spec & {
  /** The card's title. */
  label: Key;
  /** The card's label of each secret field. */
  labels?: Record<string, Key>;
  /** Exactly what it does, as the card says it. */
  describe: (args: Args, project: string) => string;
  /** What it makes lose, for one that destroys or replaces. */
  loses?: (args: Args, project: string) => string;
  /** Who else gets what the user types in its card (the project's agents), instead of "never the agent". */
  gives?: (args: Args, project: string) => string;
  /** What it waits for (no vault yet, no connection…), naming the action that settles it; null when ready. */
  needs?: (args: Args, project: string) => Promise<string | null>;
  /** Whether its card shows even for a read (a connection's route the user has not seen yet). */
  cardAnyway?: (args: Args, project: string) => Promise<boolean>;
  /** Runs it, updates the screen and says so in a toast; what the agent reads back, never a secret. `session`: the
   * conversation that asked. */
  run: (args: Args, secrets: Record<string, string>, project: string, session: number) => Promise<unknown>;
};

/** Each screen's actions, by view, and those of anywhere. */
const SCREENS: Record<string, Action[]> = {
  ops: [...opsActions, ...opsMoreActions],
  tickets: ticketActions,
  ci: ciActions,
  library: libraryActions,
  git: gitActions,
  settings: settingsActions,
  scheduler: schedulerActions,
  newProject: setupActions,
  design: designActions,
  vault: vaultActions,
  vaultAll: vaultActions,
  project: [...projectActions, ...ticketActions],
  claude: pluginActions,
  env: envActions,
  stack: stackActions,
};
const GLOBAL: Action[] = [...vaultAnywhere, ...conversationActions, settingsRead];
/**
 * Every screen's reads: what TermF knows of the project, from any conversation. Not Ops': they reach servers and
 * prod data, so only from the Ops screen, as before.
 */
const READS = Object.values(SCREENS)
  .flat()
  .filter((a, i, all) => gate(a.id) === "read" && !a.id.startsWith("ops.") && all.findIndex((b) => b.id === a.id) === i);
/** A role's own actions, whatever the screen: the TermF expert changes TermF's settings from anywhere. */
const ROLES: Record<string, Action[]> = { [TERMF_EXPERT]: settingsActions };

/** What a screen holds, by name only, for the agent to act on: never a value. */
const HERE: Record<string, (project: string) => Promise<unknown>> = {
  ops: opsHere,
  tickets: ticketsHere,
  project: ticketsHere,
  ci: ciHere,
  scheduler: schedulerHere,
  vault: vaultHere,
  vaultAll: vaultHere,
  env: envHere,
  stack: stackHere,
};

/** A call of `termf mcp`, as the app gives it; `action` null: the list. */
export type Call = {
  id: number;
  session: number;
  project: string;
  role: string | null;
  title: string;
  action: string | null;
  args: Args;
  hints: Record<string, string>;
  /** Asked in a turn the user started alone: what they allowed for the conversation holds only there. */
  byUser: boolean;
};

/** How long a card waits for the user. */
const WAIT = 300_000;

/**
 * An approved call whose run stopped at the vault: locked, or prod asking for the master password again. TermF's
 * card asks for it, then runs the call again with what the user typed in the first card. `n`: this wait, not the
 * call's next one.
 */
export type VaultWait = { n: number; call: Call; secrets: Record<string, string>; need: "unlock" | "reauth" };

export const agentActions = $state({
  /** The calls whose card waits for the user, every conversation. */
  waiting: [] as Call[],
  /** The calls whose run waits for the vault, every conversation. */
  vault: [] as VaultWait[],
  /** By conversation, the changes the user allowed for the rest of it: lost when it is archived or TermF quits. */
  granted: {} as Record<number, string[]>,
});
let waits = 0;

/** The screen a conversation's user writes from: the last one a question went from, else the one it started from. */
export function screenOf(session: number): string | null {
  return ask.screens[session] ?? ask.origins[session] ?? null;
}

/** What a conversation may do: its screen's actions, its role's, those of anywhere, then every screen's reads. */
export function available(session: number): Action[] {
  const screen = screenOf(session);
  const own = (screen && SCREENS[screen]) || [];
  const role = agents.sessions.find((s) => s.id === session)?.role ?? "";
  const here = [...own, ...(ROLES[role] ?? []).filter((a) => !own.includes(a)), ...GLOBAL];
  return [...here, ...READS.filter((a) => !here.some((h) => h.id === a.id))];
}

export const actionOf = (session: number, id: unknown) => available(session).find((a) => a.id === id);

/** "Always allow in this conversation", only ever for a change. */
export function grant(session: number, action: string) {
  const mine = agentActions.granted[session] ?? [];
  if (gate(action) === "change" && !mine.includes(action)) agentActions.granted[session] = [...mine, action];
}

export function revoke(session: number, action: string) {
  agentActions.granted[session] = (agentActions.granted[session] ?? []).filter((a) => a !== action);
}

async function reply(id: number, result: unknown, error: string | null) {
  await invoke("actions_reply", { id, result: error === null ? (result ?? null) : null, error }).catch(() => {});
}

function drop(id: number) {
  agentActions.waiting = agentActions.waiting.filter((c) => c.id !== id);
  agentActions.vault = agentActions.vault.filter((w) => w.call.id !== id);
}

/** A card waits in a conversation: its panel opens, and the system tells the user when the window is elsewhere. */
async function tell(call: Call, what: Key) {
  // The CTO and the expert are talked to at full size: their card waits in Conversations.
  if (ask.origins[call.session] && !wide(call.role ?? "")) ask.open = true;
  if (document.hasFocus()) return;
  const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
  if (granted) sendNotification({ title: t("actNotify"), body: t("actAsks", { who: call.role ?? call.title, what: t(what) }) });
}

/** Runs an action the user approved: the screen updated and a toast; its result, or its error, to the agent. */
export async function perform(call: Call, secrets: Record<string, string>) {
  drop(call.id);
  const action = actionOf(call.session, call.action);
  if (!action) return reply(call.id, null, `No action ${call.action} here any more: list them again.`);
  try {
    const result = await action.run(call.args, secrets, call.project, call.session);
    await reply(call.id, result ?? { done: true }, null);
  } catch (e) {
    const need = vaultBlock(e);
    if (need) return waitVault({ n: ++waits, call, secrets, need }, action.label);
    toast(String(e), { kind: "error" });
    const next = secretMissing(e);
    await reply(call.id, null, next ? `${String(e)}. ${next}` : String(e));
  }
}

/** The run stopped at the vault: TermF's card asks the user to open it, or to confirm prod, rather than an error. */
function waitVault(wait: VaultWait, label: Key) {
  agentActions.vault = [...agentActions.vault, wait];
  setTimeout(() => {
    if (!agentActions.vault.some((w) => w.n === wait.n)) return;
    drop(wait.call.id);
    void reply(wait.call.id, null, "Nobody opened the vault in TermF within 5 minutes, so it did not run.");
  }, WAIT);
  return tell(wait.call, label);
}

/**
 * The user opened the vault, or confirmed prod (password or Touch ID), in the card: the call runs again, unless its
 * wait ended meanwhile (5 minutes, its caller gone), the agent being told it did not run.
 */
export function resume(wait: VaultWait) {
  const [kept, rest] = takeWait(agentActions.vault, wait.n);
  if (!kept) return;
  agentActions.vault = rest;
  return perform(kept.call, kept.secrets);
}

export function refuse(call: Call, why: string) {
  drop(call.id);
  const said = why.trim() ? ` They said: ${why.trim()}` : "";
  return reply(call.id, null, `The user refused this in TermF: it did not run.${said} Do not ask again unless they tell you to.`);
}

/** The card answered with the user's question: the call did not run, and nothing is decided yet. */
export function askFirst(call: Call, message: string) {
  drop(call.id);
  return reply(call.id, null, message);
}

/** What the action waits for, or why that could not be read; null when it may go to the user. */
export async function prerequisite(action: Action, args: Args, project: string): Promise<string | null> {
  try {
    return (await action.needs?.(args, project)) ?? null;
  } catch (e) {
    return String(e);
  }
}

async function answer(call: Call) {
  const here = available(call.session);
  if (call.action === null) {
    const screen = screenOf(call.session);
    const holds = screen ? HERE[screen] : undefined;
    const shown = holds ? await holds(call.project).catch((e) => `unread: ${String(e)}`) : undefined;
    return reply(call.id, { screen, ...(shown === undefined ? {} : { here: shown }), sync: syncFacts(), actions: here.map(forAgent) }, null);
  }
  const action = here.find((a) => a.id === call.action);
  if (!action) return reply(call.id, null, `No action ${call.action} here: actions lists those you may do from the user's screen.`);
  const why = refused(action, call.args, call.hints) ?? (await prerequisite(action, call.args, call.project));
  if (why) return reply(call.id, null, why);
  const granted = call.byUser ? (agentActions.granted[call.session] ?? []) : [];
  const anyway = action.cardAnyway ? await action.cardAnyway(call.args, call.project).catch(() => true) : false;
  if (!anyway && withoutCard(action, call.args, granted)) return perform(call, {});
  agentActions.waiting = [...agentActions.waiting, call];
  setTimeout(() => {
    if (!agentActions.waiting.some((c) => c.id === call.id)) return;
    drop(call.id);
    void reply(call.id, null, "Nobody approved this in TermF within 5 minutes, so it did not run.");
  }, WAIT);
  return tell(call, action.label);
}

/** Listened to from the start: an agent may act from any conversation. */
export function watchActions(): () => void {
  const stops = Promise.all([
    listen<Call>("agent-action", ({ payload }) => void answer(payload)),
    listen<Call>("agent-action-done", ({ payload }) => drop(payload.id)),
    listen<AgentSession>("agent-updated", ({ payload }) => {
      if (payload.archived) delete agentActions.granted[payload.id];
    }),
  ]);
  return () => void stops.then((list) => list.forEach((stop) => stop()));
}
