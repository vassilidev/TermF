import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { t } from "$lib/i18n";

/** A key an agent asks for: never a value. */
export type SecretRequest = {
  id: number;
  project: string;
  variable: string;
  purpose: string;
  signupUrl: string | null;
  agent: { session: number; run: number | null; title: string; role: string | null };
  at: number;
};

/** A variable the project's agents get from the vault; `found`: null while the vault is locked. */
export type Grant = { variable: string; secret: string; scope: string; project: boolean; found: boolean | null };

export const agentEnv = $state({
  /** What the agents wait for, every project. */
  requests: [] as SecretRequest[],
  /** Grants given here: the Vault view reads its list again. */
  granted: 0,
});

/** Listened to from the start, so the Home lists them whatever view is open. */
export function watchAgentEnv(): () => void {
  const finished = new Set<number>();
  const stops = Promise.all([
    listen<SecretRequest>("agentenv-request", async ({ payload }) => {
      if (finished.has(payload.id)) return;
      agentEnv.requests = [...agentEnv.requests.filter((r) => r.id !== payload.id), payload];
      if (document.hasFocus()) return;
      const allowed = (await isPermissionGranted()) || (await requestPermission()) === "granted";
      const who = payload.agent.role ?? payload.agent.title;
      if (allowed) sendNotification({ title: t("agentenvNotify"), body: t("agentenvAsk", { who, variable: payload.variable }) });
    }),
    listen<SecretRequest>("agentenv-request-done", ({ payload }) => {
      finished.add(payload.id);
      agentEnv.requests = agentEnv.requests.filter((r) => r.id !== payload.id);
      agentEnv.granted++;
    }),
  ]);
  stops
    .then(() => invoke<SecretRequest[]>("agentenv_requests"))
    .then(
      (list) => {
        const fresh = list.filter((r) => !finished.has(r.id) && !agentEnv.requests.some((p) => p.id === r.id));
        agentEnv.requests = [...agentEnv.requests, ...fresh];
      },
      () => {},
    );
  return () => void stops.then((list) => list.forEach((stop) => stop()));
}

/** Answered here: gone from the cards at once, whatever the event says later. */
export function dropSecretRequest(id: number) {
  agentEnv.requests = agentEnv.requests.filter((r) => r.id !== id);
}
