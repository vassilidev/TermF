<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import Folder from "@lucide/svelte/icons/folder";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import FolderSearch from "@lucide/svelte/icons/folder-search";
  import Search from "@lucide/svelte/icons/search";
  import RepoCard from "$lib/RepoCard.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import ForgeIcon from "$lib/ui/ForgeIcon.svelte";
  import { t } from "$lib/i18n";
  import { cards, forgetCards, readCards } from "$lib/newproject.svelte";
  import { byActivity, groups, shown, type Card, type Filter } from "$lib/setup-logic";
  import { app, shortPath } from "$lib/state.svelte";

  /** The repositories of the machine to pick from, most recently worked on first, as rich cards. */
  let {
    chosen = $bindable(),
    project = "",
    autofocus = false,
  }: {
    chosen: string[];
    /** The project being edited, which is not "another project" of its own repositories. */
    project?: string;
    autofocus?: boolean;
  } = $props();

  let query = $state("");
  let filter = $state<Filter>("all");
  /** Folders added with the dialog, outside discovery. */
  let added = $state<string[]>([]);
  let error = $state("");
  let list = $state<HTMLElement>();
  /** Where the keyboard is in the list, by path. */
  let active = $state("");
  const now = Date.now() / 1000;
  forgetCards();
  const home = $derived(app.workspace?.home ?? "");

  const all = $derived.by(() => {
    const seen = new Map<string, Card>();
    const known = [...(app.workspace?.repos ?? []), ...(app.workspace?.projects.flatMap((p) => p.repos) ?? [])];
    for (const r of known) if (!seen.has(r.path)) seen.set(r.path, { ...r });
    for (const path of [...added, ...chosen]) if (!seen.has(path)) seen.set(path, { name: path.split(/[\\/]/).pop() ?? path, path, modified: null });
    const sorted = byActivity([...seen.values()].map((c) => ({ ...c, ...cards[c.path] })));
    // A folder just added comes first, where the owner looks.
    return [...sorted.filter((c) => added.includes(c.path)), ...sorted.filter((c) => !added.includes(c.path))];
  });
  const visible = $derived(all.filter((c) => shown(c, filter, query, now)));
  const counts = $derived({
    all: all.length,
    recent: all.filter((c) => shown(c, "recent", "", now)).length,
    github: all.filter((c) => shown(c, "github", "", now)).length,
  });
  const suggestions = $derived(groups(all, chosen, home).slice(0, 2));

  const others = (path: string) =>
    (app.workspace?.projects ?? []).filter((p) => p.id !== project && p.repos.some((r) => r.path === path)).map((p) => p.name);

  // Each card is read once per visit; the stack's survey is kept by the backend, so a second visit is quick.
  $effect(() => {
    readCards(all.map((c) => c.path)).catch((e) => (error = String(e)));
  });

  function toggle(path: string) {
    chosen = chosen.includes(path) ? chosen.filter((p) => p !== path) : [...chosen, path];
  }

  function take(paths: string[]) {
    chosen = [...chosen, ...paths.filter((p) => !chosen.includes(p))];
  }

  async function addFolder() {
    error = "";
    try {
      const path = await open({ directory: true, multiple: false, title: t("setupAddFolder") });
      if (typeof path !== "string") return;
      if (!added.includes(path)) added = [...added, path];
      take([path]);
      await readCards([path]);
    } catch (e) {
      error = String(e);
    }
  }

  function focusOption(path: string) {
    active = path;
    list?.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`)?.focus();
  }

  function onListKey(e: KeyboardEvent) {
    const i = visible.findIndex((c) => c.path === active);
    const go = (to: number) => {
      e.preventDefault();
      const card = visible[Math.max(0, Math.min(visible.length - 1, to))];
      if (card) focusOption(card.path);
    };
    if (e.key === "ArrowDown") go(i + 1);
    else if (e.key === "ArrowUp") i <= 0 ? (e.preventDefault(), list?.parentElement?.querySelector<HTMLInputElement>("input[type=search]")?.focus()) : go(i - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(visible.length - 1);
    else if ((e.key === " " || e.key === "Enter") && !e.metaKey && !e.ctrlKey && active) {
      e.preventDefault();
      toggle(active);
    }
  }

  function onSearchKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown" && visible.length) {
      e.preventDefault();
      focusOption(visible.some((c) => c.path === active) ? active : visible[0].path);
    } else if (e.key === "Escape" && query) {
      e.preventDefault();
      e.stopPropagation();
      query = "";
    }
  }

  const filters: [Filter, Parameters<typeof t>[0]][] = [
    ["all", "setupFilterAll"],
    ["recent", "setupFilterRecent"],
    ["github", "setupFilterGithub"],
  ];
</script>

<div class="picker">
  <label class="search">
    <Search size={16} aria-hidden="true" />
    <input
      type="search"
      class="field"
      placeholder={t("setupSearch")}
      aria-label={t("setupSearch")}
      aria-controls="repo-list"
      spellcheck="false"
      autocomplete="off"
      bind:value={query}
      onkeydown={onSearchKey}
      {@attach (el) => {
        if (autofocus) el.focus();
      }}
    />
  </label>
  <div class="tools">
    <div class="segmented" role="radiogroup" aria-label={t("setupFilter")}>
      {#each filters as [id, label] (id)}
        <button type="button" role="radio" aria-checked={filter === id} onclick={() => (filter = id)}>
          {t(label)} <span class="n">{counts[id]}</span>
        </button>
      {/each}
    </div>
    <span class="grow"></span>
    <button type="button" class="btn sm quiet" onclick={addFolder}><FolderPlus size={14} aria-hidden="true" />{t("setupAddFolder")}</button>
  </div>

  {#if suggestions.length}
    <ul class="suggest" aria-label={t("setupSuggestions")}>
      {#each suggestions as g (g.kind + g.label)}
        <li>
          <button type="button" onclick={() => take(g.paths)}>
            {#if g.kind === "owner"}<ForgeIcon host="github.com" size={13} />{:else}<Folder size={13} aria-hidden="true" />{/if}
            <span class="text">
              {#if g.kind === "owner"}{t("setupGroupOwner", { owner: g.label, count: g.paths.length })}{:else}{t("setupGroupFolder", { folder: shortPath(g.label), count: g.paths.length })}{/if}
            </span>
            <span class="take">{t("setupTakeAll")}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if error}<p class="notice error" role="alert">{error}</p>{/if}

  {#if visible.length}
    <!-- svelte-ignore a11y_interactive_supports_focus (each option takes the focus, the list passes it along) -->
    <ul
      id="repo-list"
      class="list"
      role="listbox"
      aria-multiselectable="true"
      aria-label={t("repos")}
      bind:this={list}
      onkeydown={onListKey}
    >
      {#each visible as card (card.path)}
        {@const on = chosen.includes(card.path)}
        <!-- svelte-ignore a11y_click_events_have_key_events (the listbox takes the keys: arrows, Space, Enter) -->
        <li
          role="option"
          aria-selected={on}
          data-path={card.path}
          tabindex={card.path === active || (!visible.some((c) => c.path === active) && card === visible[0]) ? 0 : -1}
          onclick={() => ((active = card.path), toggle(card.path))}
          onfocus={() => (active = card.path)}
        >
          <RepoCard {card} selected={on} elsewhere={others(card.path)} />
        </li>
      {/each}
    </ul>
  {:else if all.length}
    <p class="none">{t("setupNoMatch", { query: query || t(filters.find(([id]) => id === filter)![1]) })}</p>
  {:else}
    <EmptyState icon={FolderSearch} text={t("setupNoRepos")}>
      <button type="button" class="btn primary" onclick={addFolder}><FolderPlus size={14} aria-hidden="true" />{t("setupAddFolder")}</button>
    </EmptyState>
  {/if}
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }

  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 10px;
  }

  .search {
    position: relative;
    color: var(--muted);
  }

  .search :global(svg) {
    position: absolute;
    top: 12px;
    left: 13px;
    pointer-events: none;
  }

  .search .field {
    height: var(--h-lg);
    padding-left: 38px;
    border-color: var(--border-strong);
    border-radius: var(--radius-md);
    font-size: var(--text-md);
  }

  .grow {
    flex: 1;
  }

  .segmented button {
    height: 26px;
  }

  .n {
    margin-left: 2px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .suggest {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .suggest button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: var(--h-sm);
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--panel);
    color: var(--soft);
    font-size: var(--text-sm);
    transition: border-color var(--dur-fast) ease;
  }

  .suggest button:hover {
    border-color: var(--border-strong);
  }

  .suggest .take {
    color: var(--accent-ink);
    font-weight: 500;
  }

  .list {
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  .list li {
    position: relative;
    cursor: pointer;
    transition: background-color var(--dur-fast) ease-out;
  }

  .list li + li {
    border-top: 1px solid var(--border);
  }

  .list li:hover {
    background: var(--hover);
  }

  .list li[aria-selected="true"] {
    background: var(--accent-soft);
  }

  .list li:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }

  .none {
    margin: 0;
    padding: 28px 16px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-lg);
    color: var(--muted);
    text-align: center;
  }
</style>
