<script lang="ts" module>
  /** The plans ignored in this launch, by their text: a conversation drawn again keeps them folded. */
  const ignored = $state<Record<string, true>>({});
</script>

<script lang="ts">
  import BrushCleaning from "@lucide/svelte/icons/brush-cleaning";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import Button from "$lib/ui/Button.svelte";
  import { t } from "$lib/i18n";
  import { shortPath } from "$lib/state.svelte";
  import { scopeRepos } from "$lib/files.svelte";
  import { planOf, planTarget } from "$lib/git-logic";
  import { proposeCleanup } from "$lib/git.svelte";

  /**
   * A cleanup the git expert proposed in a `termf-cleanup` block: what it would remove, then Apply (the Git view's
   * cleanup runs it, asking first for whatever loses work), Modify (the same selection, to change) or Ignore. A plan
   * that does not name a repository of the project is shown, never applied: its names could match another repository.
   */
  let { text }: { text: string } = $props();

  const plan = $derived(planOf(text));
  const repo = $derived(planTarget(plan.repo, scopeRepos().map((r) => r.path)));
</script>

{#if ignored[text]}
  <p class="folded"><BrushCleaning size={13} aria-hidden="true" />{t("gitclProposalTitle")} · {t("gitclProposalIgnore")}</p>
{:else}
  <div class="proposal" role="group" aria-label={t("gitclProposalTitle")} data-testid="git-proposal">
    <p class="title"><BrushCleaning size={15} aria-hidden="true" />{t("gitclProposalTitle")}</p>
    {#if plan.keys.length}
      <ul>
        {#each plan.keys as key (key)}
          <li>
            {#if key.startsWith("w:")}<FolderGit2 size={13} aria-hidden="true" />{:else}<GitBranch size={13} aria-hidden="true" />{/if}
            <span class="mono">{key.startsWith("w:") ? shortPath(key.slice(2)) : key.slice(2)}</span>
          </li>
        {/each}
      </ul>
      {#if !repo}<p class="muted">{t("gitclPlanNoRepo")}</p>{/if}
      <div class="answers">
        <Button size="sm" variant="quiet" onclick={() => (ignored[text] = true)}>{t("gitclProposalIgnore")}</Button>
        {#if repo}
          {@const target = repo}
          <Button size="sm" onclick={() => proposeCleanup(target, plan.keys, false)}>{t("gitclProposalEdit")}</Button>
          <Button size="sm" variant="primary" onclick={() => proposeCleanup(target, plan.keys, true)}>{t("gitclProposalApply")}</Button>
        {/if}
      </div>
    {:else}
      <p class="muted">{t("gitclProposalEmpty")}</p>
    {/if}
  </div>
{/if}

<style>
  .proposal {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 6px 0;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--raised);
    box-shadow: var(--shadow-1);
  }

  .title,
  .folded {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-weight: 600;
  }

  .title :global(svg) {
    color: var(--accent-ink);
  }

  .folded {
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--soft);
  }

  .mono {
    overflow: hidden;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .answers {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
</style>
