<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import Plus from "@lucide/svelte/icons/plus";
  import Search from "@lucide/svelte/icons/search";
  import X from "@lucide/svelte/icons/x";
  import ClaudeTrust from "$lib/ClaudeTrust.svelte";
  import TicketSources from "$lib/TicketSources.svelte";
  import { openSetup } from "$lib/onboarding.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { lang, t } from "$lib/i18n";
  import { library, showLibrary } from "$lib/library.svelte";
  import { closeProject } from "$lib/project.svelte";
  import { app, repoName, setProject, shortPath, ui, type Project } from "$lib/state.svelte";
  import { added } from "$lib/team-logic";

  // Mounted per project (keyed on `ui.editing`), so the project only seeds the form.
  const project = app.workspace?.projects.find((p) => p.id === ui.editing);
  let name = $state(project?.name ?? "");
  let chosen = $state(project?.repos.map((r) => r.path) ?? []);
  let filter = $state("");
  let adding = $state(false);
  let addButton = $state<HTMLButtonElement>();
  let confirming = $state(false);
  let asked = false;
  let busy = $state(false);
  let error = $state("");
  let sources = $state<TicketSources>();
  let recipe = $state<{ saved: boolean } | null>(null);
  let question = $state<HTMLDialogElement>();
  /** Where the current project went while the form had changes, until the user answers. */
  let switching = $state<{ to: string | null }>();
  let savedForm = $state(JSON.stringify([project?.name ?? "", project?.repos.map((r) => r.path) ?? []]));
  const dirty = $derived(JSON.stringify([name, chosen]) !== savedForm || !!sources?.dirty());
  // An id that names no project (deleted, or gone after a reload) never shows a form that would create a copy.
  const missing = $derived(!!ui.editing && !app.workspace?.projects.some((p) => p.id === ui.editing));

  // The Worktrees tab tells why a recipe cannot be read; here it only gives its state.
  if (project) invoke<{ saved: boolean }>("worktree_recipe", { project: project.id }).then((r) => (recipe = r), () => {});

  // The page is the current project's: switching projects switches the page, and every project leaves it.
  $effect(() => {
    if (missing) return closeProject();
    if (!project || app.project === project.id) return;
    const to = app.project;
    if (!untrack(() => dirty)) return follow(to);
    switching = { to };
    question?.showModal();
  });

  function follow(to: string | null) {
    if (to === null) closeProject();
    else ui.editing = to;
  }

  /** Discards the changes and follows the switch, or goes back to this project. */
  function answer(discard: boolean) {
    const to = switching?.to ?? null;
    switching = undefined;
    question?.close();
    if (discard) follow(to);
    else if (project) setProject(project.id);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented || e.isComposing || adding || switching || ui.palette) return;
    // Escape typed in a terminal (the quick terminal over the page) is the terminal's.
    if ((e.target as Element | null)?.closest?.(".xterm")) return;
    e.preventDefault();
    closeProject();
  }

  // The project's own repositories first, even if discovery no longer finds them.
  const candidates = $derived.by(() => {
    const own = project?.repos ?? [];
    const others = (app.workspace?.repos ?? [])
      .filter((r) => !own.some((o) => o.path === r.path))
      .sort((a, b) => a.name.localeCompare(b.name, lang, { sensitivity: "base" }));
    const query = filter.toLowerCase();
    return [...own, ...others].filter((r) => r.path.toLowerCase().includes(query));
  });

  function pick(path: string, on: boolean) {
    chosen = on ? [...chosen, path] : chosen.filter((p) => p !== path);
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!name.trim() || busy) return;
    busy = true;
    error = "";
    try {
      const before = app.workspace!.projects;
      const after = await invoke<Project[]>("project_save", { id: project?.id ?? null, name, repos: chosen });
      app.workspace!.projects = after;
      savedForm = JSON.stringify([name, chosen]);
      await sources?.save();
      const created = project ? null : added(before, after);
      closeProject();
      if (created) setProject(created);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    busy = true;
    error = "";
    try {
      app.workspace!.projects = await invoke<Project[]>("project_delete", { id: project!.id });
      closeProject();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function openRecipe() {
    closeProject();
    Object.assign(library, { global: false, tab: "worktrees", panel: null });
    showLibrary();
  }
</script>

<svelte:window {onkeydown} />

{#if !missing}
  <div class="project-page">
    <ViewHeader title={project?.name ?? t("newProject")} context={t("t5ProjectSettings")}>
      {#snippet actions()}
        <button class="btn sm" onclick={closeProject}>{t("cancel")}</button>
        <button type="submit" form="project-form" class="btn sm" class:primary={!!name.trim()} disabled={!name.trim() || busy}>
          {t("save")}
        </button>
      {/snippet}
    </ViewHeader>

    {#if error}
      <div class="banners">
        <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
      </div>
    {/if}

    <div class="content">
      <form id="project-form" class="form" onsubmit={save}>
        <section>
          <h2><label for="project-name">{t("projectName")}</label></h2>
          <input
            id="project-name"
            class="field name"
            bind:value={name}
            autocomplete="off"
            spellcheck="false"
            {@attach (el) => el.focus()}
          />
        </section>

        <section>
          <h2>{t("repos")} <small>{chosen.length}</small></h2>
          <ul class="repos">
            {#each chosen as path (path)}
              <li>
                <GitBranch size={14} aria-hidden="true" />
                <b>{repoName(path)}</b>
                <span class="mono">{shortPath(path)}</span>
                <button
                  type="button"
                  class="icon-btn sm"
                  aria-label={t("t5RemoveRepo", { name: repoName(path) })}
                  title={t("t5RemoveRepo", { name: repoName(path) })}
                  onclick={() => pick(path, false)}
                >
                  <X size={14} aria-hidden="true" />
                </button>
              </li>
            {/each}
            <li>
              <button
                type="button"
                class="add"
                bind:this={addButton}
                aria-haspopup="dialog"
                aria-expanded={adding}
                onclick={() => (adding = !adding)}
              >
                <Plus size={14} aria-hidden="true" />{t("t5AddRepos")}
              </button>
            </li>
          </ul>
        </section>

        {#if project}
          <section>
            <h2>{t("obConfigure")}</h2>
            <div class="line">
              <span class="muted grow">{t("obConfigureHint")}</span>
              <button type="button" class="btn sm" disabled={dirty} onclick={() => openSetup(project.id)}>
                {t("obConfigure")}
              </button>
            </div>
          </section>

          <TicketSources project={project.id} bind:this={sources} />

          <section>
            <h2>{t("wtTitle")}</h2>
            <div class="line">
              {#if recipe}<span class="muted">{recipe.saved ? t("wtSaved") : t("wtDetected")}</span>{/if}
              <button type="button" class="btn sm" onclick={openRecipe}>{t("t5RecipeEdit")}</button>
            </div>
          </section>

          <section>
            <h2>{t("obTrustTitle")}</h2>
            <ClaudeTrust project={project.id} />
          </section>

          <div class="danger-zone">
            {#if confirming}
              <p role="alert">{t("confirmDeleteProject", { name: project.name })}</p>
              <button type="button" class="btn sm" onclick={() => (confirming = false)} {@attach (el) => el.focus()}>
                {t("cancel")}
              </button>
              <button type="button" class="btn sm danger" disabled={busy} onclick={remove}>{t("delete")}</button>
            {:else}
              <b>{t("t5DeleteProject")}</b>
              <button
                type="button"
                class="btn sm quiet delete"
                onclick={() => ((asked = true), (confirming = true))}
                {@attach (el) => {
                  if (asked) el.focus();
                }}
              >
                {t("delete")}
              </button>
            {/if}
          </div>
        {/if}
      </form>
    </div>
  </div>

  <Popover bind:open={adding} trigger={addButton} label={t("t5AddRepos")} width={440}>
    <div class="search">
      <Search size={14} aria-hidden="true" />
      <input type="search" bind:value={filter} placeholder={t("filter")} aria-label={t("filter")} spellcheck="false" />
    </div>
    <ul class="candidates">
      {#each candidates as repo (repo.path)}
        <li>
          <label>
            <input
              type="checkbox"
              checked={chosen.includes(repo.path)}
              onchange={(e) => pick(repo.path, e.currentTarget.checked)}
            />
            <span class="name">{repo.name}</span>
            <span class="mono">{shortPath(repo.path)}</span>
          </label>
        </li>
      {:else}
        <li class="none">{t("noRepos")}</li>
      {/each}
    </ul>
  </Popover>
{/if}

<dialog bind:this={question} class="question" oncancel={() => answer(false)}>
  <p>{t("t5UnsavedSwitch", { name: project?.name ?? "" })}</p>
  <div class="answers">
    <button class="btn" onclick={() => answer(false)}>{t("cancel")}</button>
    <button class="btn danger" onclick={() => answer(true)}>{t("confirm")}</button>
  </div>
</dialog>

<style>
  .project-page {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }

  .content {
    flex: 1;
    min-height: 0;
    padding: 24px 32px;
    overflow-y: auto;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 28px;
    max-width: 760px;
  }

  h2 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0 0 8px;
    font-size: var(--text-md);
    font-weight: 650;
  }

  h2 small {
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 400;
  }

  .name {
    max-width: 360px;
  }

  .repos {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  .repos li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 42px;
    padding: 0 8px 0 14px;
    color: var(--muted);
  }

  .repos li + li {
    border-top: 1px solid var(--border);
  }

  .repos b {
    color: var(--text);
    font-weight: 500;
  }

  .repos .mono {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .repos li:last-child {
    padding: 0;
  }

  .add {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 42px;
    padding: 0 14px;
    border: 0;
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
    background: none;
    color: var(--accent-ink);
    font-weight: 500;
    text-align: left;
  }

  .repos li:only-child .add {
    border-radius: var(--radius-lg);
  }

  .add:hover,
  .add[aria-expanded="true"] {
    background: var(--hover);
  }

  .line {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .grow {
    flex: 1;
    min-width: 0;
  }

  .danger-zone {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    padding: 12px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .danger-zone b,
  .danger-zone p {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .danger-zone b {
    font-weight: 600;
  }

  .delete {
    color: var(--danger);
  }

  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    height: 40px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
  }

  .search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    background: none;
    color: var(--text);
  }

  .search input:focus-visible {
    outline: none;
  }

  .candidates {
    margin: 0;
    padding: 6px;
    list-style: none;
  }

  .candidates label {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 34px;
    padding: 0 8px;
    border-radius: var(--radius);
    cursor: pointer;
  }

  .candidates label:hover {
    background: var(--hover);
  }

  .candidates input {
    margin: 0;
  }

  .candidates .name {
    flex-shrink: 0;
    font-weight: 500;
  }

  .candidates .mono {
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .none {
    padding: 8px;
    color: var(--muted);
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
    background: rgb(10 12 20 / 0.28);
  }

  .question p {
    margin: 0 0 16px;
    overflow-wrap: anywhere;
  }

  .answers {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
