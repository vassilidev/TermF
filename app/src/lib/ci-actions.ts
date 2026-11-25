import { invoke } from "@tauri-apps/api/core";
import type { Action } from "$lib/actions.svelte";
import { ci, deployLabel, deploys, loadDeploys, loadHooks, type Hook, type Target, type Tool } from "$lib/ci.svelte";
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
  needs: (_a, project) => hasTargets(project),
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

/** The project's deploy targets, without asking each tool for its deployments as the view does. */
async function targetsOf(project: string): Promise<Target[]> {
  return deploys.targetsOf === project && deploys.targets ? deploys.targets : invoke<Target[]>("deploy_targets", { project });
}

async function hasTargets(project: string): Promise<string | null> {
  return (await targetsOf(project)).length ? null : "No deploy tool is connected to this project: propose ci.connect_deploy first, then this action again.";
}

async function targetNamed(project: string, name: unknown): Promise<Target> {
  const all = await targetsOf(project);
  const found = all.find((x) => x.name === name);
  if (!found) throw new Error(`No deploy target ${String(name)}: ${all.map((x) => x.name).join(", ") || "none connected"}.`);
  return found;
}

const where = (x: Target) => [x.target, x.scope, x.service].filter(Boolean).join(" · ");

const cancel: Action = {
  id: "ci.cancel",
  description: "Cancels a GitHub Actions run of the repository the CI/CD view shows, while it runs.",
  fields: { run: { type: "integer", description: "The run's id, as the context gives it." } },
  required: ["run"],
  label: "actCiCancel",
  describe: (a) => t("actCiCancelText", { run: String(a.run), repo: shown() }),
  loses: () => t("actCiCancelLoses"),
  async run(a) {
    await invoke("ci_cancel", { repo: repoShown(), id: a.run });
    toast(t("ciCancelDone"));
    return { cancelled: a.run };
  },
};

const editDeploy: Action = {
  id: "ci.edit_deploy",
  description:
    "Changes a connected deploy target, as the Tools tab's form does: the fields given (as ci.connect_deploy names them) replace its own, `rename` gives it another name in TermF.",
  fields: {
    name: { type: "string", description: "The target's name in TermF now." },
    rename: { type: "string", description: "Its new name in TermF." },
    target: connectDeploy.fields.target,
    scope: connectDeploy.fields.scope,
    service: connectDeploy.fields.service,
    env: connectDeploy.fields.env,
    repo: connectDeploy.fields.repo,
    token: connectDeploy.fields.token,
  },
  required: ["name"],
  label: "actCiEditDeploy",
  needs: (_a, project) => hasTargets(project),
  describe: (a) =>
    `${t("actCiEditDeployText", { name: String(a.name) })}\n${Object.entries(a)
      .filter(([k, v]) => k !== "name" && v !== null && v !== undefined)
      .map(([k, v]) => `${k} → ${String(v)}`)
      .join("\n")}`,
  async run(a, _secrets, project) {
    const found = await targetNamed(project, a.name);
    // A text given replaces, an empty one clears; one not given keeps.
    const pick = (key: string, was: string | null | undefined) => (typeof a[key] === "string" ? text(a[key]) : (was ?? null));
    const target: Target = {
      ...found,
      name: text(a.rename) ?? found.name,
      target: text(a.target) ?? found.target,
      scope: pick("scope", found.scope),
      service: pick("service", found.service),
      repo: pick("repo", found.repo),
      token: pick("token", found.token),
      env: text(a.env) ?? found.env,
    };
    deploys.targets = await invoke<Target[]>("deploy_target_save", { project, target, previous: found.name });
    toast(t("cvTargetSaved", { target: target.name, tool: deployLabel[target.tool] }));
    void loadDeploys(project, true);
    return { saved: target.name };
  },
};

const deleteDeploy: Action = {
  id: "ci.delete_deploy",
  description: "Removes a connected deploy target from TermF, as the Tools tab does: nothing changes at the deploy tool itself.",
  fields: { name: { type: "string", description: "The target's name in TermF." } },
  required: ["name"],
  label: "actCiDeleteDeploy",
  needs: (_a, project) => hasTargets(project),
  describe: (a) => {
    const x = deploys.targets?.find((d) => d.name === a.name);
    return t("actCiDeleteDeployText", { name: String(a.name), tool: x ? deployLabel[x.tool] : "", target: x ? where(x) : "" });
  },
  loses: () => t("actCiDeleteDeployLoses"),
  async run(a, _secrets, project) {
    const found = await targetNamed(project, a.name);
    deploys.targets = await invoke<Target[]>("deploy_target_delete", { project, name: found.name });
    delete deploys.found[found.name];
    toast(t("cvTargetDeleted", { target: found.name }));
    return { removed: found.name };
  },
};

/** As the Webhooks tab offers them. */
const EVENTS: Record<string, string[]> = { push: ["push"], prs: ["push", "pull_request"], all: ["*"] };
const host = (url: string) => {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
};

async function hookNamed(repo: string, id: unknown): Promise<Hook> {
  if (deploys.hooksOf !== repo || !deploys.hooks) await loadHooks(repo);
  const found = deploys.hooks?.find((h) => h.id === id);
  if (!found) throw new Error(`No webhook ${String(id)} on ${shown()}: ${deploys.hooks?.map((h) => `${h.id} (${h.url})`).join(", ") || "none"}.`);
  return found;
}

const addHook: Action = {
  id: "ci.add_webhook",
  description:
    "Adds a webhook to the GitHub repository the CI/CD view shows, as its Webhooks tab does: GitHub then calls `url` on pushes (`push`), pushes and pull requests (`prs`), or every event (`all`).",
  fields: {
    url: { type: "string", description: "https://… the address GitHub calls (a deploy hook, a chat's webhook…)." },
    events: { type: "string", description: "Which events, push by default.", enum: Object.keys(EVENTS) },
  },
  required: ["url"],
  label: "actCiHookAdd",
  describe: (a) => t("actCiHookAddText", { repo: shown(), url: String(a.url), events: String(a.events ?? "push") }),
  async run(a) {
    const repo = repoShown();
    const url = String(a.url).trim();
    if (!/^https?:\/\/\S+$/.test(url)) throw new Error("url: an http(s) address");
    const made = await invoke<Hook>("ci_hook_create", { repo, url, events: EVENTS[String(a.events ?? "push")] });
    toast(t("cvHookCreated", { host: host(made.url) }));
    await loadHooks(repo);
    return { created: made.id, url: made.url };
  },
};

const deleteHook: Action = {
  id: "ci.delete_webhook",
  description: "Deletes a webhook of the GitHub repository the CI/CD view shows, as its Webhooks tab does.",
  fields: { hook: { type: "integer", description: "The webhook's id, as `here` lists it." } },
  required: ["hook"],
  label: "actCiHookDelete",
  describe: (a) => t("actCiHookDeleteText", { repo: shown(), url: deploys.hooks?.find((h) => h.id === a.hook)?.url ?? String(a.hook) }),
  loses: (a) => t("actCiHookDeleteLoses", { host: host(deploys.hooks?.find((h) => h.id === a.hook)?.url ?? ""), repo: shown() }),
  async run(a) {
    const repo = repoShown();
    const gone = await hookNamed(repo, a.hook);
    await invoke("ci_hook_delete", { repo, hook: gone.id });
    toast(t("cvHookDeleted", { host: host(gone.url) }));
    await loadHooks(repo);
    return { deleted: gone.id };
  },
};

const redeliver: Action = {
  id: "ci.redeliver",
  description: "Sends one delivery of a webhook again, as the Webhooks tab does: GitHub calls its address with the same event.",
  fields: {
    hook: { type: "integer", description: "The webhook's id." },
    delivery: { type: "integer", description: "The delivery's id, as the Webhooks tab lists it." },
  },
  required: ["hook", "delivery"],
  label: "actCiRedeliver",
  describe: (a) => t("actCiRedeliverText", { delivery: String(a.delivery), url: deploys.hooks?.find((h) => h.id === a.hook)?.url ?? String(a.hook) }),
  async run(a) {
    const repo = repoShown();
    const hook = await hookNamed(repo, a.hook);
    await invoke("ci_redeliver", { repo, hook: hook.id, delivery: a.delivery });
    toast(t("actCiRedelivered", { delivery: String(a.delivery), host: host(hook.url) }));
    return { redelivered: a.delivery, note: "GitHub records the new attempt in a few seconds." };
  },
};

/** The project's deploy targets and the shown repository's webhooks, by name and id. */
export async function ciHere(project: string) {
  const targets = await targetsOf(project);
  if (ci.repo && deploys.hooksOf !== ci.repo) await loadHooks(ci.repo);
  return {
    repo: ci.repo ? shown() : null,
    deployTargets: targets.map((x) => ({ name: x.name, tool: x.tool, target: where(x), env: x.env })),
    webhooks: deploys.hooksOf === ci.repo ? (deploys.hooks ?? []).map((h) => ({ id: h.id, url: h.url, events: h.events })) : [],
  };
}

export const ciActions: Action[] = [connectDeploy, editDeploy, deleteDeploy, rerun, cancel, dispatch, redeploy, addHook, deleteHook, redeliver];
