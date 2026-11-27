import { invoke } from "@tauri-apps/api/core";
import { claudeSettingOf } from "$lib/actions-logic";
import type { Action } from "$lib/actions.svelte";
import { ci, type CiNotify } from "$lib/ci.svelte";
import type { Effective, McpServer, Preview } from "$lib/claude.svelte";
import { isMac } from "$lib/files.svelte";
import { t } from "$lib/i18n";
import { actions as shortcuts, conflict, format, parse, refusal, setBinding } from "$lib/keymap-logic";
import { current, keymap, saveKeys } from "$lib/keymap.svelte";
import { setUpdates, updates, type Channel, type UpdateState } from "$lib/release.svelte";
import type { RemoteMode } from "$lib/remote-logic";
import { savePolicy } from "$lib/remote.svelte";
import { app, setContextSettings, setKeepAwake, type ContextSettings, type KeepAwakeState } from "$lib/state.svelte";
import { scopes as syncScopes, type SyncStatus } from "$lib/sync.svelte";
import { toast } from "$lib/ui/toast.svelte";
import { setStatus, type VaultStatus } from "$lib/vault.svelte";

type Key = Parameters<typeof t>[0];

/** The shipped role that knows TermF's settings and changes them on request, from any screen. */
export const TERMF_EXPERT = "termf-expert";

const ONOFF: Record<string, Key> = { on: "actSettingOn", off: "actSettingOff" };

async function setUpdate(channel: Channel, check: boolean) {
  await setUpdates(channel, check);
  if (updates.error) throw new Error(updates.error);
}
const updateState = async () => updates.state ?? (updates.state = await invoke<UpdateState>("update_state"));

async function setTray(key: "icon" | "badge", on: boolean) {
  const now = await invoke<{ icon: boolean; badge: boolean }>("tray_settings");
  await invoke("tray_settings_set", { settings: { ...now, [key]: on } });
}

const keepAwake = async () => app.keepAwake ?? (app.keepAwake = await invoke<KeepAwakeState>("keep_awake_state"));
async function setAwake(mode: KeepAwakeState["mode"], displayOn: boolean) {
  const failed = await setKeepAwake(mode, displayOn);
  if (failed) throw new Error(failed);
}

const context = async () => app.contextSettings ?? (app.contextSettings = await invoke<ContextSettings>("context_settings"));
async function setContext(change: Partial<ContextSettings>) {
  const failed = await setContextSettings({ ...(await context()), ...change });
  if (failed) throw new Error(failed);
}

const artifacts = () => invoke<{ refreshHours: number; deleteWithoutConfirmation: boolean }>("artifacts_list");
const vaultStatus = () => invoke<VaultStatus>("vault_status");
const onOff = (on: boolean) => (on ? "on" : "off");

function count(value: string): number {
  const n = Number(value);
  if (!Number.isInteger(n)) throw new Error(`${value}: a whole number`);
  return n;
}
const list = (value: string) => value.split(",").map((v) => v.trim()).filter(Boolean);

type Memory = { enabled: boolean; sources: { source: string; included: boolean }[] };
const memory = (project: string) => invoke<Memory>("memory_status", { project });

/**
 * Each setting an agent reads and may change: `about`, in English, is what the agent reads; `values` and their names,
 * or none for a number or a list, which `write` and TermF check. `sensitive`: it touches security or what agents may
 * do, so the user confirms each change in TermF's card, every time (`settings.set_sensitive`).
 */
type Setting = {
  label: Key;
  about: string;
  values?: Record<string, Key>;
  sensitive?: boolean;
  read: (project: string) => Promise<unknown>;
  write: (value: string, project: string) => Promise<unknown>;
};

const SETTINGS: Record<string, Setting> = {
  ci_notifications: {
    label: "ciNotify",
    about: "Which CI runs notify the user: theirs (mine), all, or none (off).",
    values: { mine: "ciNotifyMine", all: "ciNotifyAll", off: "ciNotifyOff" },
    read: async () => ci.notify ?? (await invoke<CiNotify>("ci_notify")),
    write: async (value) => (ci.notify = await invoke<CiNotify>("ci_notify_set", { notify: value })),
  },
  keep_awake: {
    label: "keepAwake",
    about: "Keeps the machine awake: while agents work (auto), always, or never (off).",
    values: { auto: "keepAwakeAuto", always: "keepAwakeAlways", off: "keepAwakeOff" },
    // Always awake keeps the session from locking.
    sensitive: true,
    read: async () => (await keepAwake()).mode,
    write: async (value) => setAwake(value as KeepAwakeState["mode"], (await keepAwake()).displayOn),
  },
  keep_display_on: {
    label: "keepDisplayOn",
    about: "While the machine is kept awake, keeps the screen on too (which also prevents locking).",
    values: ONOFF,
    sensitive: true,
    read: async () => onOff((await keepAwake()).displayOn),
    write: async (value) => setAwake((await keepAwake()).mode, value === "on"),
  },
  update_channel: {
    label: "releaseChannel",
    about: "Which versions of TermF it updates to: stable, or beta.",
    values: { stable: "releaseStable", beta: "releaseBeta" },
    read: async () => (await updateState()).channel,
    write: async (value) => setUpdate(value as Channel, (await updateState()).check),
  },
  update_check: {
    label: "releaseAuto",
    about: "Looks for a new version of TermF at startup.",
    values: ONOFF,
    read: async () => onOff((await updateState()).check),
    write: async (value) => setUpdate((await updateState()).channel, value === "on"),
  },
  tray_icon: {
    label: "remoteTrayIcon",
    about: "TermF's icon in the menu bar.",
    values: ONOFF,
    read: async () => onOff((await invoke<{ icon: boolean }>("tray_settings")).icon),
    write: (value) => setTray("icon", value === "on"),
  },
  tray_badge: {
    label: "remoteTrayBadge",
    about: "The count of what waits for the user, on the dock icon.",
    values: ONOFF,
    read: async () => onOff((await invoke<{ badge: boolean }>("tray_settings")).badge),
    write: (value) => setTray("badge", value === "on"),
  },
  context_reset: {
    label: "contextAuto",
    about: "Resets a Claude Code tile's nearly full context by itself, with a handoff.",
    values: ONOFF,
    read: async () => onOff((await context()).on),
    write: (value) => setContext({ on: value === "on" }),
  },
  context_reset_at: {
    label: "contextResetAt",
    about: "The context's fill, in percent (1 to 100), at which it is reset.",
    read: async () => (await context()).resetAt,
    write: (value) => setContext({ resetAt: count(value) }),
  },
  context_warning_at: {
    label: "contextWarningAt",
    about: "The context's fill, in percent (1 to 100), at which the user is warned.",
    read: async () => (await context()).warningAt,
    write: (value) => setContext({ warningAt: count(value) }),
  },
  artifacts_refresh: {
    label: "permSetArtifacts",
    about: "Lists the user's claude.ai artifacts again at startup, then every 6 hours, through a hidden Claude Code session.",
    values: ONOFF,
    read: async () => onOff((await artifacts()).refreshHours > 0),
    write: (value) => invoke("artifacts_auto", { on: value === "on" }),
  },
  artifacts_delete_unconfirmed: {
    label: "permSetArtifactsDelete",
    about: "Deletes a claude.ai artifact without asking the user to confirm first.",
    values: ONOFF,
    sensitive: true,
    read: async () => onOff((await artifacts()).deleteWithoutConfirmation),
    write: (value) => invoke("artifacts_confirm", { skip: value === "on" }),
  },
  vault_auto_lock: {
    label: "vaultAutoLock",
    about: "Minutes without use after which the vault locks (1 to 1440).",
    sensitive: true,
    read: async () => (await vaultStatus()).autoLockMinutes,
    write: async (value) => setStatus(await invoke<VaultStatus>("vault_auto_lock_set", { minutes: count(value) })),
  },
  vault_lock_on_leave: {
    label: "vaultCardsLockOnLeave",
    about: "Locks the vault as soon as the user leaves its view.",
    values: ONOFF,
    sensitive: true,
    read: async () => onOff((await vaultStatus()).lockOnLeave),
    write: async (value) => setStatus(await invoke<VaultStatus>("vault_lock_on_leave_set", { on: value === "on" })),
  },
  agents_environment: {
    label: "permSetAgentEnv",
    about: "The environment variables every agent TermF starts gets beyond the base ones: names or patterns (AWS_*), comma-separated; the whole list.",
    sensitive: true,
    read: async () => (await invoke<{ allow: string[] }>("agentenv_settings")).allow,
    write: (value) => invoke("agentenv_allow_set", { allow: list(value) }),
  },
  team_read_freely: {
    label: "practicalTeamReadFreely",
    about: "The project's team runs the commands that only read, and its checks and tests, without asking the user.",
    values: ONOFF,
    sensitive: true,
    read: async (project) => onOff(await invoke<boolean>("team_reads_freely", { project })),
    write: (value, project) => invoke("team_read_freely", { project, on: value === "on" }),
  },
  remote_control: {
    label: "remoteControl",
    about: "Whether the project's sessions can be driven from a phone (Remote Control): off, ask each time, or on.",
    values: { off: "remoteModeOff", ask: "remoteModeAsk", on: "remoteModeOn" },
    sensitive: true,
    read: async (project) => app.workspace?.projects.find((p) => p.id === project)?.remoteControl.default ?? "off",
    write: async (value, project) => {
      const now = app.workspace?.projects.find((p) => p.id === project)?.remoteControl ?? { default: "off", roles: {} };
      const failed = await savePolicy(project, { ...now, default: value as RemoteMode });
      if (failed) throw new Error(failed);
    },
  },
  memory: {
    label: "expertMemoryOn",
    about: "Indexes what the projects kept (conversations, runs, docs, git history) for the project's expert; off empties the index.",
    values: ONOFF,
    sensitive: true,
    read: async (project) => onOff((await memory(project)).enabled),
    write: async (value, project) => {
      const exclude = (await memory(project)).sources.filter((s) => !s.included).map((s) => s.source);
      await invoke("memory_settings_save", { project, settings: { enabled: value === "on", exclude } });
    },
  },
  sync_scopes: {
    label: "syncScopes",
    about: `What the encrypted sync carries between the user's machines, comma-separated, the whole list: ${syncScopes.map(([s]) => s).join(", ")}.`,
    sensitive: true,
    read: async () => (app.sync ?? (await invoke<SyncStatus>("sync_status"))).scopes,
    write: async (value) => {
      const known = syncScopes.map(([s]) => s as string);
      const unknown = list(value).filter((s) => !known.includes(s));
      if (unknown.length) throw new Error(`${unknown.join(", ")}: one of ${known.join(", ")}`);
      app.sync = await invoke<SyncStatus>("sync_scopes", { scopes: list(value) });
    },
  },
};

const named = (sensitive: boolean) => Object.keys(SETTINGS).filter((k) => !!SETTINGS[k].sensitive === sensitive);

/** A change of the Settings view; a sensitive one says in its card why it is asked every time. */
function setter(sensitive: boolean): Omit<Action, "id" | "label"> {
  return {
    description: sensitive
      ? "Changes one of TermF's sensitive settings (security, what agents may do): the user confirms each change in TermF's card, every time. settings.read lists each one, its value and what it does; propose the change in one sentence first."
      : "Changes one of TermF's settings, as the Settings view does. settings.read lists each one, its value and what it does.",
    fields: {
      setting: { type: "string", description: "The setting.", enum: named(sensitive) },
      value: { type: "string", description: "Its new value: one of its values, a number, or a comma-separated list, as settings.read says." },
    },
    required: ["setting", "value"],
    describe: (a) => {
      const s = SETTINGS[String(a.setting)];
      const value = s?.values?.[String(a.value)];
      if (!s) return "";
      const text = t("actSettingsSetText", { setting: t(s.label), value: value ? t(value) : String(a.value) });
      return sensitive ? `${text}\n${t("permSensitiveWhy")}` : text;
    },
    async run(a, _secrets, project) {
      const s = SETTINGS[String(a.setting)];
      const value = String(a.value).trim();
      if (s.values && !(value in s.values)) throw new Error(`${a.setting}: one of ${Object.keys(s.values).join(", ")}`);
      await s.write(value, project);
      toast(t("actSettingsDone", { setting: t(s.label), value: s.values ? t(s.values[value]) : value }));
      return { setting: a.setting, value };
    },
  };
}

const set: Action = { id: "settings.set", label: "actSettingsSet", ...setter(false) };
const setSensitive: Action = { id: "settings.set_sensitive", label: "permSetSensitive", ...setter(true) };

const keyAction = (id: unknown) => shortcuts.find((a) => a.id === id);

const shortcut: Action = {
  id: "settings.set_shortcut",
  description:
    "Changes a keyboard shortcut of TermF, as Settings > Shortcuts does: `keys` as settings.read writes them (Cmd+Shift+P, Ctrl+Alt+T), empty for none. Refused when another action already takes them: change that one first.",
  fields: {
    action: { type: "string", description: "What the shortcut does, as settings.read lists it.", enum: shortcuts.filter((a) => !a.typed).map((a) => a.id) },
    keys: { type: "string", description: "The new combination, empty for none." },
  },
  required: ["action", "keys"],
  label: "permSetShortcut",
  describe: (a) => {
    const k = keyAction(a.action);
    return k ? t("permSetShortcutText", { action: t(k.label), keys: String(a.keys ?? "").trim() || t("permShortcutNone") }) : "";
  },
  async run(a) {
    const k = keyAction(a.action)!;
    const typed = String(a.keys).trim();
    const combo = typed ? parse(typed) : null;
    if (typed && !combo) throw new Error(`${typed} is no shortcut: Cmd+Shift+P, Ctrl+Alt+T`);
    const why = combo && refusal(k, combo, isMac);
    if (why) throw new Error(t(why, { keys: typed, mods: isMac ? "⌘ / ⌃ / ⌥" : "Ctrl / Alt" }));
    const keys = combo ? format(k.digits ? { ...combo, key: "" } : combo) : "";
    const other = keys ? conflict(k.id, keys, current(), isMac) : null;
    if (other) throw new Error(`${keys} is already the shortcut of ${other}: change that one first.`);
    await saveKeys(setBinding(keymap.overrides, k.id, keys, isMac));
    toast(t("actSettingsDone", { setting: t(k.label), value: keys || t("permShortcutNone") }));
    return { action: k.id, keys };
  },
};

/** The project's repository by name (its first by default): Claude Code's project and local settings live there. */
function repoOf(project: string, name: unknown): string {
  const repos = app.workspace?.projects.find((p) => p.id === project)?.repos ?? [];
  const repo = typeof name === "string" && name.trim() ? repos.find((r) => r.name === name.trim()) : repos[0];
  if (!repo) throw new Error(`No repository ${String(name ?? "")} in this project: ${repos.map((r) => r.name).join(", ")}`);
  return repo.path;
}
const scopeField = { type: "string" as const, description: "user (every project), project (shared in the repository) or local (this machine's copy).", enum: ["user", "project", "local"] };
const repoField = { type: "string" as const, description: "For project and local: the repository's name, the project's first by default." };
const repoFor = (a: Record<string, unknown>, project: string) => (a.scope === "user" ? null : repoOf(project, a.repo));

/** A JSON value as typed by the agent; a bare word is a text. */
function json(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

const claudeSetting: Action = {
  id: "settings.claude_code",
  description:
    "Sets or removes one of Claude Code's settings (settings.json), as the Claude view does: a dotted key (model, permissions.defaultMode, env.NODE_ENV) and its value as JSON (\"acceptEdits\", true, [\"Bash(npm test)\"]), empty to remove it. Sensitive: the user confirms each change in TermF's card. settings.read with claude_code true shows which keys are set (the value of a few).",
  fields: {
    scope: scopeField,
    key: { type: "string", description: "The dotted key." },
    value: { type: "string", description: "Its value as JSON; empty removes the key." },
    repo: repoField,
  },
  required: ["scope", "key", "value"],
  label: "permClaudeSetting",
  describe: (a) => {
    const value = String(a.value ?? "").trim();
    const text = value
      ? t("permClaudeSettingText", { key: String(a.key), value, scope: String(a.scope) })
      : t("permClaudeSettingRemove", { key: String(a.key), scope: String(a.scope) });
    return `${text}\n${t("permSensitiveWhy")}`;
  },
  async run(a, _secrets, project) {
    const value = String(a.value).trim();
    const edit = { op: "set", key: String(a.key).trim(), value: value ? json(value) : null };
    const repo = repoFor(a, project);
    const preview = await invoke<Preview>("claude_setting_preview", { scope: a.scope, edit, repo });
    await invoke("claude_setting_save", { scope: a.scope, edit, hash: preview.hash, repo });
    toast(t("claudeViewSaved", { key: edit.key, path: preview.path }));
    return { saved: preview.path, diff: preview.diff };
  },
};

const claudeMcp: Action = {
  id: "settings.claude_mcp",
  description:
    "Adds or removes an MCP server of Claude Code, as the Claude view does (claude mcp add / remove). `server`, for add, as JSON: {\"type\": \"stdio\", \"command\", \"args\"?, \"env\"?} or {\"type\": \"http\" | \"sse\", \"url\", \"headers\"?}. Sensitive: the user confirms each change in TermF's card.",
  fields: {
    op: { type: "string", description: "add or remove.", enum: ["add", "remove"] },
    name: { type: "string", description: "The server's name." },
    scope: scopeField,
    server: { type: "string", description: "For add: the server, as JSON." },
    repo: repoField,
  },
  required: ["op", "name", "scope"],
  label: "permClaudeMcp",
  describe: (a) => {
    const said = { name: String(a.name), scope: String(a.scope), server: String(a.server ?? "") };
    return `${t(a.op === "add" ? "permClaudeMcpAdd" : "permClaudeMcpRemove", said)}\n${t("permSensitiveWhy")}`;
  },
  async run(a, _secrets, project) {
    const name = String(a.name).trim();
    const server = a.op === "add" ? json(String(a.server ?? "")) : undefined;
    if (a.op === "add" && (typeof server !== "object" || server === null)) throw new Error("server: a JSON object, as the description says");
    const action = a.op === "add" ? { op: "add", name, scope: a.scope, server } : { op: "remove", name, scope: a.scope };
    const done = await invoke<{ message: string }>("claude_mcp", { repo: repoFor(a, project), action });
    toast(t(a.op === "add" ? "claudeMgmtMcpAdded" : "claudeMgmtMcpRemoved", { name }));
    return { done: done.message };
  },
};

/** Every setting with its value, never a secret: what the TermF expert reads first. */
export const settingsRead: Action = {
  id: "settings.read",
  description:
    "Reads TermF's settings: each one's value, its values, what it does, and the action that changes it; the keyboard shortcuts; with `claude_code`, Claude Code's settings (the value of a few, such as model and permissions.defaultMode; the others by name only) and MCP servers by name. Never a secret's value. Reads only.",
  fields: { claude_code: { type: "boolean", description: "Also Claude Code's settings and MCP servers (slower)." } },
  label: "permSettingsRead",
  describe: () => "",
  async run(a, _secrets, project) {
    const settings = await Promise.all(
      Object.entries(SETTINGS).map(async ([setting, s]) => ({
        setting,
        value: await s.read(project).catch((e) => `unread: ${String(e)}`),
        ...(s.values ? { values: Object.keys(s.values) } : {}),
        about: s.about,
        change: s.sensitive ? "settings.set_sensitive" : "settings.set",
      })),
    );
    const bound = current();
    const keys = shortcuts.filter((k) => !k.typed).map((k) => ({ action: k.id, does: t(k.label), keys: bound[k.id] ?? "" }));
    if (a.claude_code !== true) return { settings, shortcuts: keys };
    if (!claudeRead || Date.now() - claudeRead.at > CLAUDE_READ_FOR) {
      const read = invoke<{ settings: { effective: Effective[] }; mcp: { servers: McpServer[] } }>("claude_overview", { repo: null }).then(
        (o) => ({ settings: o.settings.effective.map(claudeSettingOf), mcp: o.mcp.servers.map((m) => ({ name: m.name, scope: m.scope, status: m.status })) }),
        (e) => `unread: ${String(e)}`,
      );
      claudeRead = { at: Date.now(), read };
    }
    return { settings, shortcuts: keys, claude_code: await claudeRead.read };
  },
};

/** `claude mcp list` takes seconds: one read serves every `settings.read` of the next 30 s. */
const CLAUDE_READ_FOR = 30_000;
let claudeRead: { at: number; read: Promise<unknown> } | null = null;

/** `settings.read` is an action of anywhere. */
export const settingsActions: Action[] = [set, setSensitive, shortcut, claudeSetting, claudeMcp];
