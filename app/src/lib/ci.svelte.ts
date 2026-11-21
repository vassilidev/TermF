import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { ui } from "$lib/state.svelte";
import { t } from "$lib/i18n";

export { outcome } from "$lib/ci-logic";

export type WorkflowRun = {
  /** `owner/name`. */
  repo: string;
  id: number;
  workflow: string;
  title: string;
  number: number;
  attempt: number;
  event: string;
  status: string;
  conclusion: string | null;
  branch: string | null;
  sha: string;
  actor: string | null;
  triggeredBy: string | null;
  pullRequests: number[];
  url: string;
  /** ISO 8601. */
  created: string;
  updated: string;
  started: string | null;
};
export type Step = {
  number: number;
  name: string;
  status: string;
  conclusion: string | null;
  started: string | null;
  completed: string | null;
};
export type Job = {
  id: number;
  name: string;
  status: string;
  conclusion: string | null;
  started: string | null;
  completed: string | null;
  url: string;
  runner: string | null;
  steps: Step[];
};
export type RunPage = { repo: string; runs: WorkflowRun[]; total: number; head: string | null };
export type RunDetail = { run: WorkflowRun; jobs: Job[] };
/** `errors` index `lines`; the first line shown is number `skipped + 1`. */
export type JobLog = { lines: string[]; errors: number[]; skipped: number };
export type Workflow = { id: number; name: string; path: string; state: string };
export type Input = {
  name: string;
  description: string | null;
  kind: "string" | "boolean" | "choice" | "number" | "environment";
  required: boolean;
  default: string | null;
  options: string[];
};
export type AgentMessage = { text: string; file: string; prompt: string };
export type CiNotify = "mine" | "all" | "off";

export const ci = $state({
  /** Loaded once first shown, then kept mounted, like the Library view. */
  loaded: false,
  /** The repository's path, one of the current project's. */
  repo: "",
  branch: "",
  status: "",
  pr: "",
  /** Asks the view for the dispatch form. */
  dispatch: false,
  notify: null as CiNotify | null,
});

export function showCi() {
  ui.view = "ci";
  ci.loaded = true;
}

export function dispatchWorkflow() {
  showCi();
  ci.dispatch = true;
}

/** "3 min 12 s" between two ISO dates, or until now while it runs. */
export function duration(from: string | null, to: string | null): string {
  if (!from) return "";
  const seconds = Math.max(0, Math.round(((to ? Date.parse(to) : Date.now()) - Date.parse(from)) / 1000));
  const [h, m, s] = [Math.floor(seconds / 3600), Math.floor((seconds % 3600) / 60), seconds % 60];
  if (h) return t("ciHours", { h, m });
  return m ? t("ciMinutes", { m, s }) : t("ciSeconds", { s });
}

/** Runs the whole app: notifies the CI runs that finish, and asks every minute about those followed. */
export function watchCi(): () => void {
  invoke<CiNotify>("ci_notify").then((n) => (ci.notify = n), () => {});
  const unlisten = listen<WorkflowRun>("ci-finished", async ({ payload: run }) => {
    const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
    if (!granted) return;
    sendNotification({
      title: t(run.conclusion === "success" ? "ciNotifyPassed" : "ciNotifyFailed", { workflow: run.workflow }),
      body: [run.title, run.branch, run.repo].filter(Boolean).join(" · "),
    });
  });
  const timer = setInterval(() => invoke("ci_poll").catch(() => {}), 60_000);
  return () => {
    clearInterval(timer);
    unlisten.then((stop) => stop());
  };
}
