<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleStop from "@lucide/svelte/icons/circle-stop";
  import Copy from "@lucide/svelte/icons/copy";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Images from "@lucide/svelte/icons/images";
  import Info from "@lucide/svelte/icons/info";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Share2 from "@lucide/svelte/icons/share-2";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { focusSession } from "$lib/commands";
  import { ago, lang, t } from "$lib/i18n";
  import { app, repoName, type Session } from "$lib/state.svelte";
  import { GALLERY, claudeView, copyText, openUrl, type Artifact, type Artifacts, type Failure } from "$lib/claude.svelte";

  const data = $derived(claudeView.artifacts);
  const refresh = $derived(data?.refresh);
  /** The artifact whose deletion waits for confirmation. */
  let deleting = $state<Artifact | null>(null);
  let busy = $state("");

  const failures: Record<Failure, Parameters<typeof t>[0]> = {
    toolMissing: "claudeViewFailToolMissing",
    notStarted: "claudeViewFailNotStarted",
    timeout: "claudeViewFailTimeout",
    budget: "claudeViewFailBudget",
    needsYou: "claudeViewFailNeedsYou",
    closed: "claudeViewFailClosed",
    invalid: "claudeViewFailInvalid",
    spawn: "claudeViewFailSpawn",
    cancelled: "claudeViewFailCancelled",
  };

  const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD", maximumFractionDigits: 3 });
  const cost = (usd: number | null) => (usd === null ? "" : t("claudeViewCost", { cost: money.format(usd) }));

  async function call(name: string, args: Record<string, unknown> = {}) {
    busy = name;
    try {
      claudeView.artifacts = await invoke<Artifacts>(name, args);
    } catch (e) {
      claudeView.error = String(e);
    } finally {
      busy = "";
    }
  }

  /** A session TermF opened on an artifact: its tile, focused. */
  async function session(command: string, artifact: Artifact) {
    busy = `${command}:${artifact.id}`;
    try {
      const s = await invoke<Session>(command, { url: artifact.url });
      if (!app.sessions.some((o) => o.id === s.id)) app.sessions.push(s);
      deleting = null;
      await focusSession(s.id);
    } catch (e) {
      claudeView.error = String(e);
    } finally {
      busy = "";
    }
  }

  function share(artifact: Artifact) {
    openUrl(artifact.url);
    claudeView.done = t("claudeViewShareOpened");
  }
</script>

<div class="artifacts">
  <div class="head">
    <div class="actions">
      {#if refresh?.running}
        <button class="btn sm" onclick={() => call("artifacts_cancel")} disabled={!!busy}>
          <CircleStop size={14} aria-hidden="true" />{t("claudeViewStop")}
        </button>
      {:else}
        <button class="btn sm primary" onclick={() => call("artifacts_refresh")} disabled={!!busy}>
          <RefreshCw size={14} aria-hidden="true" />{t("claudeViewArtifactsRefresh")}
        </button>
      {/if}
      <button class="btn sm" onclick={() => openUrl(GALLERY)}>
        <Images size={14} aria-hidden="true" />{t("claudeViewGallery")}
      </button>
      <label class="auto">
        <input
          type="checkbox"
          role="switch"
          class="switch"
          checked={(data?.refreshHours ?? 0) > 0}
          onchange={(e) => call("artifacts_auto", { on: e.currentTarget.checked })}
        />
        {t("claudeViewAuto")}
      </label>
    </div>

    {#if refresh?.running && refresh.revealed}
      <p class="notice wait" role="status">
        <SquareTerminal size={14} aria-hidden="true" />
        <span>{t("claudeViewNeedsYou")}</span>
        {#if refresh.session !== null}
          <button class="btn sm" onclick={() => refresh?.session != null && focusSession(refresh.session)}>{t("claudeViewShowTile")}</button>
        {/if}
      </p>
    {:else if refresh?.running}
      <p class="notice" role="status">
        <LoaderCircle size={14} aria-hidden="true" class="spin" />
        <span>{t("claudeViewListing")}{refresh.cost !== null ? ` · ${cost(refresh.cost)}` : ""}</span>
      </p>
    {:else if refresh?.last?.failure}
      <p class="notice error" role="alert">
        <CircleAlert size={14} aria-hidden="true" />
        <span>{t(failures[refresh.last.failure], { detail: refresh.last.detail ?? "" })}</span>
      </p>
    {:else if data?.listed}
      <p class="status muted">
        {t("claudeViewListed", { ago: ago(data.listed), count: refresh?.last?.count ?? data.artifacts.length })}{refresh?.last?.cost != null
          ? ` · ${cost(refresh.last.cost)}`
          : ""}
      </p>
    {/if}

    <p class="notice"><Info size={14} aria-hidden="true" /><span>{t("claudeViewArtifactsHelp")}</span></p>
  </div>

  {#if !data}
    <p class="loading muted" role="status"><LoaderCircle size={16} aria-hidden="true" class="spin" /></p>
  {:else if !data.artifacts.length}
    <div class="empty">
      <Images size={20} aria-hidden="true" />
      <h2>{t("claudeViewNoArtifacts")}</h2>
      <p>{t("claudeViewNoArtifactsHelp")}</p>
    </div>
  {:else}
    <ul class="list">
      {#each data.artifacts as a (a.id)}
        <li>
          <div class="row">
            <div class="what">
              <span class="name">{a.title ?? t("claudeViewUntitled")}</span>
              <span class="desc">
                {#if a.owner}<span class="badge">{a.owner === "me" ? t("claudeViewMine") : t("claudeViewShared")}</span>{/if}
                {#if a.updated}<span>{t("claudeViewUpdated", { when: a.updated })}</span>{/if}
                {#if a.cwd}<span>{t("claudeViewFromRepo", { repo: repoName(a.cwd), ago: ago(a.seen) })}</span>{/if}
                <span class="mono">{a.url}</span>
              </span>
            </div>
            <div class="buttons">
              <button class="icon-btn" onclick={() => openUrl(a.url)} aria-label={t("claudeViewOpen")} title={t("claudeViewOpen")}>
                <ExternalLink size={15} aria-hidden="true" />
              </button>
              <button class="icon-btn" onclick={() => copyText(a.url, t("claudeViewLinkCopied"))} aria-label={t("claudeViewCopyLink")} title={t("claudeViewCopyLink")}>
                <Copy size={15} aria-hidden="true" />
              </button>
              <button class="icon-btn" onclick={() => share(a)} aria-label={t("claudeViewShare")} title={t("claudeViewShareTitle")}>
                <Share2 size={15} aria-hidden="true" />
              </button>
              <button class="icon-btn" onclick={() => session("artifacts_edit", a)} disabled={!!busy} aria-label={t("claudeViewEditArtifact")} title={t("claudeViewEditArtifactTitle")}>
                <Pencil size={15} aria-hidden="true" />
              </button>
              <button class="icon-btn" onclick={() => (deleting = a)} disabled={!!busy} aria-label={t("claudeViewDelete")} title={t("claudeViewDelete")}>
                <Trash2 size={15} aria-hidden="true" />
              </button>
            </div>
          </div>
          {#if deleting?.id === a.id}
            <div class="confirm" role="group" aria-label={t("claudeViewDelete")}>
              <p>{t("claudeViewDeleteHow", { title: a.title ?? a.url })}</p>
              <div class="confirm-buttons">
                <button class="btn sm" onclick={() => (deleting = null)}>{t("cancel")}</button>
                <button class="btn sm danger" onclick={() => session("artifacts_delete", a)} disabled={!!busy}>
                  {t("claudeViewDeleteOpen")}
                </button>
              </div>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .artifacts {
    max-width: 1100px;
    margin: 0 auto;
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 12px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .auto {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
    font-size: var(--text-sm);
  }

  .notice {
    align-items: center;
  }

  .notice > span {
    flex: 1;
  }

  .notice.wait {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .status {
    margin: 0;
    font-size: var(--text-sm);
  }

  .loading {
    display: flex;
    justify-content: center;
    margin: 40px auto;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    max-width: 480px;
    margin: 40px auto 0;
    color: var(--muted);
    text-align: center;
  }

  .empty > :global(svg) {
    color: var(--accent-ink);
  }

  .empty h2 {
    margin: 8px 0 6px;
    color: var(--text);
    font-size: var(--text-md);
  }

  .empty p {
    margin: 0;
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    list-style: none;
  }

  .list li {
    border-bottom: 1px solid var(--border);
  }

  .list li:last-child {
    border-bottom: 0;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
  }

  .row:hover {
    background: var(--hover);
  }

  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .name {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .desc {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-xs);
    white-space: nowrap;
  }

  .mono {
    overflow: hidden;
    font-family: var(--font-mono);
    text-overflow: ellipsis;
  }

  .buttons {
    display: flex;
    gap: 2px;
  }

  .confirm {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 14px;
    margin: 0 12px 10px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--chrome);
  }

  .confirm p {
    flex: 1 1 420px;
    margin: 0;
    font-size: var(--text-sm);
  }

  .confirm-buttons {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }
</style>
