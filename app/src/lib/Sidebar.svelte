<script lang="ts">
  import Layouts from "$lib/Layouts.svelte";
  import ProjectEditor from "$lib/ProjectEditor.svelte";
  import { toggleQuickTerminal } from "$lib/QuickTerminal.svelte";
  import { ago, lang, t } from "$lib/i18n";
  import { app, launch, load, openSelected, selected, toggleProject } from "$lib/state.svelte";

  let filter = $state("");
  let sort = $state<"recent" | "name">("recent");
  let creating = $state(false);
  let editing = $state<string | null>(null);
  const waiting = $derived(app.sessions.filter((s) => s.status === "waiting").length);

  const visibleRepos = $derived.by(() => {
    const query = filter.toLowerCase();
    const repos = (app.workspace?.repos ?? []).filter((r) => r.path.toLowerCase().includes(query));
    return sort === "recent"
      ? repos.sort((a, b) => (b.modified ?? 0) - (a.modified ?? 0))
      : repos.sort((a, b) => a.name.localeCompare(b.name, lang, { sensitivity: "base" }));
  });

  function parentDir(path: string): string {
    const home = app.workspace?.home ?? "";
    const short = home && path.startsWith(home) ? "~" + path.slice(home.length) : path;
    return short.slice(0, Math.max(short.lastIndexOf("/"), short.lastIndexOf("\\")));
  }
</script>

<aside class="sidebar">
  <header class="brand">
    TermF
    {#if waiting}
      <span class="waiting">{t("waitingCount", { count: waiting })}</span>
    {/if}
    <button
      class="icon quick"
      onclick={toggleQuickTerminal}
      aria-label={t("quickTerminal")}
      aria-keyshortcuts="Control+`"
      title={t("quickTerminalTitle")}
    >
      &gt;_
    </button>
  </header>

  {#if app.error}
    <p class="error" role="alert">{app.error}</p>
  {/if}

  {#if app.workspace}
    <div class="lists">
      {#if app.workspace.quickTerminalError}
        <p class="notice">{t("shortcutError", { error: app.workspace.quickTerminalError })}</p>
      {/if}
      <section>
        <div class="heading">
          <h2>{t("projects")}</h2>
          <button
            class="icon"
            onclick={() => ((creating = true), (editing = null))}
            aria-label={t("newProject")}
            title={t("newProject")}
          >
            +
          </button>
        </div>
        {#if creating}
          <ProjectEditor onclose={() => (creating = false)} />
        {/if}
        <ul>
          {#each app.workspace.projects as project (project.id)}
            <li>
              {#if editing === project.id}
                <ProjectEditor {project} onclose={() => (editing = null)} />
              {:else}
                <div class="item">
                  <label class="row">
                    <input
                      type="checkbox"
                      checked={project.repos.length > 0 &&
                        project.repos.every((r) => selected.has(r.path))}
                      onchange={() => toggleProject(project)}
                    />
                    <span class="name">{project.name}</span>
                    <span class="count">{project.repos.length}</span>
                  </label>
                  <button
                    class="icon edit"
                    onclick={() => ((editing = project.id), (creating = false))}
                    aria-label={t("editProject", { name: project.name })}
                    title={t("editProject", { name: project.name })}
                  >
                    ✎
                  </button>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      </section>

      <Layouts />

      <section>
        <div class="heading">
          <h2>{t("repos")} <span class="count">{app.workspace.repos.length}</span></h2>
          <div class="segmented" role="group" aria-label={t("sortBy")}>
            <button aria-pressed={sort === "recent"} onclick={() => (sort = "recent")}>
              {t("sortRecent")}
            </button>
            <button aria-pressed={sort === "name"} onclick={() => (sort = "name")}>
              {t("sortName")}
            </button>
          </div>
        </div>
        {#if app.workspace.scanTruncated}
          <p class="notice">{t("truncated")}</p>
        {/if}
        <input class="filter" type="search" placeholder={t("filter")} bind:value={filter} />
        <ul>
          {#each visibleRepos as repo (repo.path)}
            <li>
              <label class="row" title={repo.path}>
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
                  <span class="count">{ago(repo.modified)}</span>
                {/if}
              </label>
            </li>
          {:else}
            <li class="muted">{t("noRepos")}</li>
          {/each}
        </ul>
      </section>
    </div>

    <footer class="actions">
      <label class="toggle">
        <input type="checkbox" bind:checked={launch.remoteControl} />
        {t("remoteControl")}
      </label>
      {#each app.workspace.agents as agent (agent)}
        <button class="primary" disabled={!selected.size} onclick={() => openSelected(agent)}>
          {t("open", { agent, count: selected.size })}
        </button>
      {/each}
      <button class="secondary" disabled={!selected.size} onclick={() => openSelected(null)}>
        {t("openShell", { count: selected.size })}
      </button>
      <div class="meta">
        <button class="link" onclick={load}>{t("reload")}</button>
        <span class="muted" title={app.workspace.configDir}>{app.workspace.configDir}</span>
      </div>
    </footer>
  {/if}
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
    border-right: 1px solid var(--border);
  }

  .brand {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 12px 8px 16px;
    font-weight: 650;
    letter-spacing: 0.02em;
  }

  /* Next to the title; the quick terminal button stays at the right. */
  .waiting {
    margin: 0 auto 0 8px;
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--waiting);
    color: var(--waiting-text);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }

  .lists {
    flex: 1;
    overflow-y: auto;
    padding: 0 8px;
  }

  h2 {
    margin: 12px 8px 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }

  .heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-right: 4px;
  }

  .segmented {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }

  .segmented button {
    padding: 2px 8px;
    border: 0;
    background: none;
    color: var(--muted);
    font-size: 11px;
  }

  .segmented button[aria-pressed="true"] {
    background: var(--hover);
    color: var(--text);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    border-radius: 6px;
    cursor: pointer;
  }

  .row:hover {
    background: var(--hover);
  }

  .item {
    display: flex;
    align-items: center;
  }

  .item .row {
    flex: 1;
    min-width: 0;
  }

  .icon {
    width: 22px;
    height: 22px;
    flex-shrink: 0;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--muted);
  }

  .icon:hover {
    background: var(--hover);
    color: var(--text);
  }

  .edit {
    opacity: 0;
  }

  .quick {
    width: 28px;
    font: 600 12px ui-monospace, monospace;
    letter-spacing: 0;
  }

  .item:hover .edit,
  .edit:focus-visible {
    opacity: 1;
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
  }

  .path {
    color: var(--muted);
    font-size: 11px;
  }

  .count {
    flex-shrink: 0;
    color: var(--muted);
    font-size: 11px;
    font-weight: 400;
  }

  .filter {
    width: 100%;
    margin: 0 0 6px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }

  .notice {
    margin: 0 0 6px;
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--hover);
    color: var(--muted);
    font-size: 11px;
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
    border-top: 1px solid var(--border);
  }

  .primary,
  .secondary {
    padding: 7px 10px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--panel);
  }

  .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 11px;
  }

  .meta span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .link {
    align-self: flex-start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
  }

  .error {
    margin: 0 12px 8px;
    padding: 8px;
    border-radius: 6px;
    color: var(--danger);
    background: var(--hover);
    overflow-wrap: anywhere;
  }
</style>
