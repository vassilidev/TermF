<script lang="ts">
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import { repoOf } from "$lib/nav-logic";
  import { app } from "$lib/state.svelte";
  import { tickets } from "$lib/tickets.svelte";

  /** The repository a folder is in, by name, only in a project of several. */
  let { path }: { path: string } = $props();

  const repo = $derived(
    repoOf(path, (app.workspace?.projects ?? []).filter((p) => app.project === null || p.id === app.project), tickets.runs),
  );
</script>

{#if repo}
  <span class="repo-tag" title={repo.path}><FolderGit2 size={11} aria-hidden="true" />{repo.name}</span>
{/if}

<style>
  .repo-tag {
    display: inline-flex;
    align-items: center;
    min-width: 0;
    gap: 3px;
    max-width: 160px;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
