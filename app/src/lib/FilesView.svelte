<script lang="ts">
  import FileEditor from "$lib/FileEditor.svelte";
  import FileFinder from "$lib/FileFinder.svelte";
  import FileSearch from "$lib/FileSearch.svelte";
  import FileTree from "$lib/FileTree.svelte";
  import RepoChoice from "$lib/RepoChoice.svelte";
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import X from "@lucide/svelte/icons/x";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t } from "$lib/i18n";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { display, pressed } from "$lib/keymap.svelte";
  import { ui } from "$lib/state.svelte";
  import { provide } from "$lib/ask.svelte";
  import {
    activate,
    ask,
    closeTab,
    currentProject,
    files,
    refreshStatus,
    scopeRepos,
    shownRepos,
    sync,
    tabKey,
  } from "$lib/files.svelte";

  let searchInput = $state<HTMLInputElement>();
  let question: HTMLDialogElement;

  const unsaved = () => files.tabs.some((tab) => tab.dirty);

  onMount(() => {
    const unlisten = getCurrentWindow().onCloseRequested(async (event) => {
      if (!unsaved()) return;
      ui.view = "files";
      if (!(await ask(t("filesUnsavedClose")))) event.preventDefault();
    });
    return () => void unlisten.then((stop) => stop());
  });

  const project = $derived(currentProject());
  const repos = $derived(scopeRepos());
  /** With all of the project's repositories: the ones unfolded in the tree. */
  const unfolded = new SvelteSet<string>();

  // The ⌘I panel: the project's expert, with the file open and the lines selected in it.
  $effect(() => {
    const tab = files.active;
    const p = files.picked;
    const lines = p && (p.last > p.first ? `${p.first}-${p.last}` : `${p.first}`);
    return provide("files", {
      repo: files.repo || undefined,
      chips: tab
        ? [
            { id: "file", kind: "object", label: tab.path, text: `File ${tab.path} in ${tab.repo}` },
            ...(p ? [{ id: "lines", kind: "selection" as const, label: `${tab.path}:${lines}`, text: `Lines ${lines} of ${tab.path}:\n${p.text}` }] : []),
          ]
        : [],
    });
  });

  $effect(() => {
    void files.repo;
    refreshStatus();
  });

  $effect(() => {
    if (files.question && !question.open) question.showModal();
  });

  function answer(yes: boolean) {
    files.question?.answer(yes);
    files.question = null;
    question.close();
  }

  function refresh() {
    files.version++;
    refreshStatus();
    files.tabs.forEach(sync);
  }

  // Only while this view is shown (their scope): Ctrl+P belongs to the shell in a terminal.
  function onkeydown(e: KeyboardEvent) {
    const asked = shownRepos().length ? pressed(e)?.id : null;
    if (asked === "find_file") {
      e.preventDefault();
      files.finder = true;
    } else if (asked === "search_project") {
      e.preventDefault();
      files.panel = "search";
      requestAnimationFrame(() => searchInput?.focus());
    }
  }
</script>

<!-- Reload: browsers prompt on preventDefault; the native close goes through onCloseRequested above. -->
<svelte:window {onkeydown} onbeforeunload={(e) => unsaved() && e.preventDefault()} />

<div class="files">
  <ViewHeader title={t("viewFiles")} context={project?.name ?? t("ticketsAll")} />
  <div class="body">
    <aside class="side">
      <div class="bar">
        <RepoChoice />
        <button class="icon-btn" onclick={refresh} aria-label={t("filesRefresh")} title={t("filesRefresh")}>
          <RefreshCw size={14} aria-hidden="true" />
        </button>
      </div>
      <div class="segmented" role="group">
        <button aria-pressed={files.panel === "tree"} onclick={() => (files.panel = "tree")}>
          {t("filesTree")}
        </button>
        <button
          aria-pressed={files.panel === "search"}
          onclick={() => (files.panel = "search")}
          title={display("search_project")}
        >
          {t("filesSearchTab")}
        </button>
      </div>
      {#if files.error}
        <p class="notice error" role="alert">{files.error}</p>
      {/if}
      <div class="panel">
        {#if files.repo || repos.length}
          {#key files.repo}
            <div hidden={files.panel !== "tree"}>
              {#if files.repo}
                <FileTree repo={files.repo} dir="" />
              {:else}
                {#each repos as repo (repo.path)}
                  {@const open = unfolded.has(repo.path)}
                  <button
                    class="root"
                    title={repo.path}
                    aria-expanded={open}
                    onclick={() => (open ? unfolded.delete(repo.path) : unfolded.add(repo.path))}
                  >
                    <span class="chevron" class:open aria-hidden="true"><ChevronRight size={12} /></span>
                    <span class="name">{repo.name}</span>
                  </button>
                  {#if open}<FileTree repo={repo.path} dir="" depth={1} />{/if}
                {/each}
              {/if}
            </div>
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
              <button aria-current={tab === files.active} onclick={() => activate(tab)}>
                {tab.name}
              </button>
              {#if tab.dirty}
                <span class="dirty" role="img" aria-label={t("fileUnsavedLabel")} title={t("fileUnsavedLabel")}></span>
              {/if}
              <button class="icon-btn sm" onclick={() => closeTab(tab)} aria-label={t("close")} title={t("close")}>
                <X size={13} aria-hidden="true" />
              </button>
            </div>
          {/each}
        </div>
      {/if}
      <FileEditor />
    </div>
  </div>
</div>

{#if files.finder}
  <FileFinder />
{/if}

<dialog bind:this={question} class="question" oncancel={() => answer(false)}>
  <p>{files.question?.message}</p>
  <div class="answers">
    <button class="btn" onclick={() => answer(false)}>{t("cancel")}</button>
    <button class="btn danger" onclick={() => answer(true)}>{t("confirm")}</button>
  </div>
</dialog>

<style>
  .files {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .body {
    display: grid;
    flex: 1;
    grid-template-columns: 260px minmax(0, 1fr);
    min-height: 0;
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    padding-top: 8px;
    border-right: 1px solid var(--border);
    background: var(--chrome);
  }

  .bar {
    display: flex;
    gap: 4px;
    padding: 0 8px;
  }

  .bar :global(select) {
    flex: 1;
    min-width: 0;
  }

  .root {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    height: 26px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    font-weight: 600;
    text-align: left;
  }

  .root:hover {
    background: var(--hover);
  }

  .chevron {
    display: grid;
    place-items: center;
    width: 14px;
    flex-shrink: 0;
    color: var(--muted);
    transition: transform var(--dur-fast) var(--ease-out);
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bar .icon-btn {
    width: 30px;
    height: 30px;
  }

  .segmented {
    display: flex;
    margin: 0 8px;
  }

  .segmented button {
    flex: 1;
  }

  .panel {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 4px 8px;
  }

  .error {
    margin: 0 8px;
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
    background: var(--chrome);
    border-bottom: 1px solid var(--border);
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    height: 34px;
    padding-right: 6px;
    border-right: 1px solid var(--border);
    color: var(--muted);
  }

  .tab.active {
    background: var(--panel);
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .tab > button:first-child {
    height: 100%;
    padding: 0 4px 0 12px;
    border: 0;
    background: none;
  }

  .dirty {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }

  .question {
    max-width: 420px;
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--shadow-3);
  }

  .question::backdrop {
    background: var(--scrim, rgb(10 12 20 / 0.3));
  }

  .question p {
    margin: 0 0 16px;
  }

  .answers {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
