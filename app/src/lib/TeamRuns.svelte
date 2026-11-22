<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Play from "@lucide/svelte/icons/play";
  import Ticket from "@lucide/svelte/icons/ticket";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Users from "@lucide/svelte/icons/users";
  import TeamOptions from "$lib/TeamOptions.svelte";
  import TeamRunDetail, { runPill, stepName } from "$lib/TeamRunDetail.svelte";
  import { runSession } from "$lib/TicketRun.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { usd } from "$lib/agents.svelte";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import { budgetError, current, type TeamRun } from "$lib/team-run-logic";
  import { scopedRuns, showTeamRun, shownRun, team, upsertRun } from "$lib/team.svelte";
  import { showTickets, tickets } from "$lib/tickets.svelte";

  const runs = $derived(scopedRuns());
  const shown = $derived(shownRun());

  /** The ticket runs a team can start on: open, with a worktree, without a team yet. */
  const free = $derived(
    tickets.runs.filter(
      (r) =>
        r.status === "running" &&
        r.worktree &&
        (app.project === null || r.project === app.project) &&
        !team.runs.some((run) => run.run === r.id),
    ),
  );
  let picked = $state<number | null>(null);
  const chosen = $derived(free.find((r) => r.id === picked) ?? free[0]);
  let workflow = $state("");
  let budget = $state<number | null>(null);
  let busy = $state(false);
  let error = $state("");

  async function launch() {
    if (!chosen) return;
    busy = true;
    error = "";
    try {
      const run = await invoke<TeamRun>("team_start", {
        run: chosen.id,
        options: { workflow: workflow || undefined, budget: budget ?? undefined },
      });
      upsertRun(run);
      showTeamRun(run.run);
      workflow = "";
      budget = null;
      picked = null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  const steps = (run: TeamRun) => current(run).map(stepName).join(", ");
</script>

{#snippet form()}
  <section class="launch" aria-labelledby="team-launch">
    <h2 id="team-launch">{t("teamRunsLaunch")}</h2>
    <p class="help">{t("teamRunsHow")}</p>
    {#if chosen}
      <label class="labelled">
        <span>{t("teamRunsTicketRun")}</span>
        <select class="field" value={chosen.id} onchange={(e) => (picked = Number(e.currentTarget.value))}>
          {#each free as r (r.id)}<option value={r.id}>{r.ticket.key} · {r.title}</option>{/each}
        </select>
      </label>
      <TeamOptions project={chosen.project} bind:workflow bind:budget />
      {#if runSession(chosen)}
        <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{t("teamRunsSessionOpen")}</p>
      {/if}
      {#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
      <div class="buttons">
        {#if runs.length}<button class="btn sm" onclick={() => (team.launching = false)}>{t("cancel")}</button>{/if}
        <button class="btn sm primary" disabled={busy || !!budgetError(budget)} onclick={launch}>
          {#if busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Play size={14} aria-hidden="true" />{/if}
          {t("teamRunsLaunch")}
        </button>
      </div>
    {:else}
      <p class="notice">{t("teamRunsNoTicketRun")}</p>
      <div class="buttons">
        {#if runs.length}<button class="btn sm" onclick={() => (team.launching = false)}>{t("cancel")}</button>{/if}
        <button class="btn sm" onclick={showTickets}><Ticket size={14} aria-hidden="true" />{t("showTickets")}</button>
      </div>
    {/if}
  </section>
{/snippet}

{#if !runs.length}
  <div class="pane">
    {#if team.launching}
      <div class="alone">{@render form()}</div>
    {:else}
      <EmptyState icon={Users} title={t("teamRunsEmpty")} text={t("teamRunsEmptyHelp")}>
        <button class="btn" onclick={() => (team.launching = true)}>
          <Play size={14} aria-hidden="true" />{t("teamRunsLaunch")}
        </button>
      </EmptyState>
    {/if}
  </div>
{:else}
  <div class="split">
    <ul class="list" aria-label={t("teamRunsTab")}>
      {#each runs as run (run.run)}
        {@const [status, label] = runPill(run.state)}
        {@const now = steps(run)}
        <li class="row" class:current={run === shown && !team.launching}>
          <button
            class="pick"
            aria-current={run === shown && !team.launching}
            onclick={() => Object.assign(team, { selected: run.run, launching: false })}
          >
            <span class="name"><span class="mono">{run.target.ticket}</span> {run.target.title}</span>
            <span class="desc">
              {run.workflow} · {usd(run.cost)}{run.budget !== null ? ` / ${usd(run.budget)}` : ""}{now ? ` · ${now}` : ""}
            </span>
          </button>
          <StatusPill {status} {label} />
        </li>
      {/each}
    </ul>
    <section class="detail" aria-live="polite">
      {#if team.launching}
        {@render form()}
      {:else if shown}
        {#key shown.run}<TeamRunDetail run={shown} />{/key}
      {/if}
    </section>
  </div>
{/if}

<style>
  .pane {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .alone {
    max-width: 640px;
    margin: 32px auto;
    padding: 0 24px;
  }

  .launch {
    display: flex;
    flex-direction: column;
    gap: 12px;
    font-size: var(--text-sm);
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .help {
    margin: 0;
    color: var(--muted);
    line-height: 1.5;
  }

  .launch .labelled {
    max-width: 420px;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
