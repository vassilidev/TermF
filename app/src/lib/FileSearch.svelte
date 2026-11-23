<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Regex from "@lucide/svelte/icons/regex";
  import Skeleton from "$lib/ui/Skeleton.svelte";
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
  // Until this query answers, the hits on screen are an older one's: a large repository takes seconds.
  const pending = $derived(!!query && query !== searched);

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
      class="field"
      type="search"
      placeholder={t("filesSearch")}
      aria-label={t("filesSearch")}
      bind:value={query}
    />
    <button
      class="icon-btn regex"
      aria-pressed={regex}
      title={t("filesRegex")}
      aria-label={t("filesRegex")}
      onclick={() => (regex = !regex)}
    >
      <Regex size={15} aria-hidden="true" />
    </button>
  </div>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if pending}
    <div class="loading"><Skeleton rows={4} /></div>
  {:else if query && !hits.length}
    <p class="muted note">{t("filesNoResults")}</p>
  {/if}
  {#if truncated && !pending}
    <p class="muted note">{t("filesTruncated", { count: hits.length })}</p>
  {/if}
  <ul hidden={pending}>
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
  }

  .regex {
    width: 30px;
    height: 30px;
  }

  .regex[aria-pressed="true"] {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .loading {
    padding: 0 12px;
  }

  .note,
  .error {
    margin: 0;
    padding: 0 12px;
    font-size: var(--text-sm);
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
    font-size: var(--text-sm);
  }

  .hit {
    display: flex;
    gap: 8px;
    width: 100%;
    padding: 2px 12px 2px 20px;
    border: 0;
    background: none;
    text-align: left;
    font: var(--text-sm) / 1.5 var(--font-mono);
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
