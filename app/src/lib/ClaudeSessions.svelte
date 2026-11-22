<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleStop from "@lucide/svelte/icons/circle-stop";
  import CopyPlus from "@lucide/svelte/icons/copy-plus";
  import History from "@lucide/svelte/icons/history";
  import Play from "@lucide/svelte/icons/play";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import StatusPill, { type Status } from "$lib/ui/StatusPill.svelte";
  import { ago, t } from "$lib/i18n";
  import type { Session } from "$lib/state.svelte";
  import { claudeView, size } from "$lib/claude.svelte";
  import { attempt, openTile, repoArg, type Background, type BackgroundSession, type SavedSession, type SessionProject } from "$lib/claude-native.svelte";

  const SHOWN = 8;
  let projects = $state<SessionProject[] | null>(null);
  let background = $state<Background | null>(null);
  let loading = $state(false);
  let busy = $state("");
  /** Projects whose every session is shown. */
  let all = $state<string[]>([]);

  let purging = $state<{ path: string; plan: string } | null>(null);
  let confirmPurge = $state(false);
  let stopping = $state<BackgroundSession | null>(null);
  let confirmStop = $state(false);

  $effect(() => {
    void claudeView.repo;
    load();
  });

  async function load() {
    loading = true;
    const repo = repoArg();
    const [p, b] = await Promise.all([
      attempt(() => invoke<SessionProject[]>("claude_sessions", { repo })),
      attempt(() => invoke<Background>("claude_background")),
    ]);
    if (repo === repoArg()) {
      projects = p ?? [];
      background = b ?? null;
    }
    loading = false;
  }

  async function open(p: SessionProject, s: SavedSession, fork: boolean) {
    busy = `${fork ? "fork" : "open"}:${s.id}`;
    const tile = await attempt(() => invoke<Session>("claude_session_open", { id: s.id, cwd: p.cwd, fork }));
    if (tile) await openTile(tile);
    busy = "";
  }

  async function askPurge(p: SessionProject) {
    if (!p.cwd) return;
    busy = `purge:${p.folder}`;
    const plan = await attempt(() => invoke<string>("claude_purge_preview", { path: p.cwd }));
    busy = "";
    if (plan !== undefined) {
      purging = { path: p.cwd, plan };
      confirmPurge = true;
    }
  }

  async function purge() {
    if (!purging) return;
    const { path } = purging;
    busy = "purging";
    const done = await attempt(() => invoke<string>("claude_purge", { path }));
    busy = "";
    if (done !== undefined) {
      claudeView.done = t("claudeDgPurged", { path });
      await load();
    }
  }

  async function attach(b: BackgroundSession) {
    const id = b.id ?? "";
    const tile = await attempt(() => invoke<Session>("claude_background_attach", { id }));
    if (tile) await openTile(tile);
  }

  async function stop() {
    const id = stopping?.id;
    if (!id) return;
    const done = await attempt(() => invoke<string>("claude_background_stop", { id }));
    if (done !== undefined) {
      claudeView.done = t("claudeDgStopped", { name: stopping?.name ?? id });
      await load();
    }
  }

  const where = (p: SessionProject) => p.cwd ?? p.folder;
  /** `claude agents --json` states, as the app shows a status. */
  const pill: Record<string, Status> = { working: "working", blocked: "waiting", done: "done", failed: "failed", stopped: "stopped" };
</script>

<header class="page-head">
  <div class="titles">
    <h2>{t("claudeDgSessions")}</h2>
    <p class="lead">{t("claudeDgSessionsHelp")}</p>
  </div>
  <button class="btn sm quiet" onclick={load} disabled={loading}>
    <RefreshCw size={14} aria-hidden="true" class={loading ? "spin" : ""} />{t("claudeViewRefresh")}
  </button>
</header>

{#if background && (background.sessions.length || background.error)}
  <section class="block" aria-labelledby="claude-background">
    <h3 id="claude-background">{t("claudeDgBackground")} <span class="count">{background.sessions.length}</span></h3>
    {#if background.error}
      <p class="notice"><CircleAlert size={14} aria-hidden="true" />{t("claudeDgBackgroundError", { error: background.error })}</p>
    {:else}
      <ul class="rows">
        {#each background.sessions as b, i (b.id ?? b.sessionId ?? i)}
          <li>
            <div class="what">
              <span class="name">{b.name ?? b.id ?? "—"}</span>
              <span class="mono desc">{b.cwd ?? ""}</span>
            </div>
            {#if b.waitingFor}<span class="muted small">{b.waitingFor}</span>{/if}
            {#if b.state && b.state in pill}<StatusPill status={pill[b.state]} />{/if}
            {#if b.startedAt}<span class="muted small when">{ago(Math.floor(b.startedAt / 1000))}</span>{/if}
            {#if b.id}
              <button class="btn sm quiet" onclick={() => attach(b)}><SquareTerminal size={13} aria-hidden="true" />{t("claudeDgAttach")}</button>
              {#if b.state === "working" || b.state === "blocked"}
                <button
                  class="icon-btn sm danger"
                  onclick={() => {
                    stopping = b;
                    confirmStop = true;
                  }}
                  aria-label={t("claudeDgStop", { name: b.name ?? b.id })}
                  title={t("claudeDgStop", { name: b.name ?? b.id })}
                >
                  <CircleStop size={14} aria-hidden="true" />
                </button>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

{#if projects === null}
  <Skeleton shape="list" rows={5} label={t("claudeViewLoading")} />
{:else}
  {#each projects as p (p.folder)}
    {@const shown = all.includes(p.folder) ? p.sessions : p.sessions.slice(0, SHOWN)}
    <section class="block project" aria-label={where(p)}>
      <header>
        <h3 class="mono" title={where(p)}>{where(p)}</h3>
        <span class="count">{p.sessions.length}</span>
        <span class="grow"></span>
        {#if p.cwd}
          <button class="btn sm quiet" onclick={() => askPurge(p)} disabled={busy === `purge:${p.folder}`}>
            <Trash2 size={13} aria-hidden="true" />{t("claudeDgPurge")}
          </button>
        {/if}
      </header>
      <ul class="rows">
        {#each shown as s (s.id)}
          <li>
            <div class="what stacked">
              <span class="name" class:off={!s.title && !s.prompt}>{s.title ?? s.prompt ?? s.id}</span>
              {#if s.title && s.prompt}<span class="desc">{s.prompt}</span>{/if}
            </div>
            <span class="muted small when" title={new Date(s.modified * 1000).toLocaleString()}>{ago(s.modified)}</span>
            <span class="muted small weight">{size(s.size)}</span>
            <button class="btn sm quiet" onclick={() => open(p, s, false)} disabled={busy === `open:${s.id}`} title={t("claudeDgResumeHelp")}>
              <Play size={13} aria-hidden="true" />{t("claudeDgResume")}
            </button>
            <button class="icon-btn sm" onclick={() => open(p, s, true)} disabled={busy === `fork:${s.id}`} aria-label={t("claudeDgFork")} title={t("claudeDgForkHelp")}>
              <CopyPlus size={14} aria-hidden="true" />
            </button>
          </li>
        {/each}
      </ul>
      {#if p.sessions.length > SHOWN && !all.includes(p.folder)}
        <button class="btn sm quiet more" onclick={() => (all = [...all, p.folder])}>{t("claudeDgShowAll", { count: p.sessions.length })}</button>
      {/if}
    </section>
  {:else}
    <EmptyState icon={History} text={claudeView.repo ? t("claudeDgNoSessionsRepo") : t("claudeDgNoSessions")} />
  {/each}
{/if}

<ConfirmDialog bind:open={confirmPurge} title={t("claudeDgPurgeTitle", { path: purging?.path ?? "" })} confirm={t("claudeDgPurge")} danger onconfirm={purge}>
  <p>{t("claudeDgPurgeText")}</p>
  <pre class="plan">{purging?.plan}</pre>
</ConfirmDialog>

<ConfirmDialog bind:open={confirmStop} title={t("claudeDgStopTitle", { name: stopping?.name ?? stopping?.id ?? "" })} confirm={t("claudeDgStopConfirm")} danger onconfirm={stop}>
  <p>{t("claudeDgStopText")}</p>
</ConfirmDialog>

<style>
  .page-head {
    display: flex;
    align-items: flex-start;
    gap: 12px 24px;
    margin-bottom: 16px;
  }

  .titles {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0 0 4px;
    font-size: var(--text-xl);
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .lead {
    max-width: 68ch;
    margin: 0;
    color: var(--muted);
  }

  .block {
    margin-top: 24px;
  }

  .block:first-of-type {
    margin-top: 0;
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 8px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .project header {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    margin-bottom: 8px;
  }

  .project h3 {
    overflow: hidden;
    margin: 0;
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .grow {
    flex: 1;
  }

  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
    line-height: 16px;
    text-align: center;
  }

  .rows {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    list-style: none;
  }

  .rows li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 6px 8px 6px 14px;
  }

  .rows li + li {
    border-top: 1px solid var(--border);
  }

  .what {
    display: flex;
    flex: 1;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }

  .what.stacked {
    flex-direction: column;
    gap: 1px;
  }

  .name {
    max-width: 100%;
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name.off {
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    font-weight: 400;
  }

  .desc {
    max-width: 100%;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .small {
    font-size: var(--text-xs);
  }

  .when {
    width: 92px;
    text-align: right;
    white-space: nowrap;
  }

  .weight {
    width: 64px;
    text-align: right;
    white-space: nowrap;
  }





  .icon-btn.danger:hover:not(:disabled) {
    color: var(--danger);
  }

  .more {
    margin-top: 6px;
  }

  .plan {
    max-height: 220px;
    margin: 10px 0 0;
    padding: 8px 10px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    font: var(--text-xs) / 1.55 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
