<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CloudDownload from "@lucide/svelte/icons/cloud-download";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Rocket from "@lucide/svelte/icons/rocket";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import { t } from "$lib/i18n";
  import { loadOps } from "$lib/ops.svelte";
  import { toast } from "$lib/ui/toast.svelte";

  /** The project's Laravel Forge account: its token in the vault, the import of its servers and sites, and a site's
   * deployments with their log and a redeploy once approved. */
  let { project, ask }: { project: string; ask: (e: unknown, retry: () => Promise<void>) => boolean } = $props();

  type Account = { label: string; token: string };
  type Site = { id: number; server_id: number; name: string; directory?: string; repository?: string; repository_branch?: string };
  type Found = { server: { id: number; name: string; ip_address?: string }; sites: Site[] };
  type Deployment = { id: number; status: string; commit_hash?: string; commit_message?: string; commit_author?: string; started_at?: string };

  let account = $state<Account | null | undefined>(undefined);
  let editing = $state(false);
  let label = $state("");
  let token = $state("");
  let busy = $state("");
  let error = $state("");
  let found = $state<Found[] | null>(null);
  let picked = $state<string[]>([]);
  let site = $state<Site | null>(null);
  let deployments = $state<Deployment[]>([]);
  let output = $state<{ id: number; text: string } | null>(null);
  let deploying = $state(false);
  let removing = $state(false);
  let forgetToken = $state(false);

  async function step<T>(name: string, work: () => Promise<T>): Promise<T | undefined> {
    busy = name;
    error = "";
    try {
      return await work();
    } catch (e) {
      if (!ask(e, async () => void (await step(name, work)))) error = String(e);
    } finally {
      busy = "";
    }
  }

  $effect(() => {
    void step("account", async () => {
      account = await invoke<Account | null>("forge_account", { project });
      label = account?.label ?? "";
    });
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    const saved = await step("save", () => invoke<Account>("forge_account_save", { project, label, token: token || null }));
    if (!saved) return;
    account = saved;
    token = "";
    editing = false;
  }

  async function removeAccount() {
    const keptFor = await step("remove", () => invoke<string[]>("forge_account_delete", { project, forgetToken }));
    if (!keptFor) return;
    const name = account?.token ?? "";
    [account, found, picked, site, label, forgetToken] = [null, null, [], null, "", false];
    toast(keptFor.length ? t("uxs2ForgeTokenKept", { name, projects: keptFor.join(", ") }) : t("uxs2ForgeRemoved"));
  }

  const browse = () => step("sites", async () => void (found = await invoke<Found[]>("forge_sites", { project })));

  async function importSites() {
    const picks = picked.map((k) => {
      const [server, site] = k.split(":").map(Number);
      return { server, site };
    });
    const done = await step("import", () => invoke<{ connections: string[]; targets: string[]; skipped: string[] }>("forge_import", { project, picks }));
    if (!done) return;
    toast(t("opsforgeImported", { connections: done.connections.length, targets: done.targets.length }));
    if (done.skipped.length) error = done.skipped.join(" · ");
    picked = [];
    await loadOps(project);
  }

  function open(s: Site) {
    site = s;
    output = null;
    void step("deployments", async () => void (deployments = await invoke<Deployment[]>("forge_deployments", { project, server: s.server_id, site: s.id })));
  }

  const log = (d: Deployment) =>
    site && step("output", async () => void (output = { id: d.id, text: await invoke<string>("forge_output", { project, server: site!.server_id, site: site!.id, deployment: d.id }) }));

  async function redeploy() {
    if (!site) return;
    const s = site;
    await step("deploy", () => invoke("forge_deploy", { project, server: s.server_id, site: s.id }));
    if (!error) toast(t("opsforgeDeploying", { site: s.name }));
    open(s);
  }

  const key = (s: Site) => `${s.server_id}:${s.id}`;
  const tone = (status: string) => (status === "finished" ? "ok" : status === "failed" ? "failed" : "");
</script>

<div class="forge">
  {#if error}<p class="notice error" role="alert">{error}</p>{/if}

  <section class="panel" aria-label={t("opsforgeAccount")}>
    <header>
      <h3>{t("opsforgeAccount")}</h3>
      {#if account && !editing}
        <span class="buttons">
          <button class="btn sm" onclick={() => (editing = true)}>{t("opsforgeChange")}</button>
          <button class="btn sm quiet danger" disabled={!!busy} onclick={() => (removing = true)}>{t("uxs2ForgeRemove")}</button>
        </span>
      {/if}
    </header>
    {#if account === undefined}
      <LoaderCircle size={16} aria-hidden="true" class="spin" />
    {:else if account && !editing}
      <p><b>{account.label}</b> <span class="muted small">· {t("opsforgeTokenIn", { name: account.token })}</span></p>
    {:else}
      <p class="muted small">{t("opsforgeAccountHelp")}</p>
      <form class="account" onsubmit={save}>
        <label class="labelled"><span>{t("opsforgeLabel")}</span><input class="field" bind:value={label} placeholder="Acme" required /></label>
        <label class="labelled">
          <span>{t("opsforgeToken")}</span>
          <input class="field mono" type="password" autocomplete="off" bind:value={token} required={!account} placeholder={account ? "••••••••" : ""} />
        </label>
        <div class="buttons">
          {#if account}<button type="button" class="btn sm" onclick={() => (editing = false)}>{t("cancel")}</button>{/if}
          <button class="btn sm primary" disabled={!!busy || !label.trim()}>{t("opsforgeSave")}</button>
        </div>
      </form>
    {/if}
  </section>

  {#if account}
    {@const name = account.token}
    <ConfirmDialog bind:open={removing} title={t("uxs2ForgeRemoveTitle", { label: account.label })} confirm={t("uxs2ForgeRemove")} danger onconfirm={removeAccount}>
      <p>{t("uxs2ForgeRemoveLoses")}</p>
      <label class="check"><input type="checkbox" bind:checked={forgetToken} />{t("uxs2ForgeRemoveToken", { name })}</label>
    </ConfirmDialog>
    <section class="panel" aria-label={t("opsforgeImport")}>
      <header>
        <h3>{t("opsforgeImport")}</h3>
        <button class="btn sm" disabled={!!busy} onclick={browse}>
          {#if busy === "sites"}<LoaderCircle size={13} aria-hidden="true" class="spin" />{:else}<CloudDownload size={13} aria-hidden="true" />{/if}{t(
            found ? "opsmoreReadAgain" : "opsforgeList",
          )}
        </button>
      </header>
      <p class="muted small">{t("opsforgeImportHelp")}</p>
      {#if found}
        {#each found as f (f.server.id)}
          <div class="server">
            <p><b>{f.server.name}</b> <span class="mono muted small">forge@{f.server.ip_address ?? "?"}</span></p>
            <ul>
              {#each f.sites as s (s.id)}
                <li>
                  <label><input type="checkbox" value={key(s)} bind:group={picked} /> <span class="mono">{s.name}</span></label>
                  {#if s.repository}<span class="muted small">{s.repository}{s.repository_branch ? `@${s.repository_branch}` : ""}</span>{/if}
                  <button class="btn sm quiet" onclick={() => open(s)}>{t("opsforgeDeployments")}</button>
                </li>
              {/each}
            </ul>
          </div>
        {:else}
          <EmptyState icon={CloudDownload} text={t("opsforgeNone")} />
        {/each}
        <button class="btn sm primary go" disabled={!picked.length || !!busy} onclick={importSites}>
          {#if busy === "import"}<LoaderCircle size={13} aria-hidden="true" class="spin" />{/if}{t("opsforgeImportPicked", { count: picked.length })}
        </button>
      {/if}
    </section>

    {#if site}
      <section class="panel" aria-label={t("opsforgeDeployments")}>
        <header>
          <h3>{t("opsforgeDeploymentsOf", { site: site.name })}</h3>
          <button class="btn sm primary" disabled={!!busy} onclick={() => (deploying = true)}><Rocket size={13} aria-hidden="true" />{t("opsforgeRedeploy")}</button>
        </header>
        <ul class="deploys">
          {#each deployments as d (d.id)}
            <li>
              <button class="pick" onclick={() => log(d)}>
                <span class="badge {tone(d.status)}">{d.status}</span>
                <span class="mono">{d.commit_hash?.slice(0, 7) ?? ""}</span>
                <span class="msg">{d.commit_message ?? ""}</span>
                <span class="muted small">{d.commit_author ?? ""} · {d.started_at ?? ""}</span>
              </button>
            </li>
          {:else}<li class="muted small">{busy === "deployments" ? "…" : t("opsforgeNoDeployment")}</li>{/each}
        </ul>
        {#if output}<pre class="log">{output.text}</pre>{/if}
      </section>
    {/if}
  {/if}
</div>

<ConfirmDialog bind:open={deploying} title={t("opsforgeRedeployTitle", { site: site?.name ?? "" })} confirm={t("opsforgeRedeploy")} onconfirm={redeploy}>
  <p>{t("opsforgeRedeployText")}</p>
</ConfirmDialog>

<style>
  .forge {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 920px;
    padding: 16px 28px;
    overflow-y: auto;
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

  h3,
  p {
    margin: 0;
  }

  h3 {
    font-size: var(--text-md, 14px);
  }

  .small {
    font-size: var(--text-sm);
  }

  .account {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    align-items: end;
    gap: 10px;
  }

  .buttons {
    display: flex;
    gap: 6px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
  }

  .server ul,
  .deploys {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
  }

  .server li {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--text-sm);
  }

  .server li label {
    flex: 1;
  }

  .pick {
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
  }

  .pick:hover {
    background: var(--hover);
  }

  .go {
    align-self: flex-start;
  }

  .msg {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge.ok {
    color: var(--done);
  }

  .badge.failed {
    color: var(--danger);
  }

  .log {
    max-height: 360px;
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    white-space: pre-wrap;
    overflow: auto;
  }
</style>
