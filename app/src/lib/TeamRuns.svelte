<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import PenLine from "@lucide/svelte/icons/pen-line";
  import Play from "@lucide/svelte/icons/play";
  import Ticket from "@lucide/svelte/icons/ticket";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Users from "@lucide/svelte/icons/users";
  import TeamOptions from "$lib/TeamOptions.svelte";
  import { stepName, ticketKey } from "$lib/TeamRunDetail.svelte";
  import { runSession } from "$lib/RunView.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import StatusPill, { type Status } from "$lib/ui/StatusPill.svelte";
  import { usd } from "$lib/agents.svelte";
  import { showRun } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { shown } from "$lib/run-logic";
  import { app } from "$lib/state.svelte";
  import { budgetError, current, type TeamRun } from "$lib/team-run-logic";
  import { scopedRuns, team, upsertRun } from "$lib/team.svelte";
  import { showTickets, tickets } from "$lib/tickets.svelte";

  const runs = $derived(scopedRuns());

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

  /** A ticket run by default when there is one, else a request written here. */
  let mode = $state<"run" | "text" | null>(null);
  const shownMode = $derived(mode ?? (free.length ? "run" : "text"));
  let text = $state("");
  let pickedProject = $state("");
  let pickedRepo = $state("");
  const projects = $derived(app.workspace?.projects ?? []);
  const requestProject = $derived(app.project ?? (pickedProject || projects[0]?.id || ""));
  const repos = $derived(projects.find((p) => p.id === requestProject)?.repos ?? []);
  const requestRepo = $derived(repos.find((r) => r.name === pickedRepo)?.name ?? repos[0]?.name ?? "");

  async function launch() {
    const request = shownMode === "text";
    if (request ? !text.trim() || !requestProject : !chosen) return;
    busy = true;
    error = "";
    try {
      const options = { workflow: workflow || undefined, budget: budget ?? undefined };
      const run = await invoke<TeamRun>(
        "team_start",
        request
          ? { request: { text: text.trim(), project: requestProject, repo: requestRepo || undefined }, options }
          : { run: chosen?.id, options },
      );
      upsertRun(run);
      team.launching = false;
      showRun(run.run);
      workflow = "";
      budget = null;
      picked = null;
      text = "";
      mode = null;
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
    <div class="segmented" role="radiogroup" aria-label={t("teamRunsLaunch")}>
      <button type="button" role="radio" aria-checked={shownMode === "run"} onclick={() => (mode = "run")}>
        <Ticket size={14} aria-hidden="true" />{t("ctoEntryFromRun")}
      </button>
      <button type="button" role="radio" aria-checked={shownMode === "text"} onclick={() => (mode = "text")}>
        <PenLine size={14} aria-hidden="true" />{t("ctoEntryFromText")}
      </button>
    </div>
    {#if shownMode === "text"}
      <label class="labelled request">
        <span>{t("ctoEntryText")}</span>
        <textarea class="field" rows="5" placeholder={t("ctoEntryTextHint")} bind:value={text}></textarea>
      </label>
      <p class="help">{t("ctoEntryTextHelp")}</p>
      {#if app.project === null || repos.length > 1}
        <div class="where">
          {#if app.project === null}
            <label class="labelled">
              <span>{t("ctoEntryProject")}</span>
              <select class="field" value={requestProject} onchange={(e) => (pickedProject = e.currentTarget.value)}>
                {#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
              </select>
            </label>
          {/if}
          {#if repos.length > 1}
            <label class="labelled">
              <span>{t("ctoEntryRepo")}</span>
              <select class="field" value={requestRepo} onchange={(e) => (pickedRepo = e.currentTarget.value)}>
                {#each repos as r (r.path)}<option value={r.name}>{r.name}</option>{/each}
              </select>
            </label>
          {/if}
        </div>
      {/if}
      {#if requestProject}<TeamOptions project={requestProject} bind:workflow bind:budget />{/if}
      {#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
      <div class="buttons">
        {#if runs.length}<button class="btn sm" onclick={() => (team.launching = false)}>{t("cancel")}</button>{/if}
        <button class="btn sm primary" disabled={busy || !text.trim() || !requestProject || !!budgetError(budget)} onclick={launch}>
          {#if busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Play size={14} aria-hidden="true" />{/if}
          {t("teamRunsLaunch")}
        </button>
      </div>
    {:else if chosen}
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

<div class="pane">
  {#if team.launching}
    <div class="alone">{@render form()}</div>
  {:else if !runs.length}
    <EmptyState icon={Users} title={t("teamRunsEmpty")} text={t("teamRunsEmptyHelp")}>
      <button class="btn" onclick={() => (team.launching = true)}>
        <Play size={14} aria-hidden="true" />{t("teamRunsLaunch")}
      </button>
    </EmptyState>
  {:else}
    <ul class="runs" aria-label={t("teamRunsTab")}>
      {#each runs as run (run.run)}
        {@const ticket = tickets.runs.find((r) => r.id === run.run)}
        {@const pill = ticket ? shown(ticket, run) : null}
        {@const now = steps(run)}
        <li>
          <button class="pick" onclick={() => showRun(run.run)}>
            <span class="name"><span class="mono">{ticketKey(run.target.ticket)}</span> {run.target.title}</span>
            <span class="desc">
              {run.workflow} · {usd(run.cost)}{run.budget !== null ? ` / ${usd(run.budget)}` : ""}{now ? ` · ${now}` : ""}
            </span>
          </button>
          {#if pill}<StatusPill status={pill.status as Status} label={pill.label ? t(pill.label) : undefined} />{/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

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

  /* A run's row opens its page. */
  .runs {
    max-width: 1100px;
    margin: 0;
    padding: 8px 12px 32px;
    list-style: none;
  }

  .runs li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 10px;
    border-bottom: 1px solid var(--border);
  }

  .runs li:hover {
    background: var(--hover);
  }

  .pick {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    padding: 8px 0;
    border: 0;
    background: none;
    color: var(--text);
    text-align: left;
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

  .launch .request {
    max-width: none;
  }

  textarea.field {
    height: auto;
    padding: 6px 8px;
    resize: vertical;
  }

  .segmented {
    align-self: flex-start;
  }

  .segmented button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .where {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 12px;
  }

  .where .labelled {
    width: 200px;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
