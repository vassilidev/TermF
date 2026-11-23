<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import FileText from "@lucide/svelte/icons/file-text";
  import Plus from "@lucide/svelte/icons/plus";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import ClaudeFileEditor from "$lib/ClaudeFileEditor.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { t } from "$lib/i18n";
  import { toast } from "$lib/ui/toast.svelte";
  import { claudeView, scopeLabel, size, stateLabel, type Overview } from "$lib/claude.svelte";
  import { attempt, fileName, refreshFiles, repoArg } from "$lib/claude-native.svelte";

  let { o }: { o: Overview } = $props();

  let open = $state<{ path: string; create: boolean } | null>(null);
  let deleting = $state<{ name: string; path: string } | null>(null);
  let confirmDelete = $state(false);

  /** Claude Code's folder for the chosen repository: its memory opens first. */
  const repoFolder = $derived(claudeView.repo.replace(/[^A-Za-z0-9]/g, "-"));
  const join = (dir: string, name: string) => `${dir}${dir.includes("\\") ? "\\" : "/"}${name}`;

  async function remove() {
    if (!deleting) return;
    const { path, name } = deleting;
    const done = await attempt(() => invoke<{ trashed: string; index: string | null }>("claude_file_delete", { path, repo: repoArg() }));
    if (done) {
      toast(done.index ? t("claudeDgMemoryForgotten", { name }) : t("claudeDgTrashed", { name, path: done.trashed }));
      if (open?.path === path) open = null;
      await refreshFiles();
    }
  }
</script>

{#if open}
  <ClaudeFileEditor path={open.path} create={open.create} onclose={() => (open = null)} />
{/if}

<h3 class="title">{t("claudeDgInstructions")}</h3>
<ul class="rows">
  {#each o.memory.files as f (f.path)}
    {@const absent = f.state === "absent"}
    <li class:current={open?.path === f.path}>
      <div class="what">
        <span class="name">{fileName(f.path)}</span>
        <span class="mono desc" title={f.path}>{f.path}</span>
      </div>
      {#if f.size !== null}<span class="muted small">{size(f.size)}</span>{/if}
      {#if f.scope}<span class="badge scope">{scopeLabel(f.scope)}</span>{/if}
      <span class="badge state-{f.state}">{stateLabel(f.state)}</span>
      {#if !absent}
        <button class="icon-btn sm" onclick={() => (open = { path: f.path, create: false })} aria-label={t("claudeDgOpen", { name: fileName(f.path) })} title={t("claudeDgOpen", { name: fileName(f.path) })}>
          <FileText size={14} aria-hidden="true" />
        </button>
      {:else if f.scope !== "managed"}
        <button class="btn sm quiet" onclick={() => (open = { path: f.path, create: true })}>
          <Plus size={13} aria-hidden="true" />{t("claudeDgCreate")}
        </button>
      {:else}
        <span class="act-space"></span>
      {/if}
    </li>
  {/each}
</ul>
{#if !claudeView.repo}<p class="muted small note">{t("claudeViewPickRepo")}</p>{/if}

<h3 class="title">{t("claudeViewAutoMemory")} <span class="count">{o.memory.auto.length}</span></h3>
<p class="muted lead">{t("claudeDgAutoHelp")}</p>
{#each o.memory.auto as m (m.path)}
  <details class="fold" open={o.memory.auto.length === 1 || m.project === repoFolder}>
    <summary>
      <ChevronRight size={14} aria-hidden="true" class="chevron" />
      <span class="fold-title mono">{m.project || m.path}</span>
      <span class="muted small">{t("claudeViewFileCount", { count: m.files.length })} · {size(m.files.reduce((s, [, b]) => s + b, 0))}</span>
    </summary>
    <ul class="entries">
      {#each m.files as [name, bytes] (name)}
        {@const path = join(m.path, name)}
        <li class:current={open?.path === path}>
          {#if name.endsWith(".md")}
            <button class="main" onclick={() => (open = { path, create: false })}>
              <span class="mono">{name}</span>
              {#if name === "MEMORY.md"}<span class="badge">{t("claudeDgIndex")}</span>{/if}
            </button>
          {:else}
            <span class="main other mono">{name}</span>
          {/if}
          <span class="muted small">{size(bytes)}</span>
          {#if name.endsWith(".md") && name !== "MEMORY.md"}
            <button
              class="icon-btn sm danger"
              onclick={() => {
                deleting = { name, path };
                confirmDelete = true;
              }}
              aria-label={t("claudeDgForget", { name })}
              title={t("claudeDgForget", { name })}
            >
              <Trash2 size={14} aria-hidden="true" />
            </button>
          {:else}
            <span class="act-space"></span>
          {/if}
        </li>
      {/each}
    </ul>
  </details>
{:else}
  <p class="muted">{t("claudeViewNone")}</p>
{/each}

<ConfirmDialog
  bind:open={confirmDelete}
  title={t("claudeDgForgetTitle", { name: deleting?.name ?? "" })}
  confirm={t("claudeDgForgetConfirm")}
  danger
  onconfirm={remove}
>
  <p>{t("claudeDgForgetText")}</p>
</ConfirmDialog>

<style>
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 22px 0 8px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .title:first-of-type {
    margin-top: 0;
  }

  .lead {
    max-width: 72ch;
    margin: -2px 0 10px;
    font-size: var(--text-sm);
  }

  .note {
    margin: 6px 0 0;
  }

  .small {
    font-size: var(--text-xs);
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

  .rows,
  .entries {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .rows li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 42px;
    padding: 6px 8px 6px 12px;
  }

  .rows li + li,
  .entries li + li {
    border-top: 1px solid var(--border);
  }

  li.current {
    background: var(--hover);
  }

  .what {
    display: flex;
    flex: 1;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }

  .name {
    font-weight: 500;
    white-space: nowrap;
  }

  .desc {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge.scope {
    min-width: 64px;
    justify-content: center;
  }

  .state-ok {
    border-color: transparent;
    background: var(--done-soft);
    color: var(--done);
  }

  .state-invalid {
    border-color: transparent;
    background: var(--danger-soft);
    color: var(--danger);
  }

  .act-space {
    width: 26px;
    flex-shrink: 0;
  }

  .fold {
    margin-top: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .fold > summary {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding: 6px 12px;
    border-radius: var(--radius-lg);
    cursor: pointer;
    list-style: none;
  }

  .fold > summary::-webkit-details-marker {
    display: none;
  }

  .fold > summary:hover {
    background: var(--hover);
  }

  .fold :global(.chevron) {
    flex-shrink: 0;
    color: var(--muted);
    transition: transform var(--dur-fast) var(--ease-out);
  }

  .fold[open] > summary :global(.chevron) {
    transform: rotate(90deg);
  }

  .fold-title {
    font-weight: 600;
  }

  .entries {
    padding: 0 8px 6px 34px;
  }

  .entries li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 36px;
  }

  .main {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 8px;
    min-width: 0;
    height: 32px;
    padding: 0 6px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
  }

  button.main:hover {
    background: var(--hover);
  }

  .main.other {
    color: var(--muted);
  }

  .icon-btn.danger:hover:not(:disabled) {
    color: var(--danger);
  }
</style>
