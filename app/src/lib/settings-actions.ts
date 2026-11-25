import { invoke } from "@tauri-apps/api/core";
import type { Action } from "$lib/actions.svelte";
import { ci, type CiNotify } from "$lib/ci.svelte";
import { t } from "$lib/i18n";
import { setUpdates, updates, type Channel, type UpdateState } from "$lib/release.svelte";
import { app, setContextSettings, setKeepAwake, type ContextSettings } from "$lib/state.svelte";
import { toast } from "$lib/ui/toast.svelte";

type Key = Parameters<typeof t>[0];

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

/** Each setting an agent may change: its name, its values and their names, and how the Settings view writes it. */
const SETTINGS: Record<string, { label: Key; values: Record<string, Key>; write: (value: string) => Promise<unknown> }> = {
  ci_notifications: {
    label: "ciNotify",
    values: { mine: "ciNotifyMine", all: "ciNotifyAll", off: "ciNotifyOff" },
    write: async (value) => (ci.notify = await invoke<CiNotify>("ci_notify_set", { notify: value })),
  },
  keep_awake: {
    label: "keepAwake",
    values: { auto: "keepAwakeAuto", always: "keepAwakeAlways", off: "keepAwakeOff" },
    write: async (value) => {
      const failed = await setKeepAwake(value as Parameters<typeof setKeepAwake>[0], app.keepAwake?.displayOn ?? false);
      if (failed) throw new Error(failed);
    },
  },
  update_channel: {
    label: "releaseChannel",
    values: { stable: "releaseStable", beta: "releaseBeta" },
    write: async (value) => setUpdate(value as Channel, (await updateState()).check),
  },
  update_check: {
    label: "releaseAuto",
    values: ONOFF,
    write: async (value) => setUpdate((await updateState()).channel, value === "on"),
  },
  tray_icon: { label: "remoteTrayIcon", values: ONOFF, write: (value) => setTray("icon", value === "on") },
  tray_badge: { label: "remoteTrayBadge", values: ONOFF, write: (value) => setTray("badge", value === "on") },
  context_reset: {
    label: "contextAuto",
    values: ONOFF,
    write: async (value) => {
      const now = app.contextSettings ?? (await invoke<ContextSettings>("context_settings"));
      const failed = await setContextSettings({ ...now, on: value === "on" });
      if (failed) throw new Error(failed);
    },
  },
};

const set: Action = {
  id: "settings.set",
  description:
    "Changes one of TermF's settings, as the Settings view does: ci_notifications (mine, all, off), keep_awake (auto: while agents work, always, off), update_channel (stable, beta), update_check (on, off: look for a new version at startup), tray_icon (on, off: the menu bar icon), tray_badge (on, off: the count on the dock icon), context_reset (on, off: the automatic reset of a nearly full context).",
  fields: {
    setting: { type: "string", description: "The setting.", enum: Object.keys(SETTINGS) },
    value: { type: "string", description: "Its new value, one of those the description lists for it." },
  },
  required: ["setting", "value"],
  label: "actSettingsSet",
  describe: (a) => {
    const s = SETTINGS[String(a.setting)];
    const value = s?.values[String(a.value)];
    return s ? t("actSettingsSetText", { setting: t(s.label), value: value ? t(value) : String(a.value) }) : "";
  },
  async run(a) {
    const s = SETTINGS[String(a.setting)];
    const value = String(a.value);
    if (!(value in s.values)) throw new Error(`${a.setting}: one of ${Object.keys(s.values).join(", ")}`);
    await s.write(value);
    toast(t("actSettingsDone", { setting: t(s.label), value: t(s.values[value]) }));
    return { setting: a.setting, value };
  },
};

export const settingsActions: Action[] = [set];
