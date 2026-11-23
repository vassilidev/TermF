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
  /** Its file, `.github/workflows/ci.yml`. */
  path: string;
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
export type { Node } from "$lib/ci-logic";
export type Hook = { id: number; url: string; events: string[]; active: boolean; lastCode: number | null; lastStatus: string | null; updated: string };
export type Delivery = { id: number; event: string; action: string | null; code: number; status: string; redelivery: boolean; duration: number; delivered: string };
export type Tool = "forge" | "vercel" | "netlify" | "fly" | "railway";
/** `target`: the Vercel project, Netlify site id, Fly app, Railway project or Forge site; `scope`: the Vercel team, Railway environment or Forge server. */
export type Target = { name: string; tool: Tool; target: string; scope?: string | null; service?: string | null; repo?: string | null; token?: string | null; env: string };
export type Deployment = {
  id: string;
  state: "running" | "queued" | "passed" | "failed" | "stopped" | "unknown";
  status: string;
  url: string | null;
  environment: string | null;
  sha: string | null;
  branch: string | null;
  pr: number | null;
  created: string | null;
  by: string | null;
  description: string | null;
};
export type ToolInfo = { tool: Tool; path: string | null; install: string; tokenVar: string };
/** Starts the error of a webhooks listing GitHub refused for want of admin rights. */
export const NO_ADMIN = "admin rights needed";

export const ci = $state({
  /** Loaded once first shown, then kept mounted, like the Library view. */
  loaded: false,
  /** The repository's path, one of the current project's. */
  repo: "",
  /** The branch whose runs are shown; "" for the most recent one. */
  branch: "",
  tab: "runs" as "runs" | "deploys" | "hooks" | "tools",
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

/**
 * The repository's webhooks and the project's deployments, read by the CI/CD view and shown in two of its tabs and in
 * the summary under a run. null while loading; `targets` null before the project's are read.
 */
export const deploys = $state({
  hooks: null as Hook[] | null,
  hooksError: "",
  hooksOf: "",
  targets: null as Target[] | null,
  targetsOf: "",
  targetsError: "",
  /** By target name: its deployments, newest first, or why they could not be read. */
  found: {} as Record<string, Deployment[] | { error: string }>,
  tools: null as ToolInfo[] | null,
});

export async function loadHooks(repo: string) {
  if (deploys.hooksOf !== repo) [deploys.hooks, deploys.hooksError] = [null, ""];
  deploys.hooksOf = repo;
  try {
    const hooks = await invoke<Hook[]>("ci_hooks", { repo });
    if (deploys.hooksOf === repo) [deploys.hooks, deploys.hooksError] = [hooks, ""];
  } catch (e) {
    if (deploys.hooksOf === repo) [deploys.hooks, deploys.hooksError] = [[], String(e)];
  }
}

/** The project's targets, then each one's deployments, side by side. */
export async function loadDeploys(project: string) {
  if (deploys.targetsOf !== project) [deploys.targets, deploys.found] = [null, {}];
  deploys.targetsOf = project;
  deploys.targetsError = "";
  try {
    const targets = await invoke<Target[]>("deploy_targets", { project });
    if (deploys.targetsOf !== project) return;
    deploys.targets = targets;
    await Promise.all(
      targets.map(async (target) => {
        try {
          const found = await invoke<Deployment[]>("deploy_list", { project, name: target.name });
          if (deploys.targetsOf === project) deploys.found[target.name] = found;
        } catch (e) {
          if (deploys.targetsOf === project) deploys.found[target.name] = { error: String(e) };
        }
      }),
    );
  } catch (e) {
    if (deploys.targetsOf === project) [deploys.targets, deploys.targetsError] = [[], String(e)];
  }
}

export const deployLabel: Record<Tool, string> = { forge: "Laravel Forge", vercel: "Vercel", netlify: "Netlify", fly: "Fly.io", railway: "Railway" };
/** A tool's mark in its little square. */
export const deployMark: Record<Tool, string> = { forge: "F", vercel: "▲", netlify: "N", fly: "Fly", railway: "R" };

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
