<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { files, openFile } from "$lib/files.svelte";

  type Hit = { path: string; line: number; text: string };

  let { input = $bindable() }: { input?: HTMLInputElement } = $props();

  let query = $state("");
  let regex = $state(false);
  let hits = $state<Hit[]>([]);
  let truncated = $state(false);
  let error = $state("");
  let searched = $state("");
  let seq = 0;

  const groups = $derived(
    hits.reduce<{ path: string; hits: Hit[] }[]>((groups, hit) => {
      const last = groups[groups.length - 1];
      if (last?.path === hit.path) last.hits.push(hit);
      else groups.push({ path: hit.path, hits: [hit] });
      return groups;
    }, []),
  );

  $effect(() => {
    const args = { repo: files.repo, query, regex };
    const n = ++seq;
    const timer = setTimeout(async () => {
      try {
        const result = await invoke<{ hits: Hit[]; truncated: boolean }>("files_search", args);
        if (n !== seq) return;
        ({ hits, truncated } = result);
        searched = args.query;
        error = "";
      } catch (e) {
        if (n === seq) error = String(e);
      }
    }, 250);
    return () => clearTimeout(timer);
  });
</script>

<div class="search">
  <div class="fields">
    <input
      bind:this={input}
      type="search"
      placeholder={t("filesSearch")}
      aria-label={t("filesSearch")}
      bind:value={query}
    />
    <button
      class="regex"
      aria-pressed={regex}
      title={t("filesRegex")}
      aria-label={t("filesRegex")}
      onclick={() => (regex = !regex)}>.*</button
    >
  </div>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if query && query === searched && !hits.length}
    <p class="muted note">{t("filesNoResults")}</p>
  {/if}
  {#if truncated}
    <p class="muted note">{t("filesTruncated", { count: hits.length })}</p>
  {/if}
  <ul>
    {#each groups as group (group.path)}
      <li>
        <div class="file" title={group.path}>{group.path}</div>
        <ul>
          {#each group.hits as hit (hit.line)}
            <li>
              <button class="hit" onclick={() => openFile(hit.path, hit.line)}>
                <span class="line">{hit.line}</span>
                <span class="text">{hit.text}</span>
              </button>
            </li>
          {/each}
        </ul>
      </li>
    {/each}
  </ul>
</div>

<style>
  .search {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
  }

  .fields {
    display: flex;
    gap: 4px;
    padding: 0 8px;
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }

  .regex {
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: none;
    color: var(--muted);
    font-family: "JetBrains Mono", Menlo, Consolas, monospace;
  }

  .regex[aria-pressed="true"] {
    border-color: var(--accent);
    color: var(--accent);
  }

  .note,
  .error {
    margin: 0;
    padding: 0 12px;
    font-size: 12px;
  }

  .error {
    color: var(--danger);
    overflow-wrap: anywhere;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .file {
    padding: 6px 12px 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
    font-size: 12px;
  }

  .hit {
    display: flex;
    gap: 8px;
    width: 100%;
    padding: 2px 12px 2px 20px;
    border: 0;
    background: none;
    text-align: left;
    font: 12px/1.5 "JetBrains Mono", Menlo, Consolas, monospace;
  }

  .hit:hover {
    background: var(--hover);
  }

  .line {
    flex-shrink: 0;
    min-width: 3ch;
    color: var(--muted);
    text-align: right;
  }

  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
