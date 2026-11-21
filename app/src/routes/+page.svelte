<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Terminal from "$lib/Terminal.svelte";
  import { ago, lang, t } from "$lib/i18n";

  type Repo = { name: string; path: string; modified: number | null };
  type Project = { id: string; name: string; repos: Repo[] };
  type Workspace = {
    configDir: string;
    home: string;
    agents: string[];
    projects: Project[];
    repos: Repo[];
    scanTruncated: boolean;
  };
  type Session = { id: number; title: string; running: boolean; exitCode: number | null };

  let workspace = $state<Workspace | null>(null);
  let error = $state("");
  let filter = $state("");
  let sort = $state<"recent" | "name">("recent");
  let sessions = $state<Session[]>([]);
  let focused = $state<number | null>(null);
  const selected = new SvelteSet<string>();

  const repoNames = $derived(
    new Map(
      [...(workspace?.repos ?? []), ...(workspace?.projects.flatMap((p) => p.repos) ?? [])].map(
        (r) => [r.path, r.name],
      ),
    ),
  );
  const visibleRepos = $derived.by(() => {
    const query = filter.toLowerCase();
    const repos = (workspace?.repos ?? []).filter((r) => r.path.toLowerCase().includes(query));
    return sort === "recent"
      ? repos.sort((a, b) => (b.modified ?? 0) - (a.modified ?? 0))
      : repos.sort((a, b) => a.name.localeCompare(b.name, lang, { sensitivity: "base" }));
  });
  const cols = $derived(Math.max(1, Math.ceil(Math.sqrt(focused === null ? sessions.length : 1))));

  onMount(() => {
    document.documentElement.lang = lang;
    load();
    // Sessions outlive the webview: re-attach to whatever is still running after a reload.
    invoke<Session[]>("pty_list").then((list) => (sessions = list));
    const unlisten = listen<{ id: number; exitCode: number | null }>("pty-exit", ({ payload }) => {
      const session = sessions.find((s) => s.id === payload.id);
      if (session) {
        session.running = false;
        session.exitCode = payload.exitCode;
      }
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  });

  async function load() {
    try {
      workspace = await invoke<Workspace>("workspace");
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  function parentDir(path: string): string {
    const home = workspace?.home ?? "";
    const short = home && path.startsWith(home) ? "~" + path.slice(home.length) : path;
    return short.slice(0, Math.max(short.lastIndexOf("/"), short.lastIndexOf("\\")));
  }

  function toggleProject(project: Project) {
    const all = project.repos.every((r) => selected.has(r.path));
    for (const r of project.repos) {
      if (all) selected.delete(r.path);
      else selected.add(r.path);
    }
  }

  async function open(agent: string | null) {
    for (const path of selected) {
      const name = repoNames.get(path) ?? path;
      try {
        sessions.push(
          await invoke<Session>("pty_spawn", {
            agent,
            cwd: path,
            title: `${name} · ${agent ?? "shell"}`,
            cols: 120,
            rows: 32,
          }),
        );
      } catch (e) {
        error = String(e);
      }
    }
  }

  async function close(id: number) {
    await invoke("pty_kill", { id }).catch(() => {});
    sessions = sessions.filter((s) => s.id !== id);
    if (focused === id) focused = null;
  }
</script>

<div class="app">
  <aside class="sidebar">
    <header class="brand">TermF</header>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    {#if workspace}
      <div class="lists">
        {#if workspace.projects.length}
          <section>
            <h2>{t("projects")}</h2>
            <ul>
              {#each workspace.projects as project (project.id)}
                <li>
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
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        <section>
          <div class="heading">
            <h2>{t("repos")} <span class="count">{workspace.repos.length}</span></h2>
            <div class="segmented" role="group" aria-label={t("sortBy")}>
              <button aria-pressed={sort === "recent"} onclick={() => (sort = "recent")}>
                {t("sortRecent")}
              </button>
              <button aria-pressed={sort === "name"} onclick={() => (sort = "name")}>
                {t("sortName")}
              </button>
            </div>
          </div>
          {#if workspace.scanTruncated}
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
        {#each workspace.agents as agent (agent)}
          <button class="primary" disabled={!selected.size} onclick={() => open(agent)}>
            {t("open", { agent, count: selected.size })}
          </button>
        {/each}
        <button class="secondary" disabled={!selected.size} onclick={() => open(null)}>
          {t("openShell", { count: selected.size })}
        </button>
        <div class="meta">
          <button class="link" onclick={load}>{t("reload")}</button>
          <span class="muted" title={workspace.configDir}>{workspace.configDir}</span>
        </div>
      </footer>
    {/if}
  </aside>

  <main class="grid" style:--cols={cols}>
    {#each sessions as session (session.id)}
      <section
        class="tile"
        class:focused={focused === session.id}
        hidden={focused !== null && focused !== session.id}
        aria-label={session.title}
      >
        <header>
          <span
            class="dot"
            class:running={session.running}
            title={session.running ? t("running") : t("exited")}
          ></span>
          <span class="title">{session.title}</span>
          {#if !session.running}
            <span class="muted"
              >{t("exitedLabel")}{session.exitCode != null ? ` (${session.exitCode})` : ""}</span
            >
          {/if}
          <button
            class="icon"
            onclick={() => (focused = focused === session.id ? null : session.id)}
            aria-label={focused === session.id ? t("unfocus") : t("focus")}
            title={focused === session.id ? t("unfocus") : t("focus")}
          >
            {focused === session.id ? "⤡" : "⤢"}
          </button>
          <button
            class="icon"
            onclick={() => close(session.id)}
            aria-label={t("close")}
            title={t("close")}
          >
            ×
          </button>
        </header>
        <Terminal id={session.id} />
      </section>
    {:else}
      <p class="empty muted">{t("empty")}</p>
    {/each}
  </main>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 280px 1fr;
    height: 100vh;
  }

  .sidebar {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
    border-right: 1px solid var(--border);
  }

  .brand {
    padding: 14px 16px 10px;
    font-weight: 650;
    letter-spacing: 0.02em;
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

  .muted {
    color: var(--muted);
  }

  .error {
    margin: 0 12px 8px;
    padding: 8px;
    border-radius: 6px;
    color: var(--danger);
    background: var(--hover);
    overflow-wrap: anywhere;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    grid-auto-rows: minmax(0, 1fr);
    gap: 6px;
    padding: 6px;
    min-height: 0;
    overflow: hidden;
  }

  .empty {
    grid-column: 1 / -1;
    place-self: center;
  }

  .tile {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    background: var(--term-bg);
  }

  .tile[hidden] {
    display: none;
  }

  .tile header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px 4px 10px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
  }

  .title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--stopped);
  }

  .dot.running {
    background: var(--running);
  }

  .icon {
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--muted);
  }

  .icon:hover {
    background: var(--hover);
    color: var(--text);
  }
</style>
