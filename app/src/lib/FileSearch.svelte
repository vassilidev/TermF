<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Regex from "@lucide/svelte/icons/regex";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { t } from "$lib/i18n";
  import { openFile, scopeRepos, shownRepos } from "$lib/files.svelte";

  type Hit = { repo: string; path: string; line: number; text: string };

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

  const repos = $derived(shownRepos());
  const names = $derived(new Map(scopeRepos().map((r) => [r.path, r.name])));

  const groups = $derived(
    hits.reduce<{ repo: string; path: string; hits: Hit[] }[]>((groups, hit) => {
      const last = groups[groups.length - 1];
      if (last?.repo === hit.repo && last.path === hit.path) last.hits.push(hit);
      else groups.push({ repo: hit.repo, path: hit.path, hits: [hit] });
      return groups;
    }, []),
  );

  // Every repository shown at once: one that fails leaves the others' hits.
  $effect(() => {
    const [list, args] = [repos, { query, regex }];
    const n = ++seq;
    const timer = setTimeout(async () => {
      const found = await Promise.allSettled(
        list.map(async (repo) => {
          const result = await invoke<{ hits: Omit<Hit, "repo">[]; truncated: boolean }>("files_search", { repo, ...args });
          return { ...result, hits: result.hits.map((hit) => ({ ...hit, repo })) };
        }),
      );
      if (n !== seq) return;
      const done = found.flatMap((f) => (f.status === "fulfilled" ? [f.value] : []));
      error = found.flatMap((f, i) => (f.status === "rejected" ? [list.length > 1 ? `${names.get(list[i])}: ${f.reason}` : String(f.reason)] : [])).join("\n");
      if (list.length && !done.length) return;
      [hits, truncated, searched] = [done.flatMap((r) => r.hits), done.some((r) => r.truncated), args.query];
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
    {#each groups as group (`${group.repo}\n${group.path}`)}
      <li>
        <div class="file" title={group.path}>
          {#if repos.length > 1}<span class="repo">{names.get(group.repo)}</span>{/if}{group.path}
        </div>
        <ul>
          {#each group.hits as hit (hit.line)}
            <li>
              <button class="hit" onclick={() => openFile(hit.path, hit.line, hit.repo)}>
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
    white-space: pre-line;
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

  .repo {
    margin-right: 6px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
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
