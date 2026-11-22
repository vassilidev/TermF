<script lang="ts">
  import AppWindow from "@lucide/svelte/icons/app-window";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Columns2 from "@lucide/svelte/icons/columns-2";
  import Rows2 from "@lucide/svelte/icons/rows-2";
  import Search from "@lucide/svelte/icons/search";
  import Smartphone from "@lucide/svelte/icons/smartphone";
  import { focusSession } from "$lib/commands";
  import { ago, t } from "$lib/i18n";
  import { currentProject, scopeRepos } from "$lib/files.svelte";
  import { launchable, otherRepos } from "$lib/project.svelte";
  import { currentTab, openFrom } from "$lib/tabs.svelte";
  import {
    app,
    launch,
    launchAgent,
    selected,
    shortPath,
    toggleProject,
    type Repo,
  } from "$lib/state.svelte";

  let { title, onopen }: { title: string; onopen?: () => void } = $props();

  const uid = $props.id();
  let filter = $state("");
  const agent = $derived(launchAgent());
  const project = $derived(currentProject());
  const recent = (repos: Repo[]) =>
    repos
      .filter((r) => r.path.toLowerCase().includes(filter.toLowerCase()))
      .sort((a, b) => (b.modified ?? 0) - (a.modified ?? 0));
  const own = $derived(recent(scopeRepos()));
  const others = $derived(otherRepos());
  const count = $derived(launchable().length);
  const ticked = $derived(project ? project.repos.filter((r) => selected.has(r.path)).length : 0);
  // Beside a split only when one is on screen; the launcher in place of an empty view opens a tab.
  const beside = $derived(!!currentTab() && !!onopen);
  const places = [
    [null, "t6aNewTab", AppWindow],
    ["right", "t6aBeside", Columns2],
    ["down", "t6aBelow", Rows2],
  ] as const;

  function parentDir(path: string): string {
    const short = shortPath(path);
    return short.slice(0, Math.max(short.lastIndexOf("/"), short.lastIndexOf("\\")));
  }

  async function open() {
    const paths = launchable();
    onopen?.();
    // The keyboard goes to the last split opened, ready to type.
    const last = await openFrom(paths, agent, beside ? launch.side : null);
    if (last !== null) focusSession(last);
  }
</script>

{#snippet row(repo: Repo)}
  <li>
    <label class="row" class:on={selected.has(repo.path)} title={repo.path}>
      <input
        type="checkbox"
        checked={selected.has(repo.path)}
        onchange={(e) => (e.currentTarget.checked ? selected.add(repo.path) : selected.delete(repo.path))}
      />
      <span class="name">{repo.name}<small>{parentDir(repo.path)}</small></span>
      {#if repo.modified}<span class="meta">{ago(repo.modified)}</span>{/if}
    </label>
  </li>
{/snippet}

<section class="launcher" aria-labelledby="{uid}-title">
  <h2 id="{uid}-title">{title}</h2>
  <p>{project ? t("navLauncherHint", { project: project.name }) : t("navLauncherHintAll")}</p>
  <div class="filter">
    <Search size={14} aria-hidden="true" />
    <input class="field" type="search" placeholder={t("filter")} aria-label={t("filter")} bind:value={filter} />
  </div>
  <div class="lists">
    <ul>
      {#if project && project.repos.length > 1 && !filter}
        <li>
          <label class="row all">
            <input
              type="checkbox"
              checked={ticked === project.repos.length}
              indeterminate={ticked > 0 && ticked < project.repos.length}
              onchange={() => toggleProject(project)}
            />
            <span class="name">{project.name}</span>
            <span class="meta">{project.repos.length}</span>
          </label>
        </li>
      {/if}
      {#each own as repo (repo.path)}
        {@render row(repo)}
      {:else}
        <li class="none">{t("noRepos")}</li>
      {/each}
    </ul>
    {#if project && others.length}
      <button
        class="sub"
        aria-expanded={launch.others}
        aria-controls="{uid}-others"
        onclick={() => (launch.others = !launch.others)}
      >
        <ChevronRight size={14} aria-hidden="true" />{t("navOtherRepos", {
          count: others.length,
          project: project.name,
        })}
      </button>
      <ul id="{uid}-others" hidden={!launch.others}>
        {#each recent(others) as repo (repo.path)}
          {@render row(repo)}
        {:else}
          <li class="none">{t("noRepos")}</li>
        {/each}
      </ul>
    {/if}
  </div>
  <footer>
    {#if beside}
      <div class="segmented agents place" role="radiogroup" aria-label={t("t6aWhere")}>
        {#each places as [side, label, Icon] (label)}
          <label>
            <input
              class="sr-only"
              type="radio"
              name="{uid}-place"
              checked={launch.side === side}
              onchange={() => (launch.side = side)}
            />
            <span><Icon size={14} aria-hidden="true" />{t(label)}</span>
          </label>
        {/each}
      </div>
    {/if}
    <div class="segmented agents" role="radiogroup" aria-label={t("agent")}>
      {#each [...(app.workspace?.agents ?? []), null] as choice (choice)}
        <label>
          <input
            class="sr-only"
            type="radio"
            name="{uid}-agent"
            checked={agent === choice}
            onchange={() => (launch.agent = choice)}
          />
          <span>{choice ?? t("shell")}</span>
        </label>
      {/each}
    </div>
    <label class="rc" title={t("remoteControlOn")} class:off={agent === null}>
      <Smartphone size={14} aria-hidden="true" />
      <span>{t("remoteControl")}</span>
      <input class="switch" type="checkbox" role="switch" disabled={agent === null} bind:checked={launch.remoteControl} />
    </label>
    <button class="btn open" class:primary={count > 0} disabled={!count} onclick={open}>
      {agent ? t("open", { agent, count }) : t("openShell", { count })}
    </button>
  </footer>
</section>

<style>
  .launcher {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  h2 {
    margin: 0;
    padding: 16px 16px 4px;
    font-size: var(--text-lg);
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  p {
    margin: 0;
    padding: 0 16px 12px;
    color: var(--muted);
  }

  .filter {
    position: relative;
    margin: 0 12px 6px;
    color: var(--muted);
  }

  .filter :global(svg) {
    position: absolute;
    top: 8px;
    left: 9px;
    pointer-events: none;
  }

  .filter .field {
    padding-left: 28px;
  }

  .lists {
    flex: 1 1 auto;
    min-height: 0;
    max-height: min(420px, 50vh);
    padding: 0 6px 6px;
    overflow-y: auto;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  ul[hidden] {
    display: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 34px;
    padding: 0 8px;
    border-radius: var(--radius);
    cursor: pointer;
  }

  .row:hover {
    background: var(--hover);
  }

  .row.on {
    background: var(--accent-soft);
  }

  .row input {
    flex-shrink: 0;
    margin: 0;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .name small {
    margin-left: 6px;
    color: var(--muted);
    font: 400 var(--text-sm) var(--font-mono);
  }

  .all .name {
    font-weight: 600;
  }

  .meta {
    flex-shrink: 0;
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .none {
    padding: 6px 8px;
    color: var(--muted);
  }

  .sub {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 30px;
    margin-top: 4px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 600;
    text-align: left;
  }

  .sub:hover {
    background: var(--hover);
    color: var(--text);
  }

  .sub :global(svg) {
    transition: transform var(--dur-fast) var(--ease-out);
  }

  .sub[aria-expanded="true"] :global(svg) {
    transform: rotate(90deg);
  }

  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border-top: 1px solid var(--border);
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
    background: var(--chrome);
  }

  .agents label {
    display: flex;
    cursor: pointer;
  }

  .agents span {
    padding: 0 10px;
    border-radius: var(--radius-sm);
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
    line-height: 22px;
  }

  .agents label:hover span {
    color: var(--text);
  }

  .agents input:checked + span {
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow-1);
  }

  .agents input:focus-visible + span {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }

  .place {
    display: flex;
    flex-basis: 100%;
  }

  .place label {
    flex: 1;
  }

  .place span {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    gap: 6px;
    line-height: 26px;
  }

  .rc {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: var(--text-sm);
    white-space: nowrap;
    cursor: pointer;
  }

  .rc.off {
    opacity: 0.5;
    cursor: default;
  }

  .open {
    margin-left: auto;
  }
</style>
