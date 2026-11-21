<script lang="ts">
  import Info from "@lucide/svelte/icons/info";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Search from "@lucide/svelte/icons/search";
  import Smartphone from "@lucide/svelte/icons/smartphone";
  import Layouts from "$lib/Layouts.svelte";
  import ProjectEditor from "$lib/ProjectEditor.svelte";
  import Section from "$lib/Section.svelte";
  import { ago, lang, t } from "$lib/i18n";
  import {
    app,
    launch,
    launchAgent,
    load,
    openSelected,
    selected,
    shortPath,
    toggleProject,
    ui,
  } from "$lib/state.svelte";

  let filter = $state("");
  let sort = $state<"recent" | "name">("recent");
  let creating = $state(false);
  const agent = $derived(launchAgent());

  const visibleRepos = $derived.by(() => {
    const query = filter.toLowerCase();
    const repos = (app.workspace?.repos ?? []).filter((r) => r.path.toLowerCase().includes(query));
    return sort === "recent"
      ? repos.sort((a, b) => (b.modified ?? 0) - (a.modified ?? 0))
      : repos.sort((a, b) => a.name.localeCompare(b.name, lang, { sensitivity: "base" }));
  });

  function parentDir(path: string): string {
    const short = shortPath(path);
    return short.slice(0, Math.max(short.lastIndexOf("/"), short.lastIndexOf("\\")));
  }
</script>

<aside class="sidebar">
  {#if app.workspace}
    {#if app.workspace.apiError || app.workspace.quickTerminalError}
      <div class="notices">
        {#if app.workspace.apiError}
          <p class="notice" title={app.workspace.apiError}>
            <Info size={14} aria-hidden="true" />{t("apiOff")}
          </p>
        {/if}
        {#if app.workspace.quickTerminalError}
          <p class="notice">
            <Info size={14} aria-hidden="true" />{t("shortcutError", {
              error: app.workspace.quickTerminalError,
            })}
          </p>
        {/if}
      </div>
    {/if}

    <div class="sections">
      <Section id="projects" title={t("projects")} count={app.workspace.projects.length}>
        {#snippet actions()}
          <button
            class="icon-btn sm"
            onclick={() => ((creating = true), (ui.editing = null))}
            aria-label={t("newProject")}
            title={t("newProject")}
          >
            <Plus size={14} aria-hidden="true" />
          </button>
        {/snippet}
        {#if creating}
          <ProjectEditor onclose={() => (creating = false)} />
        {/if}
        <ul>
          {#each app.workspace.projects as project (project.id)}
            {@const ticked = project.repos.filter((r) => selected.has(r.path)).length}
            <li>
              {#if ui.editing === project.id}
                <ProjectEditor {project} onclose={() => (ui.editing = null)} />
              {:else}
                <div class="item">
                  <label class="row">
                    <input
                      type="checkbox"
                      checked={project.repos.length > 0 && ticked === project.repos.length}
                      indeterminate={ticked > 0 && ticked < project.repos.length}
                      onchange={() => toggleProject(project)}
                    />
                    <span class="name">{project.name}</span>
                    <span class="meta">{project.repos.length}</span>
                  </label>
                  <button
                    class="icon-btn sm reveal"
                    onclick={() => ((ui.editing = project.id), (creating = false))}
                    aria-label={t("editProject", { name: project.name })}
                    title={t("editProject", { name: project.name })}
                  >
                    <Pencil size={13} aria-hidden="true" />
                  </button>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      </Section>

      <Section id="repos" title={t("repos")} count={app.workspace.repos.length} grow>
        {#snippet actions()}
          {#if app.workspace?.repos.length}
            <div class="segmented" role="group" aria-label={t("sortBy")}>
              <button aria-pressed={sort === "recent"} onclick={() => (sort = "recent")}>
                {t("sortRecent")}
              </button>
              <button aria-pressed={sort === "name"} onclick={() => (sort = "name")}>
                {t("sortName")}
              </button>
            </div>
          {/if}
        {/snippet}
        {#if app.workspace.scanTruncated}
          <p class="notice spaced"><Info size={14} aria-hidden="true" />{t("truncated")}</p>
        {/if}
        {#if app.workspace.repos.length}
          <div class="filter">
            <Search size={14} aria-hidden="true" />
            <input
              class="field"
              type="search"
              placeholder={t("filter")}
              aria-label={t("filter")}
              bind:value={filter}
            />
          </div>
          <ul class="repos">
            {#each visibleRepos as repo (repo.path)}
              <li>
                <label class="row repo" title={repo.path}>
                  <input
                    type="checkbox"
                    checked={selected.has(repo.path)}
                    onchange={(e) =>
                      e.currentTarget.checked ? selected.add(repo.path) : selected.delete(repo.path)}
                  />
                  <span class="stack">
                    <span class="name">{repo.name}</span>
                    <span class="path">{parentDir(repo.path)}</span>
                  </span>
                  {#if repo.modified}
                    <span class="meta">{ago(repo.modified)}</span>
                  {/if}
                </label>
              </li>
            {:else}
              <li class="none">{t("noRepos")}</li>
            {/each}
          </ul>
        {:else}
          <div class="no-repos">
            <p><strong>{t("noRepos")}</strong> {t("noReposHelp")}</p>
            <code title={app.workspace.configDir}>{shortPath(app.workspace.configDir)}/termf.toml</code>
            <button class="btn sm" onclick={load}>
              <RefreshCw size={14} aria-hidden="true" />{t("reload")}
            </button>
          </div>
        {/if}
      </Section>

      <Layouts />
    </div>

    <footer class="launch">
      <div class="segmented agents" role="radiogroup" aria-label={t("agent")}>
        {#each [...app.workspace.agents, null] as choice (choice)}
          <label>
            <input
              class="sr-only"
              type="radio"
              name="agent"
              checked={agent === choice}
              onchange={() => (launch.agent = choice)}
            />
            <span>{choice ?? t("shell")}</span>
          </label>
        {/each}
      </div>
      <label class="switch" title={t("remoteControlOn")} class:off={agent === null}>
        <Smartphone size={14} aria-hidden="true" />
        <span>{t("remoteControl")}</span>
        <input type="checkbox" role="switch" disabled={agent === null} bind:checked={launch.remoteControl} />
      </label>
      <button class="btn primary open" disabled={!selected.size} onclick={() => openSelected(agent)}>
        {agent ? t("open", { agent, count: selected.size }) : t("openShell", { count: selected.size })}
      </button>
    </footer>
  {/if}
</aside>

<style>
  .sidebar {
    grid-area: side;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border);
    background: var(--chrome);
  }

  .notices {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px 12px 0;
  }

  .sections {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    padding-bottom: 8px;
    overflow-y: auto;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .repos {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .item {
    display: flex;
    align-items: center;
    border-radius: var(--radius);
  }

  .item:hover {
    background: var(--hover);
  }

  .item .row {
    flex: 1;
    min-width: 0;
  }

  .item:hover .row {
    background: none;
  }

  .item .icon-btn {
    margin-right: 4px;
  }

  .reveal {
    opacity: 0;
  }

  .item:hover .reveal,
  .reveal:focus-visible {
    opacity: 1;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 30px;
    padding: 4px 8px;
    border-radius: var(--radius);
    cursor: pointer;
  }

  .row:hover {
    background: var(--hover);
  }

  .row input {
    flex-shrink: 0;
    margin: 0;
  }

  .repo {
    align-items: center;
    padding-block: 5px;
  }

  .stack {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .name,
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name {
    flex: 1;
    min-width: 0;
    font-weight: 500;
  }

  .path {
    color: var(--muted);
    font-size: var(--text-xs);
    line-height: 1.35;
  }

  .meta {
    flex-shrink: 0;
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .filter {
    position: relative;
    margin: 0 0 6px;
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

  .none {
    padding: 6px 8px;
    color: var(--muted);
  }

  .spaced {
    margin-bottom: 6px;
  }

  .no-repos {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 4px 8px 8px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .no-repos p {
    margin: 0;
  }

  .no-repos strong {
    display: block;
    margin-bottom: 2px;
    color: var(--text);
    font-size: var(--text-base);
    font-weight: 600;
  }

  .no-repos code {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
    font: var(--text-xs) var(--font-mono);
  }

  .launch {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border-top: 1px solid var(--border);
  }

  .agents {
    display: flex;
  }

  .agents label {
    display: flex;
    flex: 1;
    min-width: 0;
    cursor: pointer;
  }

  .agents span {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: center;
    padding: 0 8px;
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

  .switch {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 2px;
    color: var(--muted);
    font-size: var(--text-sm);
    cursor: pointer;
    white-space: nowrap;
  }

  .switch.off {
    opacity: 0.5;
    cursor: default;
  }

  .switch span {
    flex: 1;
  }

  .switch input {
    appearance: none;
    position: relative;
    flex-shrink: 0;
    width: 28px;
    height: 16px;
    margin: 0;
    border-radius: var(--radius-pill);
    background: var(--field-border);
    cursor: inherit;
    transition: background-color var(--dur-fast) ease-out;
  }

  .switch input::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #fff;
    box-shadow: var(--shadow-1);
    transition: transform var(--dur-fast) var(--ease-out);
  }

  .switch input:checked {
    background: var(--accent);
  }

  .switch input:checked::after {
    transform: translateX(12px);
  }

  .open {
    height: 34px;
    width: 100%;
  }
</style>
