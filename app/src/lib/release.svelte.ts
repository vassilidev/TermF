import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { agents } from "$lib/agents.svelte";
import { percent, working, type Chrome, type Tools } from "$lib/release-logic";
import { app } from "$lib/state.svelte";

export type Channel = "stable" | "beta";
export type UpdateState = {
  version: string;
  channel: Channel;
  check: boolean;
  checked: number | null;
  available: { version: string; notes: string | null; date: number | null } | null;
  error: string | null;
};
export type CliPath = {
  bundled: string | null;
  target: string;
  installed: boolean;
  taken: boolean;
  onPath: boolean;
  managed: "package" | "appimage" | null;
  version: string;
};

const DISMISSED = "termf.update.dismissed";

function stored(): string | null {
  try {
    return localStorage.getItem(DISMISSED);
  } catch {
    return null;
  }
}

export const updates = $state({
  state: null as UpdateState | null,
  checking: false,
  installing: false,
  /** "42 %" while the update downloads. */
  progress: "",
  /** Installs as soon as no agent works. */
  waiting: false,
  /** The banner's version the user closed; the settings still show it. */
  dismissed: stored(),
  error: "",
});

export const needs = $state({
  tools: null as Tools | null,
  checking: false,
  downloading: false,
  cli: null as CliPath | null,
  error: "",
});

export const agentsAtWork = () => working(app.sessions, agents.sessions);

export async function checkUpdates() {
  updates.checking = true;
  updates.error = "";
  try {
    updates.state = await invoke<UpdateState>("update_check", { auto: false });
  } catch (e) {
    updates.error = String(e);
  } finally {
    updates.checking = false;
  }
}

export async function setUpdates(channel: Channel, check: boolean) {
  updates.error = "";
  const before = updates.state?.channel;
  try {
    updates.state = await invoke<UpdateState>("update_settings_set", { channel, check });
  } catch (e) {
    updates.error = String(e);
    return;
  }
  // Another channel: what was found and when were forgotten, so it is checked now.
  if (before !== channel) await checkUpdates();
}

/** Downloads, installs, then TermF restarts: it only comes back on failure. */
export async function installUpdate() {
  updates.waiting = false;
  updates.installing = true;
  updates.progress = "";
  updates.error = "";
  try {
    await invoke("update_install");
  } catch (e) {
    updates.error = String(e);
  } finally {
    updates.installing = false;
  }
}

export function dismissUpdate() {
  const version = updates.state?.available?.version ?? null;
  updates.dismissed = version;
  try {
    if (version) localStorage.setItem(DISMISSED, version);
  } catch {
    // Storage off: the banner comes back at the next launch.
  }
}

export async function checkTools() {
  needs.checking = true;
  needs.error = "";
  try {
    needs.tools = await invoke<Tools>("tools_check");
  } catch (e) {
    needs.error = String(e);
  } finally {
    needs.checking = false;
  }
}

export async function downloadChrome() {
  needs.downloading = true;
  needs.error = "";
  try {
    const chrome = await invoke<Chrome>("tools_chrome_install");
    if (needs.tools) needs.tools.chrome = chrome;
  } catch (e) {
    needs.error = String(e);
  } finally {
    needs.downloading = false;
  }
}

export async function loadCli() {
  try {
    needs.cli = await invoke<CliPath>("cli_state");
  } catch (e) {
    needs.error = String(e);
  }
}

export async function setCli(on: boolean) {
  needs.error = "";
  try {
    needs.cli = await invoke<CliPath>(on ? "cli_install" : "cli_uninstall");
  } catch (e) {
    needs.error = String(e);
  }
}

const HOUR = 60 * 60 * 1000;

/** The startup check, a few seconds in so it never slows the launch, then hourly: the backend keeps it to once a day. */
export function watchUpdates(): () => void {
  const auto = () =>
    invoke<UpdateState>("update_check", { auto: true }).then(
      (s) => (updates.state = s),
      () => {},
    );
  invoke<UpdateState>("update_state").then((s) => (updates.state = s), () => {});
  const first = setTimeout(auto, 5000);
  const hourly = setInterval(auto, HOUR);
  const progress = listen<{ done: number; total: number | null }>("update-progress", ({ payload }) => {
    updates.progress = percent(payload.done, payload.total);
  });
  const stop = $effect.root(() => {
    $effect(() => {
      if (updates.waiting && agentsAtWork() === 0) installUpdate();
    });
  });
  return () => {
    clearTimeout(first);
    clearInterval(hourly);
    progress.then((off) => off());
    stop();
  };
}
