<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import FileText from "@lucide/svelte/icons/file-text";
  import X from "@lucide/svelte/icons/x";
  import GitDiff from "$lib/GitDiff.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import type { FileDiff } from "$lib/git.svelte";
  import { t } from "$lib/i18n";
  import type { Run } from "$lib/tickets.svelte";

  /**
   * What a ticket run changed against the branch it started from, committed or not: its files with the lines they add
   * and remove, the diff of each one in a dialog. Read again whenever `version` moves (the run or its team updated).
   */
  let { run, version }: { run: Run; version: number } = $props();

  const SHOWN = 5;
  let files = $state<FileDiff[] | null>(null);
  let failed = $state(false);
  let picked = $state(0);
  let dialog = $state<HTMLDialogElement>();
  let request = 0;

  $effect(() => {
    void version;
    const { worktree, base } = run;
    if (!worktree || !base) return;
    const mine = ++request;
    invoke<FileDiff[]>("git_diff_since", { repo: worktree, base }).then(
      (list) => mine === request && ((files = list), (failed = false)),
      () => mine === request && (failed = true),
    );
  });

  const added = $derived((files ?? []).reduce((n, f) => n + f.added, 0));
  const removed = $derived((files ?? []).reduce((n, f) => n + f.removed, 0));
  const name = (path: string) => path.split("/").slice(-2).join("/");

  function show(i: number) {
    picked = i;
    dialog?.showModal();
  }
</script>

{#if run.worktree && run.base && !failed && (files === null || files.length)}
  <section class="panel" aria-labelledby="run-changes">
    <div class="head">
      <h3 id="run-changes">{t("uiRunChanges")}</h3>
      {#if files}
        <span class="num muted">
          <span class="add">+{added}</span> <span class="del">−{removed}</span> · {files.length === 1 ? t("uiRunFileOne") : t("uiRunFiles", { count: files.length })}
        </span>
      {/if}
    </div>
    {#if files === null}
      <Skeleton rows={3} />
    {:else}
      <ul class="files">
        {#each files.slice(0, SHOWN) as f, i (f.path)}
          <li>
            <button class="file" onclick={() => show(i)} title={f.path}>
              <FileText size={13} aria-hidden="true" />
              <span class="path mono">{name(f.path)}</span>
              {#if f.added}<span class="add num">+{f.added}</span>{/if}
              {#if f.removed}<span class="del num">−{f.removed}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
      {#if files.length > SHOWN}<p class="muted more">{t("uiRunMoreFiles", { count: files.length - SHOWN })}</p>{/if}
      <button class="btn sm wide" onclick={() => show(0)}>{t("uiRunSeeDiff")}</button>
    {/if}
  </section>
{/if}

<dialog bind:this={dialog} class="diff" aria-labelledby="run-diff-title">
  {#if files?.length}
    <header>
      <h2 id="run-diff-title">{t("uiRunChanges")}</h2>
      <span class="num muted"><span class="add">+{added}</span> <span class="del">−{removed}</span></span>
      <button class="icon-btn" onclick={() => dialog?.close()} aria-label={t("close")} title={t("close")}><X size={16} aria-hidden="true" /></button>
    </header>
    <div class="split">
      <ul class="list" aria-label={t("uiRunChanges")}>
        {#each files as f, i (f.path)}
          <li>
            <button class="file" aria-current={i === picked ? "true" : undefined} onclick={() => (picked = i)} title={f.path}>
              <span class="path mono">{f.path}</span>
              <span class="add num">+{f.added}</span><span class="del num">−{f.removed}</span>
            </button>
          </li>
        {/each}
      </ul>
      <div class="view">{#key picked}<GitDiff diff={files[picked]} />{/key}</div>
    </div>
  {/if}
</dialog>

<style>
  .panel {
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    font-size: var(--text-sm);
  }

  .head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 6px;
  }

  h3 {
    margin: 0;
    font-size: var(--text-base);
    font-weight: 650;
  }

  .head .num {
    margin-left: auto;
    font-size: var(--text-xs);
  }

  .add {
    color: var(--done);
  }

  .del {
    color: var(--danger);
  }

  .files,
  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .file {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 28px;
    padding: 0 6px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
  }

  .file:hover,
  .file[aria-current] {
    background: var(--hover);
  }

  .file :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: 0.92em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .more {
    margin: 4px 6px 0;
  }

  .wide {
    width: 100%;
    margin-top: 8px;
  }

  .diff {
    width: min(1100px, calc(100vw - 64px));
    height: min(760px, calc(100vh - 64px));
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow-3, 0 20px 50px rgb(0 0 0 / 0.25));
  }

  .diff[open] {
    display: flex;
    flex-direction: column;
  }

  .diff::backdrop {
    background: rgb(0 0 0 / 0.35);
  }

  .diff header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .diff h2 {
    margin: 0;
    font-size: var(--text-base);
    font-weight: 650;
  }

  .diff header .icon-btn {
    margin-left: auto;
  }

  .split {
    display: grid;
    flex: 1;
    grid-template-columns: 280px minmax(0, 1fr);
    min-height: 0;
  }

  .list {
    padding: 8px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    font-size: var(--text-sm);
  }

  .list .num {
    font-size: var(--text-xs);
  }

  .view {
    min-width: 0;
    overflow: auto;
  }
</style>
