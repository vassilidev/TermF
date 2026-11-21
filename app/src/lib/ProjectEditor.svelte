<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SvelteSet } from "svelte/reactivity";
  import { lang, t } from "$lib/i18n";
  import { app, type Project } from "$lib/state.svelte";

  let { project, onclose }: { project?: Project; onclose: () => void } = $props();

  // A new editor is mounted per project, so the prop only seeds the form.
  // svelte-ignore state_referenced_locally
  let name = $state(project?.name ?? "");
  // svelte-ignore state_referenced_locally
  const chosen = new SvelteSet(project?.repos.map((r) => r.path));
  let filter = $state("");
  let confirming = $state(false);
  let error = $state("");

  // The project's own repositories first, even if discovery no longer finds them.
  const candidates = $derived.by(() => {
    const own = project?.repos ?? [];
    const others = (app.workspace?.repos ?? [])
      .filter((r) => !own.some((o) => o.path === r.path))
      .sort((a, b) => a.name.localeCompare(b.name, lang, { sensitivity: "base" }));
    const query = filter.toLowerCase();
    return [...own, ...others].filter((r) => r.path.toLowerCase().includes(query));
  });

  async function run(command: string, args: Record<string, unknown>) {
    try {
      app.workspace!.projects = await invoke<Project[]>(command, args);
      onclose();
    } catch (e) {
      error = String(e);
    }
  }

  function save(e: SubmitEvent) {
    e.preventDefault();
    run("project_save", { id: project?.id ?? null, name, repos: [...chosen] });
  }
</script>

<form
  class="editor"
  onsubmit={save}
  aria-label={project ? t("editProject", { name: project.name }) : t("newProject")}
>
  <input
    class="field"
    bind:value={name}
    placeholder={t("projectName")}
    aria-label={t("projectName")}
    {@attach (el) => el.focus()}
  />
  <input
    class="field"
    type="search"
    bind:value={filter}
    placeholder={t("filter")}
    aria-label={t("filter")}
  />
  <ul class="repos">
    {#each candidates as repo (repo.path)}
      <li>
        <label class="row" title={repo.path}>
          <input
            type="checkbox"
            checked={chosen.has(repo.path)}
            onchange={(e) =>
              e.currentTarget.checked ? chosen.add(repo.path) : chosen.delete(repo.path)}
          />
          <span class="name">{repo.name}</span>
        </label>
      </li>
    {:else}
      <li class="none">{t("noRepos")}</li>
    {/each}
  </ul>

  {#if error}
    <p class="notice error" role="alert">{error}</p>
  {/if}

  {#if confirming && project}
    <p class="confirm" role="alert">{t("confirmDeleteProject", { name: project.name })}</p>
    <div class="buttons">
      <button type="button" class="btn sm" onclick={() => (confirming = false)} {@attach (el) => el.focus()}>
        {t("cancel")}
      </button>
      <button type="button" class="btn sm danger" onclick={() => run("project_delete", { id: project.id })}>
        {t("delete")}
      </button>
    </div>
  {:else}
    <div class="buttons">
      {#if project}
        <button type="button" class="btn sm quiet delete" onclick={() => (confirming = true)}>
          {t("delete")}
        </button>
      {/if}
      <button type="button" class="btn sm" onclick={onclose}>{t("cancel")}</button>
      <button type="submit" class="btn sm primary" disabled={!name.trim()}>{t("save")}</button>
    </div>
  {/if}
</form>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 2px 0 8px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-1);
  }

  .repos {
    max-height: 220px;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 28px;
    padding: 0 6px;
    border-radius: var(--radius);
    cursor: pointer;
  }

  .row:hover {
    background: var(--hover);
  }

  .row input {
    margin: 0;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .none {
    padding: 4px 6px;
    color: var(--muted);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .delete {
    margin-right: auto;
    padding: 0 4px;
    color: var(--danger);
  }

  .confirm {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
