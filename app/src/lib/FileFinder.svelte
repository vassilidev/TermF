<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { files, openFile, scopeRepos, shownRepos } from "$lib/files.svelte";

  type Found = { repo: string; path: string; rank: number };

  let dialog: HTMLDialogElement;
  let query = $state("");
  let results = $state<Found[]>([]);
  let selected = $state(0);
  // The first query of each opening walks the repository again, so new files show up.
  let fresh = true;
  let seq = 0;
  const repos = shownRepos();
  const names = new Map(scopeRepos().map((r) => [r.path, r.name]));

  onMount(() => dialog.showModal());

  $effect(() => {
    const n = ++seq;
    const asked = { query, fresh };
    Promise.allSettled(
      repos.map(async (repo) => (await invoke<string[]>("files_find", { repo, ...asked })).map((path, rank) => ({ repo, path, rank }))),
    ).then((found) => {
      if (n !== seq) return;
      // Ranked within each repository only: every repository's best matches come first.
      results = found.flatMap((f) => (f.status === "fulfilled" ? f.value : [])).sort((a, b) => a.rank - b.rank);
      selected = 0;
      const failed = found.find((f) => f.status === "rejected");
      if (failed) files.error = String(failed.reason);
    });
    fresh = false;
  });

  function pick(found: Found | undefined) {
    if (found) openFile(found.path, null, found.repo);
    files.finder = false;
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      selected = (selected + step + results.length) % Math.max(results.length, 1);
      document.getElementById(`finder-${selected}`)?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      pick(results[selected]);
    }
  }
</script>

<dialog
  bind:this={dialog}
  class="finder"
  aria-label={t("filesFind")}
  onclose={() => (files.finder = false)}
  onclick={(e) => e.target === dialog && dialog.close()}
>
  <input
    type="search"
    placeholder={t("filesFind")}
    aria-label={t("filesFind")}
    aria-controls="finder-results"
    aria-activedescendant={results.length ? `finder-${selected}` : undefined}
    bind:value={query}
    {onkeydown}
  />
  <ul id="finder-results" role="listbox" aria-label={t("filesFind")}>
    {#each results as found, i (`${found.repo}\n${found.path}`)}
      {@const slash = found.path.lastIndexOf("/")}
      <!-- svelte-ignore a11y_click_events_have_key_events (the input drives the list with the arrow keys) -->
      <li
        id="finder-{i}"
        role="option"
        aria-selected={i === selected}
        onclick={() => pick(found)}
        onmousemove={() => (selected = i)}
      >
        <span class="name">{found.path.slice(slash + 1)}</span>
        <span class="dir">{[repos.length > 1 ? names.get(found.repo) : "", found.path.slice(0, Math.max(slash, 0))].filter(Boolean).join(" · ")}</span>
      </li>
    {:else}
      <li class="none muted">{t("filesNoResults")}</li>
    {/each}
  </ul>
</dialog>

<style>
  .finder {
    width: min(640px, 90vw);
    margin: 12vh auto auto;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--shadow-3);
  }

  .finder::backdrop {
    background: var(--scrim, rgb(10 12 20 / 0.3));
  }

  input {
    width: 100%;
    height: 48px;
    padding: 0 16px;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: none;
    color: inherit;
    font-size: var(--text-md);
  }

  input:focus-visible {
    outline: none;
  }

  ul {
    max-height: 50vh;
    margin: 0;
    padding: 4px;
    overflow-y: auto;
    list-style: none;
  }

  li {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 6px 10px;
    border-radius: var(--radius);
    cursor: pointer;
    white-space: nowrap;
  }

  li[aria-selected="true"] {
    background: var(--selected);
  }

  .dir {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .none {
    cursor: default;
  }
</style>
