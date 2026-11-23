<script lang="ts">
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import GitPullRequest from "@lucide/svelte/icons/git-pull-request";
  import Avatar from "$lib/ui/Avatar.svelte";
  import { t } from "$lib/i18n";
  import { named, prName, soloNames, stageNames, stages } from "$lib/run-logic";
  import { asks, type TeamRun } from "$lib/team-run-logic";
  import { waitTitle } from "$lib/team.svelte";
  import type { Run } from "$lib/tickets.svelte";

  /**
   * Where a run is, on one line of a list: what waits for the user, else who works and at which stage, else its open
   * pull request. Nothing for a run at rest with nothing to say.
   */
  let { run, team }: { run: Run; team?: TeamRun } = $props();

  const active = $derived(team?.state === "running" || team?.state === "waiting");
  const wait = $derived(team ? asks(team)[0] : undefined);
  const roles = $derived(
    active ? [...new Set(team!.steps.filter((s) => s.state === "running" && s.kind !== "foreach" && s.role).map((s) => s.role!))] : [],
  );
  const stage = $derived.by(() => {
    if (!active || !team) return "";
    const now = stages(run, team).find((s) => s.state === "working" || s.state === "waiting");
    const key = now && (named(stageNames, now.id) ?? named(soloNames, now.id));
    if (!now || !key) return now?.id ?? "";
    const tasks = team.steps.filter((s) => s.kind === "task");
    return now.id === "tasks" && tasks.length
      ? `${t(key)} · ${t("uiRunTasksDone", { done: tasks.filter((s) => s.state === "done").length, total: tasks.length })}`
      : t(key);
  });
  const pr = $derived(run.prUrl && !run.prState && run.status === "delivered" ? prName(run.prUrl) : "");
</script>

{#if wait}
  {@const said = waitTitle(wait)}
  <span class="progress waits" title={said}><BellRing size={13} aria-hidden="true" /><span class="text">{said}</span></span>
{:else if stage}
  <span class="progress">
    {#if roles.length}
      <span class="who" aria-label={roles.join(", ")}>
        {#each roles.slice(0, 3) as role (role)}<Avatar {role} size="sm" status="working" />{/each}
      </span>
    {/if}
    <span class="text">{stage}</span>
  </span>
{:else if pr}
  <span class="progress"><GitPullRequest size={13} aria-hidden="true" /><span class="text">{t("uiTicketsPrOpen", { pr })}</span></span>
{/if}

<style>
  .progress {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--soft);
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  .text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .progress > :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .waits {
    color: var(--text);
    font-weight: 500;
  }

  .waits > :global(svg) {
    color: var(--warning);
  }

  .who {
    display: inline-flex;
    gap: 4px;
    margin-right: 2px;
  }
</style>
