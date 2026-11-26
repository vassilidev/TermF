import { invoke } from "@tauri-apps/api/core";
import type { Args } from "$lib/actions-logic";
import type { Action } from "$lib/actions.svelte";
import { t } from "$lib/i18n";
import type { Connection } from "$lib/ops-logic";
import { loadOps, ops, runRecipe } from "$lib/ops.svelte";
import { toast } from "$lib/ui/toast.svelte";

/** What changes something on a server: Redis keys, nginx, a Laravel site's failed jobs. Reads go through `ops_run`
 * and the recipes `redis-read`, `server-health`, `nginx-status`, `laravel-read`. */

async function named(project: string, name: unknown, kind: Connection["kind"]): Promise<Connection> {
  if (ops.project !== project) await loadOps(project);
  const found = ops.connections.find((c) => c.name === name && c.kind === kind);
  if (!found) throw new Error(`No ${kind} connection ${String(name)} in this project.`);
  return found;
}

/** Runs a recipe that writes, the card's approval standing for the environment's name typed. */
async function written(project: string, recipe: string, conn: Connection, params: Record<string, string>, what: string) {
  const ran = await runRecipe(project, recipe, conn.name, params, conn.env);
  const ok = ran.output.exit === 0;
  toast(ok ? t("opsmoreDone", { what }) : ran.output.errors || ran.output.text, { kind: ok ? "ok" : "error" });
  return { exit: ran.output.exit, output: ran.output.text.slice(0, 2000), errors: ran.output.errors.slice(0, 2000) };
}

const REDIS: Record<string, string> = { del: "DEL {key}", expire: "EXPIRE {key} {seconds}", persist: "PERSIST {key}", flushdb: "FLUSHDB" };
const redisLine = (a: Args) =>
  REDIS[String(a.command)]?.replace("{key}", String(a.key ?? "")).replace("{seconds}", String(a.seconds ?? "")) ?? String(a.command);

const redisWrite: Action = {
  id: "ops.redis_write",
  description:
    "Changes a Redis connection of the project: deletes a key (del), sets its time to live (expire, with seconds), removes it (persist), or empties its database (flushdb). Read it first with ops_run and the redis-read recipe.",
  fields: {
    connection: { type: "string", description: "The project's Redis connection." },
    command: { type: "string", description: "What to change.", enum: Object.keys(REDIS) },
    key: { type: "string", description: "The key, but for flushdb." },
    seconds: { type: "integer", description: "expire: the time to live, in seconds." },
  },
  required: ["connection", "command"],
  label: "opsmoreActRedis",
  describe: (a) => t("opsmoreActRedisText", { command: redisLine(a), connection: String(a.connection) }),
  loses: (a) => {
    const key = String(a.key ?? "");
    const seconds = String(a.seconds ?? "");
    if (a.command === "flushdb") {
      const c = ops.connections.find((x) => x.name === a.connection);
      return t("opsmoreFlushLoses", { db: c?.database ?? "0", name: String(a.connection) });
    }
    if (a.command === "expire") return t("opsmoreExpireLoses", { key, seconds });
    if (a.command === "persist") return t("opsmorePersistLoses", { key });
    return t("opsmoreDeleteLoses", { key });
  },
  async run(a, _secrets, project) {
    const conn = await named(project, a.connection, "redis");
    const command = REDIS[String(a.command)];
    if (command.includes("{key}") && !String(a.key ?? "").trim()) throw new Error(`${a.command} needs key`);
    const params: Record<string, string> = { command, key: String(a.key ?? ""), seconds: String(a.seconds ?? "") };
    for (const k of ["key", "seconds"]) if (!command.includes(`{${k}}`)) delete params[k];
    return written(project, "redis-write", conn, params, t("opsmoreActRedisText", { command: redisLine(a), connection: conn.name }));
  },
};

const nginxReload: Action = {
  id: "ops.nginx_reload",
  description: "Reloads nginx on an SSH connection of the project, only when nginx -t passes. Read its state first with ops_run and the nginx-status recipe.",
  fields: { connection: { type: "string", description: "The project's SSH connection." } },
  required: ["connection"],
  label: "opsmoreActReload",
  describe: (a) => t("opsmoreActReloadText", { connection: String(a.connection) }),
  loses: () => t("opsmoreReloadLoses"),
  async run(a, _secrets, project) {
    const conn = await named(project, a.connection, "ssh");
    return written(project, "nginx-reload", conn, {}, t("opsmoreActReload"));
  },
};

const laravelJob: Action = {
  id: "ops.laravel_job",
  description:
    "Retries a failed job of the Laravel site of an SSH connection (its folder set), or forgets it for good. List them first with ops_run, the laravel-read recipe and queue:failed.",
  fields: {
    connection: { type: "string", description: "The project's SSH connection that has the site's folder." },
    action: { type: "string", description: "retry, or forget (erases it).", enum: ["retry", "forget"] },
    job: { type: "string", description: "The failed job's id, as queue:failed lists it." },
  },
  required: ["connection", "action", "job"],
  label: "opsmoreActJob",
  describe: (a) => t("opsmoreActJobText", { command: `queue:${String(a.action)} ${String(a.job)}`, connection: String(a.connection) }),
  loses: (a) => (a.action === "forget" ? t("opsmoreForgetLoses", { job: String(a.job) }) : t("opsmoreRetryLoses", { job: String(a.job) })),
  async run(a, _secrets, project) {
    const conn = await named(project, a.connection, "ssh");
    const command = a.action === "forget" ? "queue:forget {job}" : "queue:retry {job}";
    return written(project, "laravel-write", conn, { command, job: String(a.job) }, t(a.action === "forget" ? "opsmoreForget" : "opsmoreRetry"));
  },
};

type ForgeSite = { id: number; server_id: number; name: string; directory?: string; repository?: string; repository_branch?: string };
type ForgeFound = { server: { id: number; name: string; ip_address?: string }; sites: ForgeSite[] };
type ForgeOrg = { slug: string; name: string };
/** What testing a Forge token found (`forge_test`, `forge_account_save`). */
export type ForgeCheck =
  | { result: "ok"; organization: ForgeOrg; servers: number }
  | { result: "refused" }
  | { result: "forbidden" }
  | { result: "organization"; wanted: string | null; organizations: ForgeOrg[] }
  | { result: "unreachable"; detail: string }
  | { result: "failed"; detail: string };

/** The check in words. */
export function forgeCheckText(c: ForgeCheck): string {
  switch (c.result) {
    case "ok":
      return t("forgefixOk", { org: c.organization.name || c.organization.slug, count: c.servers });
    case "refused":
      return t("forgefixRefused");
    case "forbidden":
      return t("forgefixForbidden");
    case "organization":
      if (c.wanted) return t("forgefixNotFound", { org: c.wanted, list: c.organizations.map((o) => o.slug).join(", ") || "-" });
      return t(c.organizations.length ? "forgefixPick" : "forgefixNoOrg");
    case "unreachable":
    case "failed":
      return t(c.result === "failed" ? "forgefixFailed" : "forgefixUnreachable", { detail: c.detail });
  }
}

/** The account's sites named, by their domain. */
async function forgeSites(project: string, names: unknown[]): Promise<ForgeSite[]> {
  const found = await invoke<ForgeFound[]>("forge_sites", { project });
  const all = found.flatMap((f) => f.sites);
  return names.map((n) => {
    const site = all.find((s) => s.name === n);
    if (!site) throw new Error(`No Forge site ${String(n)} on the project's account: ${all.map((s) => s.name).join(", ") || "none"}.`);
    return site;
  });
}

const forgeAccount: Action = {
  id: "ops.forge_account",
  description:
    "Names the project's Laravel Forge account once its API token works: its label, and its token, which the user types in TermF's card and which goes to the vault. You never see the token, and never ask for it in the conversation. `cli` true takes instead the token the Forge CLI keeps on this machine (forge login), the user approving it in the card. `organization`: the organization's slug, needed when the token sees several. Nothing is saved when the test fails: you get why.",
  fields: {
    label: { type: "string", description: "The account's label, such as the client's name." },
    organization: { type: "string", description: "The Forge organization's slug, when the token sees several." },
    cli: { type: "boolean", description: "Take the Forge CLI's token on this machine instead of asking for one." },
    token: { type: "string", secret: true, description: "The Forge API token." },
  },
  required: ["label"],
  asks: (a) => (a.cli === true ? [] : ["token"]),
  label: "opsforgeActAccount",
  labels: { token: "opsforgeToken" },
  describe: (a) => t(a.cli === true ? "forgefixActAccountCliText" : "opsforgeActAccountText", { label: String(a.label) }),
  async run(a, secrets, project) {
    const check = await invoke<ForgeCheck>("forge_account_save", {
      project,
      label: String(a.label),
      token: a.cli === true ? null : secrets.token?.trim() || null,
      fromCli: a.cli === true,
      organization: typeof a.organization === "string" && a.organization.trim() ? a.organization.trim() : null,
    });
    if (check.result !== "ok") throw new Error(forgeCheckText(check));
    toast(t("opsmoreDone", { what: t("opsforgeActAccount") }));
    return { account: String(a.label).trim(), organization: check.organization.slug, servers: check.servers, token: "kept in the vault" };
  },
};

const forgeList: Action = {
  id: "ops.forge_list",
  description:
    "Lists the servers and sites of the project's Forge account, as the Forge tab's Import does: each server's name and IP, each site's domain, folder, repository and branch. Reads only. Use the domains with ops.forge_import and ops.forge_deploy.",
  fields: {},
  label: "forgefixActList",
  describe: () => t("forgefixActListText"),
  needs: async (_a, project) =>
    (await invoke("forge_account", { project })) ? null : "The project has no Forge account yet: propose ops.forge_account first.",
  async run(_a, _secrets, project) {
    const found = await invoke<ForgeFound[]>("forge_sites", { project });
    return {
      servers: found.map((f) => ({
        name: f.server.name,
        ip: f.server.ip_address ?? null,
        sites: f.sites.map((s) => ({ domain: s.name, folder: s.directory ?? null, repository: s.repository ?? null, branch: s.repository_branch ?? null })),
      })),
    };
  },
};


const forgeForget: Action = {
  id: "ops.forge_remove_account",
  description:
    "Removes the project's Laravel Forge account, as the Forge tab's Remove does: Ops no longer lists its servers and deployments; the connections and deploy targets already imported stay. `token` true also deletes its API token from the vault, unless another project's account uses it.",
  fields: { token: { type: "boolean", description: "Also delete the token from the vault; false by default." } },
  label: "uxs2ActForgeRemove",
  describe: (a) => t(a.token === true ? "uxs2ActForgeRemoveTokenText" : "uxs2ActForgeRemoveText"),
  loses: (a) => t(a.token === true ? "uxs2ForgeRemoveTokenLoses" : "uxs2ForgeRemoveLoses"),
  async run(a, _secrets, project) {
    const keptFor = await invoke<string[]>("forge_account_delete", { project, forgetToken: a.token === true });
    toast(t("uxs2ForgeRemoved"));
    return keptFor.length ? { removed: true, token: `kept: ${keptFor.join(", ")} use it too` } : { removed: true };
  },
};

const forgeImport: Action = {
  id: "ops.forge_import",
  description:
    "Imports sites of the project's Forge account: an SSH connection forge@ip per server, each site's database read in its env through that server (its password to the vault), and a deploy target per site.",
  fields: { sites: { type: "array", description: "The sites' domains, as Forge names them (acme.com)." } },
  required: ["sites"],
  label: "opsforgeActImport",
  describe: (a) => t("opsforgeActImportText", { sites: (Array.isArray(a.sites) ? a.sites : []).join(", ") }),
  async run(a, _secrets, project) {
    const sites = await forgeSites(project, Array.isArray(a.sites) ? a.sites : []);
    const done = await invoke<{ connections: string[]; targets: string[]; skipped: string[] }>("forge_import", {
      project,
      picks: sites.map((s) => ({ server: s.server_id, site: s.id })),
    });
    await loadOps(project);
    toast(t("opsforgeImported", { connections: done.connections.length, targets: done.targets.length }));
    return done;
  },
};

const forgeDeploy: Action = {
  id: "ops.forge_deploy",
  description: "Redeploys a site of the project's Forge account: Forge runs its deployment script at once.",
  fields: { site: { type: "string", description: "The site's domain, as Forge names it." } },
  required: ["site"],
  label: "opsforgeActDeploy",
  describe: (a) => t("opsforgeActDeployText", { site: String(a.site) }),
  loses: () => t("opsforgeRedeployText"),
  async run(a, _secrets, project) {
    const [site] = await forgeSites(project, [a.site]);
    await invoke("forge_deploy", { project, server: site.server_id, site: site.id });
    toast(t("opsforgeDeploying", { site: site.name }));
    return { deploying: site.name };
  },
};

export const opsMoreActions: Action[] = [redisWrite, nginxReload, laravelJob, forgeAccount, forgeForget, forgeList, forgeImport, forgeDeploy];
