<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SvelteSet } from "svelte/reactivity";
  import FileTree from "$lib/FileTree.svelte";
  import { t } from "$lib/i18n";
  import { files, openFile, type Entry } from "$lib/files.svelte";

  let { repo, dir, depth = 0 }: { repo: string; dir: string; depth?: number } = $props();

  let entries = $state<Entry[]>([]);
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
      .then((list) => (entries = list))
      .catch((e) => (files.error = String(e)));
  });

  /** A folder shows a dot when anything below it changed. */
  function mark(entry: Entry): string | undefined {
    if (!entry.dir) return files.status[entry.path];
    const prefix = entry.path + "/";
    return Object.keys(files.status).some((p) => p.startsWith(prefix)) ? "M" : undefined;
  }

  function toggle(entry: Entry) {
    if (!entry.dir) openFile(entry.path);
    else if (expanded.has(entry.path)) expanded.delete(entry.path);
    else expanded.add(entry.path);
  }
</script>

<ul>
  {#each entries as entry (entry.path)}
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
        <span class="chevron" aria-hidden="true"
          >{entry.dir ? (expanded.has(entry.path) ? "▾" : "▸") : ""}</span
        >
        <span class="name">{entry.name}</span>
        {#if status}
          <span class="status s-{status === '?' ? 'u' : status}" title={labels[status]}>
            <span aria-hidden="true">{entry.dir ? "•" : status}</span>
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

  .row {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 2px 8px;
    border: 0;
    border-radius: 4px;
    background: none;
    text-align: left;
  }

  .row:hover,
  .row.active {
    background: var(--hover);
  }

  .chevron {
    width: 10px;
    flex-shrink: 0;
    color: var(--muted);
    font-size: 10px;
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status {
    flex-shrink: 0;
    font: 600 11px/1 "JetBrains Mono", Menlo, Consolas, monospace;
  }

  .s-M {
    color: var(--modified);
  }

  .s-A,
  .s-u {
    color: var(--running);
  }

  .s-D {
    color: var(--danger);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
