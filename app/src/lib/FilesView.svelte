<script lang="ts">
  import FileEditor from "$lib/FileEditor.svelte";
  import FileFinder from "$lib/FileFinder.svelte";
  import FileSearch from "$lib/FileSearch.svelte";
  import FileTree from "$lib/FileTree.svelte";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import { closeTab, files, isMac, refreshStatus, tabKey } from "$lib/files.svelte";

  let searchInput = $state<HTMLInputElement>();
  let question: HTMLDialogElement;

  $effect(() => {
    void files.repo;
    refreshStatus();
  });

  $effect(() => {
    if (files.question) question.showModal();
  });

  function answer(yes: boolean) {
    files.question?.answer(yes);
    files.question = null;
    question.close();
  }

  function refresh() {
    files.version++;
    refreshStatus();
  }

  // Only while this view is shown: Ctrl+P belongs to the shell in a terminal.
  function onkeydown(e: KeyboardEvent) {
    if (files.view !== "files" || !(isMac ? e.metaKey : e.ctrlKey) || !files.repo) return;
    const key = e.key.toLowerCase();
    if (key === "p" && !e.shiftKey) {
      e.preventDefault();
      files.finder = true;
    } else if (key === "f" && e.shiftKey) {
      e.preventDefault();
      files.panel = "search";
      requestAnimationFrame(() => searchInput?.focus());
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="files">
  <aside class="side">
    <div class="bar">
      <select bind:value={files.repo} aria-label={t("filesRepo")}>
        {#if !files.repo}
          <option value="">{t("noRepos")}</option>
        {/if}
        {#each app.workspace?.projects ?? [] as project (project.id)}
          <optgroup label={project.name}>
            {#each project.repos as repo (repo.path)}
              <option value={repo.path}>{repo.name}</option>
            {/each}
          </optgroup>
        {/each}
        <optgroup label={t("repos")}>
          {#each app.workspace?.repos ?? [] as repo (repo.path)}
            <option value={repo.path}>{repo.name}</option>
          {/each}
        </optgroup>
      </select>
      <button class="icon" onclick={refresh} aria-label={t("filesRefresh")} title={t("filesRefresh")}
        >↻</button
      >
    </div>
    <div class="segmented" role="group">
      <button aria-pressed={files.panel === "tree"} onclick={() => (files.panel = "tree")}>
        {t("filesTree")}
      </button>
      <button
        aria-pressed={files.panel === "search"}
        onclick={() => (files.panel = "search")}
        title={isMac ? "⌘⇧F" : "Ctrl+Shift+F"}
      >
        {t("filesSearchTab")}
      </button>
    </div>
    {#if files.error}
      <p class="error" role="alert">{files.error}</p>
    {/if}
    <div class="panel">
      {#if files.repo}
        {#key files.repo}
          <div hidden={files.panel !== "tree"}><FileTree repo={files.repo} dir="" /></div>
          <div hidden={files.panel !== "search"}><FileSearch bind:input={searchInput} /></div>
        {/key}
      {/if}
    </div>
  </aside>

  <div class="main">
    {#if files.tabs.length}
      <div class="tabs">
        {#each files.tabs as tab (tabKey(tab))}
          <div class="tab" class:active={tab === files.active} title={tab.path}>
            <button aria-current={tab === files.active} onclick={() => (files.active = tab)}>
              {tab.name}
            </button>
            {#if tab.dirty}
              <span class="dirty" title={t("fileUnsavedLabel")}>●</span>
            {/if}
            <button class="close" onclick={() => closeTab(tab)} aria-label={t("close")} title={t("close")}
              >×</button
            >
          </div>
        {/each}
      </div>
    {/if}
    <FileEditor />
  </div>
</div>

{#if files.finder}
  <FileFinder />
{/if}

<dialog bind:this={question} class="question" oncancel={() => answer(false)}>
  <p>{files.question?.message}</p>
  <div class="answers">
    <button onclick={() => answer(false)}>{t("cancel")}</button>
    <button class="danger" onclick={() => answer(true)}>{t("confirm")}</button>
  </div>
</dialog>

<style>
  .files {
    display: grid;
    grid-template-columns: 260px minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    --modified: #b7791f;
  }

  @media (prefers-color-scheme: dark) {
    .files {
      --modified: #e3b341;
    }
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
    padding-top: 6px;
    border-right: 1px solid var(--border);
    background: var(--panel);
  }

  .bar {
    display: flex;
    gap: 4px;
    padding: 0 8px;
  }

  select {
    flex: 1;
    min-width: 0;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }

  .icon {
    width: 26px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--muted);
  }

  .icon:hover {
    background: var(--hover);
    color: var(--text);
  }

  .segmented {
    display: flex;
    margin: 0 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }

  .segmented button {
    flex: 1;
    padding: 2px 8px;
    border: 0;
    background: none;
    color: var(--muted);
    font-size: 12px;
  }

  .segmented button[aria-pressed="true"] {
    background: var(--hover);
    color: var(--text);
  }

  .panel {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 4px 8px;
  }

  .error {
    margin: 0 8px;
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--hover);
    color: var(--danger);
    font-size: 12px;
    overflow-wrap: anywhere;
  }

  .main {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }

  .tabs {
    display: flex;
    overflow-x: auto;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }

  .tab {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    border-right: 1px solid var(--border);
    color: var(--muted);
  }

  .tab.active {
    background: var(--panel);
    color: var(--text);
  }

  .tab button {
    border: 0;
    background: none;
    padding: 6px 4px 6px 10px;
  }

  .tab .close {
    padding: 2px 8px 2px 4px;
  }

  .dirty {
    color: var(--accent);
    font-size: 10px;
  }

  .question {
    max-width: 420px;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--panel);
    color: var(--text);
  }

  .question::backdrop {
    background: rgb(0 0 0 / 0.2);
  }

  .question p {
    margin: 0 0 12px;
  }

  .answers {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .answers button {
    padding: 5px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
  }

  .answers .danger {
    border-color: var(--danger);
    background: var(--danger);
    color: var(--accent-text);
  }
</style>
