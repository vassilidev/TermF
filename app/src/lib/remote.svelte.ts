import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { agents } from "$lib/agents.svelte";
import { t } from "$lib/i18n";
import { decisions, goToWait, projectWaits } from "$lib/home.svelte";
import type { Wait } from "$lib/home-logic";
import { remoteTiles, trayShown, type RemotePolicy } from "$lib/remote-logic";
import { app, type Project } from "$lib/state.svelte";
import { waitTitle } from "$lib/team.svelte";
// Each module reads the other only inside functions, so the import cycle is safe.
import { focusSession } from "$lib/commands";

export const remote = $state({ error: "" });

/** The tile whose switch opens its popover, once turned on from the tile's actions. */
export const remoteUi = $state({ open: null as number | null });

/** The tiles open to remote control, or on their way. */
export const remoteSessions = () => remoteTiles(app.sessions);

/** A tile's switch: at once when its agent is at its prompt, else at the end of its turn. */
export async function setRemote(id: number, on: boolean) {
  remote.error = "";
  try {
    await invoke("remote_set", { id, on });
  } catch (e) {
    remote.error = String(e);
  }
}

/** Every tile off now, mid-turn or not. */
export async function cutAll() {
  remote.error = "";
  try {
    await invoke<number>("remote_cut_all");
  } catch (e) {
    remote.error = String(e);
  }
}

export async function savePolicy(project: string, policy: RemotePolicy): Promise<string> {
  try {
    const projects = await invoke<Project[]>("remote_policy_save", { project, policy });
    if (app.workspace) app.workspace.projects = projects;
    return "";
  } catch (e) {
    return String(e);
  }
}

/** A wait in a native menu: what it asks, and where. */
function label(w: Wait): string {
  if (w.kind === "tile") {
    const s = app.sessions.find((x) => x.id === w.id);
    return t("ux3t1RoleWaits", { role: s?.role ?? s?.remote?.name ?? s?.title ?? "" });
  }
  if (!("run" in w)) {
    const a = agents.sessions.find((x) => x.id === w.id);
    return w.kind === "cto" ? t("ux3t1Cto") : t("ux3t1RoleWaits", { role: a?.role ?? a?.title ?? "" });
  }
  const d = decisions(null).find((x) => x.run.run === w.run && x.step === w.step);
  if (!d) return "";
  return `${d.ask ? waitTitle(d.ask) : t("teamRunsAskDeliver")} · ${d.run.target.ticket}`;
}

/** Keeps the tray icon and the dock's badge in step with what waits, and goes where a click in its menu says. */
export function watchTray(): () => void {
  let sent = "";
  const stopEffect = $effect.root(() => {
    $effect(() => {
      const shown = trayShown(
        projectWaits(null).map((w) => ({ key: w.key, label: label(w) })),
        app.sessions,
        {
          tooltip: (count) => t("remoteTrayTooltip", { count }),
          waits: (count) => t("remoteTrayWaits", { count }),
          nothing: t("remoteTrayNothing"),
          remote: (count) => t("remoteTrayRemote", { count }),
          cutAll: t("remoteCutAll"),
          quick: t("quickTerminal"),
          open: t("remoteTrayOpen"),
          awake: t("remoteTrayAwake"),
          quit: t("remoteTrayQuit"),
        },
      );
      // The menu shows the mode as it is now.
      const key = JSON.stringify([shown, app.keepAwake?.mode]);
      if (key === sent) return;
      sent = key;
      invoke("tray_update", { shown }).catch((e) => (app.error = String(e)));
    });
  });
  const stopListen = listen<string>("tray-open", ({ payload }) => {
    const [kind, ...rest] = payload.split(":");
    const key = rest.join(":");
    if (kind === "tile") return void focusSession(Number(key));
    const wait = projectWaits(null).find((w) => w.key === key);
    if (wait) goToWait(wait);
  });
  return () => {
    stopEffect();
    stopListen.then((stop) => stop());
  };
}
