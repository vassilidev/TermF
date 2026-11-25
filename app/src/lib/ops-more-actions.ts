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

export const opsMoreActions: Action[] = [redisWrite, nginxReload, laravelJob];
