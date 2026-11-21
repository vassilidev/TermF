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

<form class="editor" onsubmit={save}>
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
      <li class="muted">{t("noRepos")}</li>
    {/each}
  </ul>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if confirming && project}
    <p class="confirm" role="alert">{t("confirmDeleteProject", { name: project.name })}</p>
    <div class="buttons">
      <button type="button" onclick={() => (confirming = false)} {@attach (el) => el.focus()}>
        {t("cancel")}
      </button>
      <button type="button" class="danger" onclick={() => run("project_delete", { id: project.id })}>
        {t("delete")}
      </button>
    </div>
  {:else}
    <div class="buttons">
      {#if project}
        <button type="button" class="delete" onclick={() => (confirming = true)}>
          {t("delete")}
        </button>
      {/if}
      <button type="button" onclick={onclose}>{t("cancel")}</button>
      <button type="submit" class="primary" disabled={!name.trim()}>{t("save")}</button>
    </div>
  {/if}
</form>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 2px 0 8px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
  }

  .field {
    width: 100%;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    color: var(--text);
    font: inherit;
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
    gap: 8px;
    padding: 3px 6px;
    border-radius: 6px;
    cursor: pointer;
  }

  .row:hover {
    background: var(--hover);
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  button {
    padding: 5px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
  }

  .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .delete {
    margin-right: auto;
    border-color: transparent;
    background: none;
    color: var(--danger);
  }

  .danger {
    border-color: var(--danger);
    color: var(--danger);
    font-weight: 600;
  }

  .confirm,
  .error {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .error {
    color: var(--danger);
  }
</style>
