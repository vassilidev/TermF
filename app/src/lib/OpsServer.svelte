<script lang="ts">
  import Activity from "@lucide/svelte/icons/activity";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Radio from "@lucide/svelte/icons/radio";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import OpsWrite from "$lib/OpsWrite.svelte";
  import { lang, t } from "$lib/i18n";
  import { failedJobs, health, nginxStatus, type Connection, type FailedJob, type Health } from "$lib/ops-logic";
  import { followFile, runRecipe } from "$lib/ops.svelte";
  import { toast } from "$lib/ui/toast.svelte";

  /** What an SSH server shows at a glance: its health, its nginx, and the Laravel site of its folder. */
  let { project, conn, ask }: { project: string; conn: Connection; ask: (e: unknown, retry: () => Promise<void>) => boolean } = $props();

  let busy = $state("");
  let error = $state("");
  let shape = $state<Health | null>(null);
  let nginx = $state<ReturnType<typeof nginxStatus> | null>(null);
  type Tab = "about --json" | "schedule:list" | "queue:failed" | "horizon:status";
  let tab = $state<Tab | null>(null);
  let said = $state("");
  let jobs = $state<FailedJob[]>([]);
  let writing = $state<{ title: string; loses: string; recipe: string; params: Record<string, string> } | null>(null);
  let confirming = $state(false);

  async function read(key: string, recipe: string, params: Record<string, string>, then: (text: string) => void) {
    busy = key;
    error = "";
    try {
      const ran = await runRecipe(project, recipe, conn.name, params);
      if (ran.output.exit !== 0 && !ran.output.text.trim()) error = ran.output.errors || ran.output.stopped || "";
      then(ran.output.text);
    } catch (e) {
      if (!ask(e, () => read(key, recipe, params, then))) error = String(e);
    } finally {
      busy = "";
    }
  }

  const readHealth = () => read("health", "server-health", {}, (text) => (shape = health(text, Date.now())));
  const readNginx = () => read("nginx", "nginx-status", {}, (text) => (nginx = nginxStatus(text)));
  function readLaravel(command: Tab) {
    tab = command;
    void read("laravel", "laravel-read", { command }, (text) => {
      said = text;
      jobs = command === "queue:failed" ? failedJobs(text) : [];
    });
  }

  async function write(typed: string) {
    const w = writing;
    if (!w) return;
    busy = "write";
    try {
      const ran = await runRecipe(project, w.recipe, conn.name, w.params, typed);
      if (ran.output.exit === 0) toast(t("opsmoreDone", { what: w.title }));
      else error = ran.output.errors || ran.output.text;
      if (w.recipe === "laravel-write") readLaravel("queue:failed");
      if (w.recipe === "nginx-reload") void readNginx();
    } catch (e) {
      if (!ask(e, () => write(typed))) error = String(e);
    } finally {
      busy = "";
    }
  }

  function propose(title: string, loses: string, recipe: string, params: Record<string, string> = {}) {
    writing = { title, loses, recipe, params };
    confirming = true;
  }

  async function follow(file: string) {
    try {
      await followFile(project, conn.name, file);
    } catch (e) {
      if (!ask(e, () => follow(file))) error = String(e);
    }
  }

  const gb = (n: number) => new Intl.NumberFormat(lang, { style: "unit", unit: "gigabyte", maximumFractionDigits: 1 }).format(n / 1024 ** 3);
  const pct = (n: number) => Math.round(n);
  const tabs: [Tab, Parameters<typeof t>[0]][] = [
    ["about --json", "opsmoreAbout"],
    ["schedule:list", "opsmoreSchedule"],
    ["queue:failed", "opsmoreFailed"],
    ["horizon:status", "opsmoreHorizon"],
  ];
  /** `php artisan about --json`, by section; null for another output. */
  const about = $derived.by(() => {
    if (tab !== "about --json") return null;
    try {
      return Object.entries(JSON.parse(said) as Record<string, Record<string, unknown>>);
    } catch {
      return null;
    }
  });
</script>

{#snippet head(title: string, key: string, run: () => void, done: boolean)}
  <header>
    <h3>{title}</h3>
    <button class="btn sm" disabled={!!busy} onclick={run}>
      {#if busy === key}<LoaderCircle size={13} aria-hidden="true" class="spin" />{:else}<RefreshCw
          size={13}
          aria-hidden="true"
        />{/if}{done ? t("opsmoreReadAgain") : t("opsmoreRead")}
    </button>
  </header>
{/snippet}

<div class="server">
  {#if error}<p class="notice error" role="alert">{error}</p>{/if}

  <section class="panel" aria-label={t("opsmoreHealth")}>
    {@render head(t("opsmoreHealth"), "health", readHealth, !!shape)}
    {#if shape}
      <div class="tiles">
        <div class="tile">
          <span class="muted">{t("opsmoreCpu")}</span>
          <b class:warn={(shape.cpu ?? 0) >= 80}>{shape.cpu === null ? "—" : `${pct(shape.cpu)} %`}</b>
          {#if shape.load}<small class="muted">{t("opsmoreCpuText", { pct: pct(shape.cpu ?? 0), load: shape.load[0], cpus: shape.cpus ?? "?" })}</small>{/if}
        </div>
        <div class="tile">
          <span class="muted">{t("opsmoreMemory")}</span>
          {#if shape.memory}
            {@const used = (shape.memory.used / shape.memory.total) * 100}
            <b class:warn={used >= 85}>{pct(used)} %</b>
            <small class="muted">{t("opsmoreUsedOf", { used: gb(shape.memory.used), total: gb(shape.memory.total) })}</small>
          {:else}<b>—</b>{/if}
        </div>
        <div class="tile wide">
          <span class="muted">{t("opsmoreDisks")}</span>
          {#each shape.disks as d (d.mount)}
            <div class="bar" title={t("opsmoreUsedOf", { used: gb(d.used), total: gb(d.size) })}>
              <span class="mono">{d.mount}</span>
              <span class="track"><span class="fill" class:warn={d.pct >= 85} style:width="{d.pct}%"></span></span>
              <span class="num">{d.pct} %</span>
            </div>
          {/each}
        </div>
        <div class="tile wide">
          <span class="muted">{t("opsmoreCerts")}</span>
          {#each shape.certs as c (c.name)}
            <div class="row">
              <span class="mono">{c.name}</span>
              <span class="badge" class:danger={c.days < 0} class:warn={c.days >= 0 && c.days < 15}>
                {c.days < 0 ? t("opsmoreCertExpired") : t("opsmoreCertDays", { days: c.days })}
              </span>
            </div>
          {:else}<small class="muted">{t("opsmoreNone")}</small>{/each}
        </div>
      </div>
      <details>
        <summary>{t("opsmoreServices", { count: shape.services.length })} · {t("opsmorePorts")} {shape.ports.join(", ") || t("opsmoreNone")}</summary>
        <p class="mono list">{shape.services.join(" · ")}</p>
      </details>
    {/if}
  </section>

  <section class="panel" aria-label={t("opsmoreNginx")}>
    {@render head(t("opsmoreNginx"), "nginx", readNginx, !!nginx)}
    {#if nginx}
      <p class="status">
        <span class="badge" class:ok={nginx.active === "active"} class:danger={nginx.active !== "active"}>
          {nginx.active === "active" ? t("opsmoreRunning") : t("opsmoreStopped", { state: nginx.active || "?" })}
        </span>
        <span class:ok-text={nginx.ok === true} class:error-text={nginx.ok === false}>
          {nginx.ok === true ? t("opsmoreTestOk") : nginx.ok === false ? t("opsmoreTestFailed") : t("opsmoreTestUnknown")}
        </span>
      </p>
      {#each nginx.messages as m (m)}<pre class="message">{m}</pre>{/each}
      {#if nginx.sites.length}<p class="muted small">{t("opsmoreSites")} · <span class="mono">{nginx.sites.join(", ")}</span></p>{/if}
    {/if}
    <div class="actions">
      <button class="btn sm" onclick={() => follow("/var/log/nginx/access.log")}><Radio size={13} aria-hidden="true" />{t("opsmoreAccessLog")}</button>
      <button class="btn sm" onclick={() => follow("/var/log/nginx/error.log")}><Radio size={13} aria-hidden="true" />{t("opsmoreErrorLog")}</button>
      <button class="btn sm" disabled={nginx?.ok !== true || !!busy} onclick={() => propose(t("opsmoreActReload"), t("opsmoreReloadLoses"), "nginx-reload")}>
        <RefreshCw size={13} aria-hidden="true" />{t("opsmoreReload")}
      </button>
    </div>
  </section>

  <section class="panel" aria-label={t("opsmoreLaravel")}>
    <header><h3>{t("opsmoreLaravel")}</h3>{#if conn.path}<span class="mono muted small">{conn.path}</span>{/if}</header>
    {#if !conn.path}
      <p class="muted small">{t("opsmoreLaravelNoPath")}</p>
    {:else}
      <div class="actions">
        {#each tabs as [command, label] (command)}
          <button class="btn sm" class:on={tab === command} aria-pressed={tab === command} disabled={!!busy} onclick={() => readLaravel(command)}>
            {#if busy === "laravel" && tab === command}<LoaderCircle size={13} aria-hidden="true" class="spin" />{/if}{t(label)}
          </button>
        {/each}
        <button class="btn sm" onclick={() => follow(`${conn.path}/storage/logs/laravel.log`)}><Activity size={13} aria-hidden="true" />{t("opsmoreLaravelLog")}</button>
      </div>
      {#if tab === "queue:failed" && busy !== "laravel"}
        <ul class="jobs">
          {#each jobs as j (j.id)}
            <li>
              <span><b class="mono">{j.job}</b><small class="muted"> · {j.queue} · {j.at}</small><br /><small class="mono muted">{j.id}</small></span>
              <button class="btn sm" onclick={() => propose(t("opsmoreRetry"), t("opsmoreRetryLoses", { job: j.id }), "laravel-write", { command: "queue:retry {job}", job: j.id })}
                >{t("opsmoreRetry")}</button
              >
              <button class="btn sm danger" onclick={() => propose(t("opsmoreForget"), t("opsmoreForgetLoses", { job: j.id }), "laravel-write", { command: "queue:forget {job}", job: j.id })}
                >{t("opsmoreForget")}</button
              >
            </li>
          {:else}<li class="muted">{t("opsmoreNoFailed")}</li>{/each}
        </ul>
      {:else if about}
        <dl class="about">
          {#each about as [section, values] (section)}
            <dt>{section}</dt>
            <dd>{#each Object.entries(values) as [k, v] (k)}<span><small class="muted">{k}</small> {String(v)}</span>{/each}</dd>
          {/each}
        </dl>
      {:else if tab && said.trim() && busy !== "laravel"}
        <pre class="said">{said.trim()}</pre>
      {/if}
    {/if}
  </section>
</div>

<OpsWrite bind:open={confirming} title={writing?.title ?? ""} loses={writing?.loses ?? ""} env={conn.env} onconfirm={write} />

<style>
  .server {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin-top: 16px;
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  h3 {
    margin: 0;
    font-size: var(--text-md, 14px);
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 10px;
  }

  .tile {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px;
    border-radius: var(--radius-md);
    background: var(--hover);
  }

  .tile.wide {
    grid-column: span 2;
  }

  .tile b {
    font-size: 20px;
    font-variant-numeric: tabular-nums;
  }

  .warn {
    color: var(--waiting-ring, #b7791f);
  }

  .bar,
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    font-size: var(--text-sm);
  }

  .bar {
    grid-template-columns: minmax(60px, auto) minmax(0, 1fr) auto;
  }

  .track {
    height: 6px;
    border-radius: 3px;
    background: var(--border);
    overflow: hidden;
  }

  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
  }

  .fill.warn {
    background: var(--waiting-ring, #b7791f);
  }

  .num {
    font-variant-numeric: tabular-nums;
  }

  .badge.ok {
    color: var(--done);
  }

  .badge.danger,
  .error-text {
    color: var(--danger);
  }

  .ok-text {
    color: var(--done);
  }

  .status {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 0;
  }

  .message,
  .said {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .said {
    max-height: 320px;
    overflow: auto;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .btn.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .small {
    font-size: var(--text-sm);
    margin: 0;
  }

  .list {
    margin: 6px 0 0;
    font-size: var(--text-xs);
  }

  summary {
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .jobs {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .jobs li {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--text-sm);
  }

  .jobs li > span {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .about {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 12px;
    margin: 0;
    font-size: var(--text-sm);
  }

  .about dt {
    font-weight: 600;
    text-transform: capitalize;
  }

  .about dd {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin: 0;
  }
</style>
