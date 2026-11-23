<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CloudUpload from "@lucide/svelte/icons/cloud-upload";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import ScrollText from "@lucide/svelte/icons/scroll-text";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Wrench from "@lucide/svelte/icons/wrench";
  import Button from "$lib/ui/Button.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import List from "$lib/ui/List.svelte";
  import Row from "$lib/ui/Row.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { ago, t } from "$lib/i18n";
  import { askAI } from "$lib/ask.svelte";
  import { showRun } from "$lib/commands";
  import { openUrl, tickets } from "$lib/tickets.svelte";
  import { originIn } from "$lib/ci-logic";
  import { deployLabel, deployMark, deploys, loadDeploys, type Deployment, type Target, type WorkflowRun } from "$lib/ci.svelte";

  /** The project's deployments, every target side by side; `runs`: the CI runs they are matched with. */
  let { project, repo, slug, runs, onsetup }: { project: string; repo: string; slug: string; runs: WorkflowRun[]; onsetup: () => void } = $props();

  type Item = { key: string; target: Target; d: Deployment };
  const items = $derived.by(() => {
    const all: Item[] = [];
    for (const target of deploys.targets ?? []) {
      const found = deploys.found[target.name];
      if (Array.isArray(found)) all.push(...found.map((d) => ({ key: `${target.name}:${d.id}`, target, d })));
    }
    return all.sort((a, b) => Date.parse(b.d.created ?? "") - Date.parse(a.d.created ?? "") || 0);
  });
  const failures = $derived(
    (deploys.targets ?? []).flatMap((target) => {
      const found = deploys.found[target.name];
      return found && !Array.isArray(found) ? [{ target, error: found.error }] : [];
    }),
  );
  const loading = $derived(deploys.targets === null || (deploys.targets ?? []).some((t) => !(t.name in deploys.found)));

  let selected = $state<string | null>(null);
  const item = $derived(items.find((i) => i.key === selected) ?? items[0]);
  let log = $state<{ key: string; lines: string[] | null; error: string } | null>(null);
  let approving = $state(false);
  let busy = $state(false);

  const from = $derived(item ? originIn(item.target, repo, item.d, runs, tickets.runs) : { pr: null, run: null, here: false });

  // A token in a locked vault, or a prod one: the master password here, as in Ops, then the targets are read again.
  const vaultAsk = $derived(failures.find((f) => /the vault is locked|needs the master password again/.test(f.error)));
  const locked = $derived(/the vault is locked/.test(vaultAsk?.error ?? ""));
  let password = $state("");
  let passwordError = $state("");

  async function givePassword() {
    passwordError = "";
    try {
      if (locked) await invoke("vault_unlock", { password });
      else await invoke("vault_reauth", { password });
      password = "";
      await loadDeploys(project, true);
    } catch (e) {
      passwordError = /wrong master password/.test(String(e)) ? t("vaultWrongPassword") : String(e);
    }
  }

  async function showLog(i: Item) {
    log = { key: i.key, lines: null, error: "" };
    const mine = log;
    try {
      mine.lines = await invoke<string[]>("deploy_logs", { project, name: i.target.name, id: i.d.id });
    } catch (e) {
      mine.error = String(e);
    }
  }

  async function redeploy(i: Item) {
    busy = true;
    try {
      const said = await invoke<string>("deploy_redeploy", { project, name: i.target.name, id: i.d.id });
      toast(t("cvRedeployed", { target: i.target.name, tool: deployLabel[i.target.tool] }) + (said ? ` ${said.split("\n").pop()}` : ""));
      await loadDeploys(project, true);
    } catch (e) {
      toast(String(e), { kind: "error" });
    } finally {
      busy = false;
    }
  }

  function why(i: Item) {
    const lines = log?.key === i.key ? (log.lines ?? []) : [];
    void askAI({
      role: "devops",
      chips: [
        {
          id: `deploy:${i.key}`,
          kind: "object",
          label: `${i.target.name} · ${deployLabel[i.target.tool]}`,
          text: [`${deployLabel[i.target.tool]} deployment ${i.d.id} of ${i.target.target}: ${i.d.status}`, ...lines.slice(-60)].join("\n"),
        },
      ],
      draft: t("cvDeployWhyDraft", { target: i.target.name }),
    });
  }

  const tone = (d: Deployment) => (d.state === "passed" ? "done" : d.state === "failed" ? "failed" : d.state === "running" || d.state === "queued" ? "working" : "");
  const stateLabel = (d: Deployment) =>
    ({ passed: t("cvDeployLive"), failed: t("cvDeployFailed"), running: t("cvDeployBuilding"), queued: t("ciQueued"), stopped: t("ciStopped"), unknown: t("cvDeployLatest") })[d.state];
  const sub = (i: Item) =>
    [i.d.environment, i.d.description ?? i.d.branch, i.d.created ? ago(Date.parse(i.d.created) / 1000) : null].filter(Boolean).join(" · ");
</script>

{#if deploys.targets === null}
  <div class="pad"><Skeleton shape="list" rows={4} label={t("cvDeploysLoading")} /></div>
{:else if deploys.targetsError}
  <EmptyState>
    <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{deploys.targetsError}</p>
    <Button icon={RefreshCw} onclick={() => loadDeploys(project, true)}>{t("ciRetry")}</Button>
  </EmptyState>
{:else if !deploys.targets.length}
  <EmptyState
    icon={CloudUpload}
    title={t("cvNoTargets")}
    text={t("cvNoTargetsText")}
    action={{ label: t("cvConnectTool"), icon: Wrench, run: onsetup }}
    onask={() => askAI({ role: "devops", draft: t("cvDeploySetupDraft") })}
    askLabel={t("cvAskDevops")}
  />
{:else}
  <div class="split deploys">
    <div class="list">
      {#if vaultAsk}
        <form class="vault" onsubmit={(e) => (e.preventDefault(), givePassword())}>
          <span>{locked ? t("cvVaultLocked", { target: vaultAsk.target.name }) : t("opsPasswordReauth")}</span>
          <input class="field" type="password" autocomplete="current-password" placeholder={t("opsPassword")} aria-label={t("opsPassword")} bind:value={password} />
          {#if passwordError}<span class="bad small">{passwordError}</span>{/if}
          <Button size="sm" variant="primary" type="submit" disabled={!password}>{t("opsContinue")}</Button>
        </form>
      {/if}
      {#each failures.filter((f) => f !== vaultAsk) as f (f.target.name)}
        <p class="notice error small"><CircleAlert size={13} aria-hidden="true" /><span><b>{f.target.name} · {deployLabel[f.target.tool]}</b> {f.error}</span></p>
      {/each}
      {#if loading && !items.length}
        <Skeleton shape="list" rows={4} label={t("cvDeploysLoading")} />
      {:else if !items.length && !failures.length}
        <p class="muted small pad-s">{t("cvNoDeploys")}</p>
      {/if}
      <List label={t("cvDeploys")} {items} key={(i) => i.key} selected={item?.key ?? null} onselect={(i) => (selected = i.key)}>
        {#snippet row(i)}
          <Row title={`${i.target.name} · ${deployLabel[i.target.tool]}`} sub={sub(i)}>
            {#snippet lead()}<span class="logo" aria-hidden="true">{deployMark[i.target.tool]}</span>{/snippet}
            {#snippet trail()}<span class="badge {tone(i.d)}">{stateLabel(i.d)}</span>{/snippet}
          </Row>
        {/snippet}
      </List>
    </div>
    <section class="detail" aria-live="polite">
      {#if item}
        {@const i = item}
        <header class="head">
          <span class="logo lg" aria-hidden="true">{deployMark[i.target.tool]}</span>
          <div class="grow">
            <h2>{i.target.name} · {deployLabel[i.target.tool]}</h2>
            <p class="muted">{[i.target.target, i.d.environment, i.d.created ? ago(Date.parse(i.d.created) / 1000) : null, i.d.by].filter(Boolean).join(" · ")}</p>
          </div>
          <span class="badge {tone(i.d)}">{stateLabel(i.d)}</span>
        </header>
        {#if i.target.tool === "forge"}<p class="note">{t("cvForgeNote")}</p>{/if}
        <dl class="meta">
          {#if i.d.description}<dt>{t("cvDeployWhat")}</dt><dd>{i.d.description}</dd>{/if}
          {#if i.d.branch || i.d.sha}
            <dt>{t("ciMetaCommit")}</dt>
            <dd><span class="mono">{i.d.sha?.slice(0, 7) ?? ""}</span>{i.d.branch ? ` · ${i.d.branch}` : ""}</dd>
          {/if}
          <dt>{t("cvDeployFrom")}</dt>
          <dd>
            {#if from.pr !== null && from.here && slug}<button class="lnk" onclick={() => openUrl(`https://github.com/${slug}/pull/${from.pr}`)}>{t("cvPr", { n: from.pr })}</button>{:else if from.pr !== null}{t("cvPr", { n: from.pr })}{/if}
            {#if from.run !== null}{from.pr !== null ? " · " : ""}<button class="lnk" onclick={() => from.run !== null && showRun(from.run)}>{t("cvTeamRun", { n: from.run })}</button>{/if}
            {#if !from.here}<span class="muted">{from.pr !== null ? " · " : ""}{t(i.target.repo ? "cvDeployOtherRepo" : "cvDeployNoRepo")}</span>{:else if from.pr === null && from.run === null}<span class="muted">{t("cvDeployNoOrigin")}</span>{/if}
          </dd>
          {#if i.d.url}<dt>{t("cvDeployUrl")}</dt><dd><button class="lnk mono" onclick={() => i.d.url && openUrl(i.d.url)}>{i.d.url.replace(/^https?:\/\//, "")}</button></dd>{/if}
        </dl>
        <div class="actions">
          <Button size="sm" icon={ScrollText} onclick={() => showLog(i)}>{t("cvDeployLog")}</Button>
          <Button size="sm" icon={RotateCw} loading={busy} onclick={() => (approving = true)}>{t("cvRedeploy")}</Button>
          {#if i.d.state === "failed"}<Button size="sm" variant="primary" icon={Sparkles} onclick={() => why(i)}>{t("aigAskWhy")}</Button>{/if}
          {#if i.d.url}<Button size="sm" variant="quiet" icon={ExternalLink} onclick={() => i.d.url && openUrl(i.d.url)}>{t("cvOpen")}</Button>{/if}
        </div>
        {#if log?.key === i.key}
          {#if log.error}
            <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{log.error}</p>
          {:else if !log.lines}
            <Skeleton shape="block" label={t("ciLogLoading")} />
          {:else}
            <pre class="log" aria-label={t("cvDeployLog")}>{log.lines.join("\n") || t("ciLogEmpty")}</pre>
          {/if}
        {/if}
        <ConfirmDialog bind:open={approving} title={t("cvRedeployConfirm", { target: i.target.name })} confirm={t("cvRedeploy")} onconfirm={() => redeploy(i)}>
          <p>{t("cvRedeployText", { tool: deployLabel[i.target.tool], target: i.target.target })}</p>
        </ConfirmDialog>
      {/if}
    </section>
  </div>
{/if}

<style>
  .pad {
    padding: 16px 24px;
  }

  .pad-s {
    padding: 8px 10px;
  }

  .deploys {
    flex: 1;
  }

  .small {
    font-size: var(--text-sm);
  }

  .vault {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0 4px 10px;
    padding: 10px 12px;
    border: 1px solid var(--waiting-line);
    border-radius: var(--radius-md);
    background: var(--waiting-soft);
    font-size: var(--text-sm);
  }

  .vault :global(.btn) {
    align-self: flex-start;
  }

  .bad {
    color: var(--danger);
  }

  .notice.small {
    margin: 0 4px 8px;
    align-items: flex-start;
  }

  .logo {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--well);
    color: var(--soft);
    font-size: 10px;
    font-weight: 700;
  }

  .logo.lg {
    width: 36px;
    height: 36px;
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
  }

  .badge {
    display: inline-flex;
    align-items: center;
    height: 20px;
    padding: 0 8px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    white-space: nowrap;
  }

  .badge.done {
    background: var(--done-soft);
    color: var(--done);
  }

  .badge.failed {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .badge.working {
    background: var(--working-soft);
    color: var(--working);
  }

  .detail {
    padding: 20px 24px 60px;
    overflow-y: auto;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .grow {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
  }

  p {
    margin: 2px 0 0;
  }

  .note {
    margin-top: 12px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .meta {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 16px;
    margin: 20px 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .lnk {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font: inherit;
    cursor: pointer;
  }

  .lnk:hover {
    text-decoration: underline;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 16px;
  }

  .log {
    max-height: 420px;
    margin: 0;
    padding: 12px 14px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--well);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    line-height: 1.55;
    white-space: pre-wrap;
  }
</style>
