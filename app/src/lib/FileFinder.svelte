<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { files, openFile } from "$lib/files.svelte";

  let dialog: HTMLDialogElement;
  let query = $state("");
  let results = $state<string[]>([]);
  let selected = $state(0);
  // The first query of each opening walks the repository again, so new files show up.
  let fresh = true;
  let seq = 0;

  onMount(() => dialog.showModal());

  $effect(() => {
    const n = ++seq;
    invoke<string[]>("files_find", { repo: files.repo, query, fresh })
      .then((list) => {
        if (n !== seq) return;
        results = list;
        selected = 0;
      })
      .catch((e) => (files.error = String(e)));
    fresh = false;
  });

  function pick(path: string | undefined) {
    if (path) openFile(path);
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
    {#each results as path, i (path)}
      {@const slash = path.lastIndexOf("/")}
      <!-- svelte-ignore a11y_click_events_have_key_events (the input drives the list with the arrow keys) -->
      <li
        id="finder-{i}"
        role="option"
        aria-selected={i === selected}
        onclick={() => pick(path)}
        onmousemove={() => (selected = i)}
      >
        <span class="name">{path.slice(slash + 1)}</span>
        <span class="dir">{path.slice(0, Math.max(slash, 0))}</span>
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
    border-radius: 10px;
    background: var(--panel);
    color: var(--text);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.25);
  }

  .finder::backdrop {
    background: rgb(0 0 0 / 0.2);
  }

  input {
    width: 100%;
    padding: 10px 12px;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: none;
    color: inherit;
    font: inherit;
    font-size: 14px;
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
    padding: 4px 8px;
    border-radius: 6px;
    cursor: pointer;
    white-space: nowrap;
  }

  li[aria-selected="true"] {
    background: var(--hover);
  }

  .dir {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--muted);
    font-size: 12px;
  }

  .none {
    cursor: default;
  }
</style>
