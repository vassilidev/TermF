<script lang="ts">
  import { t } from "$lib/i18n";
  import { ALL_REPOS } from "$lib/nav-logic";
  import { app } from "$lib/state.svelte";
  import { chooseRepo, chosenRepo, currentProject } from "$lib/files.svelte";

  /** The views' common repository: all of a project's when it has several, or one; every project's are grouped. */
  const project = $derived(currentProject());
</script>

<select class="field" aria-label={t("filesRepo")} value={chosenRepo()} onchange={(e) => chooseRepo(e.currentTarget.value)}>
  {#if !chosenRepo()}
    <option value="">{t("noRepos")}</option>
  {/if}
  {#if project}
    {#if project.repos.length > 1}
      <option value={ALL_REPOS}>{t("mrAll")}</option>
    {/if}
    {#each project.repos as repo (repo.path)}
      <option value={repo.path}>{repo.name}</option>
    {/each}
  {:else}
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
  {/if}
</select>
