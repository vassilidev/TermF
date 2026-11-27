<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SvelteSet } from "svelte/reactivity";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import FileTree from "$lib/FileTree.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { t } from "$lib/i18n";
  import { files, openFile, type Entry } from "$lib/files.svelte";

  let { repo, dir, depth = 0 }: { repo: string; dir: string; depth?: number } = $props();

  let entries = $state<Entry[] | null>(null);
  const expanded = new SvelteSet<string>();

  const labels: Record<string, string> = {
    M: t("statusModified"),
    A: t("statusAdded"),
    D: t("statusDeleted"),
    "?": t("statusUntracked"),
  };

  $effect(() => {
    void files.version;
    invoke<Entry[]>("files_list", { repo, dir })
      .then((list) => { entries = list; })
      .catch((e) => {
        files.error = String(e);
        entries ??= [];
      });
  });

  const status = $derived(files.status[repo] ?? {});

  /** A folder shows a dot when anything below it changed. */
  function mark(entry: Entry): string | undefined {
    if (!entry.dir) return status[entry.path];
    const prefix = entry.path + "/";
    return Object.keys(status).some((p) => p.startsWith(prefix)) ? "M" : undefined;
  }

  function toggle(entry: Entry) {
    if (!entry.dir) openFile(entry.path, null, repo);
    else if (expanded.has(entry.path)) expanded.delete(entry.path);
    else expanded.add(entry.path);
  }
</script>

{#if !entries}
  <div class="loading" style:padding-left="{8 + depth * 12}px"><Skeleton rows={depth ? 2 : 8} /></div>
{/if}
<ul>
  {#each entries ?? [] as entry (entry.path)}
    {@const status = mark(entry)}
    <li>
      <button
        class="row"
        class:active={files.active?.repo === repo && files.active?.path === entry.path}
        style:padding-left="{8 + depth * 12}px"
        title={entry.path}
        aria-expanded={entry.dir ? expanded.has(entry.path) : undefined}
        onclick={() => toggle(entry)}
      >
        <span class="chevron" class:open={expanded.has(entry.path)} aria-hidden="true">
          {#if entry.dir}<ChevronRight size={12} />{/if}
        </span>
        <span class="name">{entry.name}</span>
        {#if status}
          <span class="status s-{status === '?' ? 'u' : status}" title={labels[status]}>
            {#if entry.dir}<span class="dot" aria-hidden="true"></span>{:else}<span aria-hidden="true"
                >{status}</span
              >{/if}
            <span class="sr-only">{labels[status]}</span>
          </span>
        {/if}
      </button>
      {#if entry.dir && expanded.has(entry.path)}
        <FileTree {repo} dir={entry.path} depth={depth + 1} />
      {/if}
    </li>
  {/each}
</ul>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .loading {
    padding: 8px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    height: 26px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    text-align: left;
  }

  .row:hover {
    background: var(--hover);
  }

  .row.active {
    background: var(--selected);
  }

  .chevron {
    display: grid;
    place-items: center;
    width: 14px;
    flex-shrink: 0;
    color: var(--muted);
    transition: transform var(--dur-fast) var(--ease-out);
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  .dot {
    display: block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status {
    flex-shrink: 0;
    font: 600 var(--text-xs) / 1 var(--font-mono);
  }

  .s-M {
    color: var(--warning);
  }

  .s-A,
  .s-u {
    color: var(--done);
  }

  .s-D {
    color: var(--danger);
  }
</style>
