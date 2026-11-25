import { invoke } from "@tauri-apps/api/core";
import type { Action } from "$lib/actions.svelte";
import { ci, deployLabel, deploys, loadDeploys, type Target, type Tool } from "$lib/ci.svelte";
import { t } from "$lib/i18n";
import { toast } from "$lib/ui/toast.svelte";

const TOOLS: Tool[] = ["forge", "vercel", "netlify", "fly", "railway"];
const ENVS = ["dev", "staging", "prod"];

const text = (v: unknown) => (typeof v === "string" && v.trim() ? v.trim() : null);
/** The repository the CI/CD view shows, by its folder's name. */
const shown = () => ci.repo.split(/[\\/]/).pop() ?? ci.repo;

function repoShown(): string {
  if (!ci.repo) throw new Error("The CI/CD view shows no repository: ask the user to open one.");
  return ci.repo;
}

const connectDeploy: Action = {
  id: "ci.connect_deploy",
  description:
    "Connects the project to a deploy tool, as the CI/CD view's Tools tab does, so its deployments show and can be redeployed: forge (Laravel Forge, `target` the site, `scope` the server), vercel (`target` the project, `scope` the team), netlify (`target` the site id), fly (`target` the app), railway (`target` the project, `scope` the environment, `service`). The tool's CLI uses its own sign-in unless `token` names a secret of TermF's vault.",
  fields: {
    tool: { type: "string", description: "The deploy tool.", enum: TOOLS },
    target: { type: "string", description: "The site, project or app, as the tool names it." },
    scope: { type: "string", description: "forge: the server; vercel: the team; railway: the environment." },
    service: { type: "string", description: "railway: the service." },
    name: { type: "string", description: "How TermF names this target, production by default." },
    env: { type: "string", description: "The environment it deploys, prod by default.", enum: ENVS },
    repo: { type: "string", description: "The repository's folder it deploys, by default the one the view shows." },
    token: { type: "string", description: "The name of a secret of TermF's vault holding the tool's token; empty: the CLI's own sign-in." },
  },
  required: ["tool", "target"],
  label: "actCiConnect",
  describe: (a) =>
    t("actCiConnectText", {
      name: text(a.name) ?? "production",
      tool: deployLabel[a.tool as Tool] ?? String(a.tool),
      target: [text(a.target), text(a.scope), text(a.service)].filter(Boolean).join(" · "),
      env: text(a.env) ?? "prod",
    }),
  async run(a, _secrets, project) {
    const tool = a.tool as Tool;
    const target: Target = {
      name: text(a.name) ?? "production",
      tool,
      target: text(a.target) ?? "",
      scope: tool === "forge" || tool === "vercel" || tool === "railway" ? text(a.scope) : null,
      service: tool === "railway" ? text(a.service) : null,
      repo: text(a.repo) ?? (ci.repo || null),
      token: text(a.token),
      env: text(a.env) ?? "prod",
    };
    deploys.targets = await invoke<Target[]>("deploy_target_save", { project, target, previous: null });
    toast(t("cvTargetSaved", { target: target.name, tool: deployLabel[tool] }));
    ci.tab = "deploys";
    void loadDeploys(project, true);
    return { connected: target.name, tool, note: "Its deployments show in the CI/CD view's Deployments tab." };
  },
};

const rerun: Action = {
  id: "ci.rerun",
  description: "Runs a GitHub Actions run of the repository the CI/CD view shows again: all its jobs, or only those that failed.",
  fields: {
    run: { type: "integer", description: "The run's id, as the context gives it." },
    failed: { type: "boolean", description: "Only the jobs that failed." },
  },
  required: ["run"],
  label: "actCiRerun",
  describe: (a) => t(a.failed ? "actCiRerunFailedText" : "actCiRerunText", { run: String(a.run), repo: shown() }),
  async run(a) {
    await invoke("ci_rerun", { repo: repoShown(), id: a.run, failed: a.failed === true });
    toast(t("ciRerunDone"));
    return { rerun: a.run, failed: a.failed === true };
  },
};

const dispatch: Action = {
  id: "ci.dispatch",
  description:
    "Starts a GitHub Actions workflow that takes workflow_dispatch, in the repository the CI/CD view shows, on a branch or tag (the default branch when empty), with its inputs.",
  fields: {
    workflow: { type: "string", description: "Its file name (deploy.yml), name or id." },
    ref: { type: "string", description: "A branch or tag." },
    inputs: { type: "object", description: "Input name → value, as the workflow declares them." },
  },
  required: ["workflow"],
  label: "actCiDispatch",
  describe: (a) => {
    const inputs = Object.entries((a.inputs ?? {}) as Record<string, string>).map(([k, v]) => `${k}=${v}`);
    const said = t("actCiDispatchText", { workflow: String(a.workflow), ref: text(a.ref) ?? t("actCiDefaultBranch"), repo: shown() });
    return inputs.length ? `${said}\n${inputs.join("\n")}` : said;
  },
  async run(a) {
    const workflow = String(a.workflow).trim();
    if (!workflow || workflow.startsWith("-")) throw new Error("workflow: its file name, name or id");
    await invoke("ci_dispatch", { repo: repoShown(), workflow, gitRef: text(a.ref), inputs: a.inputs ?? {} });
    toast(t("ciDispatched"));
    return { started: workflow, note: "GitHub creates its run in a few seconds." };
  },
};

const redeploy: Action = {
  id: "ci.redeploy",
  description: "Redeploys one deployment of a connected deploy target, as the Deployments tab does.",
  fields: {
    target: { type: "string", description: "The target's name in TermF (production…)." },
    deployment: { type: "string", description: "The deployment's id, as the Deployments tab lists it." },
  },
  required: ["target", "deployment"],
  label: "actCiRedeploy",
  describe: (a) => t("actCiRedeployText", { target: String(a.target), deployment: String(a.deployment) }),
  loses: (a) => t("actCiRedeployLoses", { target: String(a.target) }),
  async run(a, _secrets, project) {
    if (deploys.targetsOf !== project || !deploys.targets) await loadDeploys(project);
    const target = deploys.targets?.find((x) => x.name === a.target);
    if (!target) throw new Error(`No deploy target ${String(a.target)}: ${deploys.targets?.map((x) => x.name).join(", ") || "none connected"}.`);
    const said = await invoke<string>("deploy_redeploy", { project, name: target.name, id: a.deployment });
    toast(t("cvRedeployed", { target: target.name, tool: deployLabel[target.tool] }));
    ci.tab = "deploys";
    await loadDeploys(project, true);
    return { redeployed: a.deployment, target: target.name, said: said.split("\n").pop() };
  },
};

export const ciActions: Action[] = [connectDeploy, rerun, dispatch, redeploy];
