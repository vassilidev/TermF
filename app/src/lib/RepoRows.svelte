<script lang="ts" generics="T">
  import { untrack, type Snippet } from "svelte";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { chooseRepo, scopeRepos } from "$lib/files.svelte";

  /**
   * One row per repository of the project, each read on its own (an error stays on its row), and a click opens it in
   * the views. `reads`: bumped to read them again, what is shown kept meanwhile.
   */
  let { label, read, row, reads = 0 }: { label: string; read: (repo: string) => Promise<T>; row: Snippet<[T, string]>; reads?: number } = $props();

  const repos = $derived(scopeRepos());
  let found = $state<Record<string, { value: T } | { error: string }>>({});
  let request = 0;

  $effect(() => {
    void reads;
    const list = repos.map((r) => r.path);
    untrack(() => {
      const r = ++request;
      for (const repo of list) {
        read(repo).then(
          (value) => {
            if (r === request) found[repo] = { value };
          },
          (e) => {
            if (r === request) found[repo] = { error: String(e) };
          },
        );
      }
    });
  });
</script>

<ul class="rows" aria-label={label}>
  {#each repos as repo (repo.path)}
    {@const got = found[repo.path]}
    <li>
      <button class="item" title={repo.path} onclick={() => chooseRepo(repo.path)}>
        <FolderGit2 size={16} aria-hidden="true" class="lead" />
        <span class="name">{repo.name}</span>
        <span class="what">
          {#if !got}
            <Skeleton rows={1} label={repo.name} />
          {:else if "error" in got}
            <span class="error"><CircleAlert size={13} aria-hidden="true" />{got.error}</span>
          {:else}
            {@render row(got.value, repo.path)}
          {/if}
        </span>
        <ChevronRight size={14} aria-hidden="true" class="go" />
      </button>
    </li>
  {/each}
</ul>

<style>
  .rows {
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
    list-style: none;
  }

  li + li {
    border-top: 1px solid var(--border);
  }

  .item {
    display: grid;
    grid-template-columns: auto minmax(120px, 200px) minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: var(--row-lg);
    padding: 8px 14px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text);
    text-align: left;
    transition: background-color var(--dur-fast) ease;
  }

  .item:hover {
    background: var(--hover);
  }

  .item:focus-visible {
    outline-offset: -2px;
  }

  .item :global(.lead),
  .item :global(.go) {
    color: var(--muted);
  }

  .name {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .what {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 12px;
    min-width: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .what > :global(.skeleton) {
    flex: 1;
    max-width: 360px;
  }

  .error {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
    color: var(--danger);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
