<script module lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { focusSession } from "$lib/commands";
  import { app, type Session } from "$lib/state.svelte";
  import { liveSession } from "$lib/tickets-logic";
  import { tickets, type Run } from "$lib/tickets.svelte";

  export function runSession(run: Run): Session | undefined {
    return liveSession(run, app.sessions);
  }

  /** Shows the run's tile. `session-opened` may come after the answer that started the run: then the list is asked. */
  export async function openTile(run: Run) {
    if (!runSession(run)) {
      const list = await invoke<Session[]>("pty_list").catch(() => []);
      for (const s of list) if (!app.sessions.some((o) => o.id === s.id)) app.sessions.push(s);
    }
    const session = runSession(run);
    if (session) await focusSession(session.id);
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import Archive from "@lucide/svelte/icons/archive";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import BrushCleaning from "@lucide/svelte/icons/brush-cleaning";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleDashed from "@lucide/svelte/icons/circle-dashed";
  import CircleStop from "@lucide/svelte/icons/circle-stop";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import FolderInput from "@lucide/svelte/icons/folder-input";
  import GitMerge from "@lucide/svelte/icons/git-merge";
  import GitPullRequestClosed from "@lucide/svelte/icons/git-pull-request-closed";
  import Images from "@lucide/svelte/icons/images";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import Send from "@lucide/svelte/icons/send";
  import ShieldQuestionMark from "@lucide/svelte/icons/shield-question-mark";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Users from "@lucide/svelte/icons/users";
  import BaseUpdate from "$lib/BaseUpdate.svelte";
  import CiChecks from "$lib/CiChecks.svelte";
  import Delivery from "$lib/Delivery.svelte";
  import TeamOptions from "$lib/TeamOptions.svelte";
  import TeamRunDetail, { ticketKey } from "$lib/TeamRunDetail.svelte";
  import WaitCards from "$lib/WaitCards.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import StatusPill, { type Status } from "$lib/ui/StatusPill.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { usd } from "$lib/agents.svelte";
  import { closeRun, showReview } from "$lib/commands";
  import { reviewOf } from "$lib/visual.svelte";
  import { ago, lang, t } from "$lib/i18n";
  import { expected, named, prName, repoRuns, shown, soloNames, stageNames, stages, type RepoState, type StageState } from "$lib/run-logic";
  import { repoName, shortPath } from "$lib/state.svelte";
  import { TRIAGE, budgetError, progress, type TeamRun } from "$lib/team-run-logic";
  import { team, upsertRun } from "$lib/team.svelte";
  import { readyToClean, refusal, resumable } from "$lib/tickets-logic";
  import { openUrl, upsert, type Resumed, type RunEvent } from "$lib/tickets.svelte";

  /** A ticket's run on one page, whether its team works on it or not. */
  let { id }: { id: number } = $props();

  const run = $derived(tickets.runs.find((r) => r.id === id));
  const teamRun = $derived(team.runs.find((r) => r.run === id));
  const review = $derived(reviewOf(id));

  type Step =
    | null
    | { kind: "resumed"; warning: string }
    | { kind: "close" }
    | { kind: "refused"; detail: string; cleanup: boolean; uncommitted: boolean }
    | { kind: "force" }
    | { kind: "team" }
    | { kind: "resume-team" };

  let step = $state<Step>(null);
  let busy = $state<"" | "close" | "resume" | "cleanup" | "team" | "stop">("");
  let error = $state("");
  let done = $state("");
  let delivering = $state(false);
  /** The run of another repository whose delivery is open. */
  let deliveringRepo = $state<number | null>(null);
  let stopping = $state(false);
  let more = $state(false);
  // The page's own width, not the window's: a run beside another view is narrow too.
  let width = $state(0);
  const narrow = $derived(width > 0 && width < 1000);
  // Its entries move between the header and the menu: an open menu would be left stale.
  $effect(() => {
    void narrow;
    untrack(() => (more = false));
  });
  let moreTrigger = $state<HTMLButtonElement>();
  let teamWorkflow = $state("");
  let teamBudget = $state<number | null>(null);
  let events = $state<RunEvent[]>([]);

  const session = $derived(run && runSession(run));
  const open = $derived(run?.status === "running" || run?.status === "delivered");
  const active = $derived(teamRun?.state === "running" || teamRun?.state === "waiting");
  const pill = $derived(run && shown(run, teamRun));
  const wants = $derived(run ? expected(run, teamRun) : null);
  const timeline = $derived(run ? stages(run, teamRun) : []);
  // A change over several repositories: each with its own pull request.
  const repos = $derived(repoRuns(tickets.runs, id));
  const repoLabel: Record<RepoState, [Status, Parameters<typeof t>[0]]> = {
    working: ["running", "teams2RepoWorking"],
    open: ["delivered", "teams2RepoOpen"],
    merged: ["done", "teams2RepoMerged"],
    closed: ["closed", "teams2RepoClosed"],
    gone: ["closed", "teams2RepoGone"],
  };
  const spent = $derived(teamRun && progress(teamRun));
  const triaging = $derived(active && teamRun?.steps.some((s) => s.key === TRIAGE && s.state === "running"));
  /** The team's own delivery card offers it while the team waits on it, or once it is done. */
  const canDeliver = $derived(open && !active && wants !== "deliver" && !delivering);
  const canResumeTeam = $derived(!!teamRun && !active && teamRun.state !== "done");
  const canResume = $derived(!!run && !teamRun && resumable(run) && !session);
  const canTeam = $derived(!!run && !teamRun && run.status === "running" && !!run.worktree);
  const canCopy = $derived(!!run?.worktree && run.status !== "closed");
  /** Every entry but the team's resumption needs a run not closed yet. */
  const hasMenu = $derived(!!run && (run.status !== "closed" || canResumeTeam));

  /** How long a run's session may stay "starting" before the trust question is the likely reason. */
  const TRUST_DELAY = 6000;
  /** Claude Code asks whether to trust a folder before any hook runs, so its session stays "starting". */
  let untrusted = $state(false);
  $effect(() => {
    untrusted = false;
    if (session?.status !== "starting") return;
    const timer = setTimeout(() => (untrusted = true), TRUST_DELAY);
    return () => clearTimeout(timer);
  });

  let eventsRequest = 0;

  // Only the latest request fills the log: an older answer may come last.
  $effect(() => {
    void run?.updated;
    const request = ++eventsRequest;
    invoke<RunEvent[]>("tickets_events", { id }).then(
      (list) => {
        if (request === eventsRequest) events = [...list].reverse();
      },
      (e) => {
        if (request === eventsRequest) error = String(e);
      },
    );
  });

  const short = (commit: string) => commit.slice(0, 7);

  function describe(e: RunEvent): string {
    switch (e.kind) {
      case "worktree_created":
        return t("eventWorktree", { branch: e.branch, base: e.base });
      case "session_opened":
        return t("eventSession");
      case "session_resumed":
        return t("ticketsUiEventResumed");
      case "committed":
        return t("eventCommitted", { commit: short(e.commit), message: e.message.split("\n")[0] });
      case "pushed":
        return t("eventPushed", { branch: e.branch, commit: short(e.commit) });
      case "merged":
        return t("greenfieldEventMerged", { into: e.into, commit: short(e.commit) });
      case "pr_opened":
        return t("eventPr", { url: e.url });
      case "pr_merged":
        return t("ticketsUiEventMerged");
      case "pr_closed":
        return t("ticketsUiEventPrClosed");
      case "ticket_commented":
        return t("eventCommented");
      case "ticket_moved":
        return t("ticketsUiEventMoved", { to: e.to });
      case "closed":
        return e.dropped.length ? t("eventClosedForce", { dropped: e.dropped.join(", ") }) : t("eventClosed");
      case "failed":
        return t("eventFailed", { error: e.error });
      case "setup": {
        const failed = e.steps.filter((s) => s.outcome === "failed" || s.outcome === "refused").length;
        const count = e.steps.length;
        const text = failed ? t("wtEventSetupFailed", { count, failed }) : t("wtEventSetup", { count });
        return e.saved ? text : `${text} · ${t("wtDetected")}`;
      }
      case "teardown":
        return t("wtEventTeardown");
    }
  }

  /** Today's events by their time, older ones by their date; the full date on hover. */
  function when(at: number): [string, string] {
    const date = new Date(at * 1000);
    const today = date.toDateString() === new Date().toDateString();
    const compact = date.toLocaleString(lang, today ? { timeStyle: "short" } : { day: "numeric", month: "short" });
    return [compact, date.toLocaleString(lang, { dateStyle: "short", timeStyle: "short" })];
  }

  const stageLabel = (stage: string) => {
    const key = named(stageNames, stage) ?? named(soloNames, stage);
    return key ? t(key) : stage;
  };
  const stageStatus: Record<StageState, [typeof CircleCheck, Parameters<typeof t>[0]]> = {
    done: [CircleCheck, "teamRunsStepDone"],
    working: [LoaderCircle, "teamRunsStepRunning"],
    waiting: [BellRing, "statusWaiting"],
    failed: [CircleX, "teamRunsStepFailed"],
    pending: [CircleDashed, "teamRunsStepPending"],
  };

  async function act(kind: typeof busy, work: () => Promise<void>) {
    busy = kind;
    error = "";
    done = "";
    try {
      await work();
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  /**
   * Closing and cleaning up refuse alike while work is not pushed. `tickets_cleanup` takes no force: it checks the
   * pull request, then closes as `tickets_close` does without force, so a cleanup forced is a close forced.
   */
  const close = (force: boolean, cleanup = false) =>
    act(cleanup ? "cleanup" : "close", async () => {
      if (!run) return;
      try {
        upsert(await invoke<Run>(cleanup ? "tickets_cleanup" : "tickets_close", { id: run.id, force }));
      } catch (e) {
        const refused = refusal(e);
        if (force || refused.kind === "error") throw refused.message;
        step = { kind: "refused", detail: refused.message, cleanup, uncommitted: refused.kind === "uncommitted" };
        return;
      }
      step = null;
      done = t(cleanup ? "ticketsUiCleaned" : "runClosed");
    });

  /** The run of another repository whose pull request was merged or closed: its worktree goes, the others stay. */
  const cleanRepo = (other: Run) =>
    act("cleanup", async () => {
      upsert(await invoke<Run>("tickets_cleanup", { id: other.id, force: false }));
      done = t("ticketsUiCleaned");
    });

  /** Without a warning, straight to the tile, like a launch. */
  const resume = () =>
    act("resume", async () => {
      step = null;
      const resumed = await invoke<Resumed>("tickets_resume", { id, cols: 120, rows: 32 });
      upsert(resumed.run);
      if (resumed.warning) step = { kind: "resumed", warning: resumed.warning };
      else await openTile(resumed.run);
    });

  const startTeam = () =>
    act("team", async () => {
      const options = { workflow: teamWorkflow || undefined, budget: teamBudget ?? undefined };
      upsertRun(await invoke<TeamRun>("team_start", { run: id, options }));
      step = null;
    });

  const resumeTeam = () =>
    act("team", async () => {
      upsertRun(await invoke<TeamRun>("team_start", { run: id, options: { budget: teamBudget ?? undefined } }));
      step = null;
      teamBudget = null;
    });

  const stop = () =>
    act("stop", async () => {
      for (const r of await invoke<TeamRun[]>("team_stop", { run: id })) upsertRun(r);
    });

  async function copyPath() {
    if (!run?.worktree) return;
    try {
      await navigator.clipboard.writeText(run.worktree);
      done = t("ux3t2WorktreeCopied");
    } catch (e) {
      error = String(e);
    }
  }

  /** A menu entry closes the menu first: what it opens takes the page. */
  const pick = (go: () => void) => () => {
    more = false;
    go();
  };
</script>

{#snippet failure()}
  {#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
{/snippet}

<div class="run" bind:clientWidth={width}>
  <ViewHeader title={run?.title ?? teamRun?.target.title ?? ""} context={run ? ticketKey(run.ticket.key) : ""} mono>
    {#snippet lead()}
      <button class="icon-btn back" onclick={closeRun} aria-label={t("ux3t2Back")} title={t("ux3t2Back")}>
        <ArrowLeft size={16} aria-hidden="true" />
      </button>
    {/snippet}
    {#if pill}<StatusPill status={pill.status as Status} label={pill.label ? t(pill.label) : undefined} hint={pill.hint && t(pill.hint)} />{/if}
    {#snippet actions()}
      <!-- A narrow page keeps the delivery and the stop in the header, the rest goes to the menu. -->
      {#if run && session && !narrow}
        <button class="btn sm" onclick={() => openTile(run)}>
          <SquareTerminal size={14} aria-hidden="true" />{t("runOpenTile")}
        </button>
      {/if}
      {#if review && !narrow}
        {@const id = review.id}
        <button class="btn sm" onclick={() => showReview(id)}>
          <Images size={14} aria-hidden="true" />{t("review5bTitle")}
        </button>
      {/if}
      {#if canDeliver}
        <button class="btn sm" onclick={() => ((delivering = true), (step = null))}>
          <Send size={14} aria-hidden="true" />{t("runDeliver")}
        </button>
      {/if}
      {#if active}
        <button class="btn sm quiet stop" onclick={() => (stopping = true)} disabled={!!busy}>
          <CircleStop size={14} aria-hidden="true" />{t("teamRunsStop")}
        </button>
      {/if}
      {#if run && (hasMenu || (narrow && (session || review)))}
        <button
          bind:this={moreTrigger}
          class="icon-btn"
          aria-label={t("ux3t2More")}
          title={t("ux3t2More")}
          aria-haspopup="dialog"
          aria-expanded={more}
          onclick={() => (more = !more)}
        >
          <Ellipsis size={16} aria-hidden="true" />
        </button>
      {/if}
    {/snippet}
  </ViewHeader>

  {#if run}
    <div class="page">
      <div class="grid">
        <div class="main">
          <ol class="timeline" aria-label={t("ux3t2Stages")}>
            {#each timeline as stage (stage.id)}
              {@const [Icon, said] = stageStatus[stage.state]}
              {@const label = stageLabel(stage.id)}
              <li class={stage.state} aria-current={stage.state === "waiting" || stage.state === "working" ? "step" : undefined} title={label}>
                <span class="st">
                  <Icon size={14} aria-hidden="true" /><span class="label">{label}</span><span class="sr-only">, {t(said)}</span>
                </span>
              </li>
            {/each}
          </ol>

          {#if done}<Banner kind="ok" ondismiss={() => (done = "")}>{done}</Banner>{/if}
          {#if teamRun?.note && !active}
            <p class="notice"><CircleAlert size={14} aria-hidden="true" />{teamRun.note}</p>
          {/if}
          {#if step === null || step.kind === "resumed"}{@render failure()}{/if}
          {#if untrusted}
            <Banner kind="warn" icon={ShieldQuestionMark} ondismiss={() => (untrusted = false)}>
              {t("ticketsUiTrust")}
              {#snippet action()}
                <button class="btn sm" onclick={() => openTile(run)}>
                  <SquareTerminal size={14} aria-hidden="true" />{t("runOpenTile")}
                </button>
              {/snippet}
            </Banner>
          {/if}

          {#if step?.kind === "resumed"}
            <section class="card ok-card" role="status">
              <p class="ok"><Check size={14} aria-hidden="true" />{t("ticketsUiResumed")}</p>
              <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{step.warning}</p>
              <div class="buttons">
                <button class="btn sm" onclick={() => (step = null)}>{t("dismiss")}</button>
                {#if session}
                  <button class="btn sm primary" onclick={() => openTile(run)}>
                    <SquareTerminal size={14} aria-hidden="true" />{t("runOpenTile")}
                  </button>
                {/if}
              </div>
            </section>
          {:else if step?.kind === "close"}
            <section class="card" role="alert">
              <span>{t("closeConfirm")}</span>
              {@render failure()}
              <div class="buttons">
                <button class="btn sm" onclick={() => (step = null)}>{t("cancel")}</button>
                <button class="btn sm primary" onclick={() => close(false)} disabled={!!busy}>
                  {#if busy === "close"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
                  {t("closeRun")}
                </button>
              </div>
            </section>
          {:else if step?.kind === "refused"}
            <section class="card warn" role="alert">
              <p class="refused">
                <TriangleAlert size={14} aria-hidden="true" />{t(
                  !step.cleanup ? "closeRefused" : step.uncommitted ? "ticketsUiCleanUncommitted" : "ticketsUiCleanUnpushed",
                )}
              </p>
              <p class="mono muted">{step.detail}</p>
              <div class="buttons">
                <button class="btn sm" onclick={() => (step = null)}>{t("cancel")}</button>
                {#if canDeliver}<button class="btn sm" onclick={() => ((step = null), (delivering = true))}>{t("runDeliver")}</button>{/if}
                <button class="btn sm" onclick={() => (step = { kind: "force" })}>{t("closeForce")}</button>
              </div>
            </section>
          {:else if step?.kind === "force"}
            <section class="card" role="alert">
              <span>{t("closeForceConfirm", { branch: run.branch ?? "", repo: repoName(run.repo) })}</span>
              {@render failure()}
              <div class="buttons">
                <button class="btn sm" onclick={() => (step = null)}>{t("cancel")}</button>
                <button class="btn sm danger" onclick={() => close(true)} disabled={!!busy}>
                  {#if busy === "close"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
                  {t("closeForceRun")}
                </button>
              </div>
            </section>
          {:else if step?.kind === "team"}
            <section class="card" aria-labelledby="team-title">
              <h3 id="team-title">{t("teamRunsLaunchLong")}</h3>
              <p class="help">{t("teamRunsHow")}</p>
              <TeamOptions project={run.project} bind:workflow={teamWorkflow} bind:budget={teamBudget} />
              {#if session}
                <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{t("teamRunsSessionOpen")}</p>
              {/if}
              {@render failure()}
              <div class="buttons">
                <button class="btn sm" onclick={() => (step = null)}>{t("cancel")}</button>
                <button class="btn sm primary" onclick={startTeam} disabled={!!busy || !!budgetError(teamBudget)}>
                  {#if busy === "team"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Users size={14} aria-hidden="true" />{/if}
                  {t("teamRunsLaunch")}
                </button>
              </div>
            </section>
          {:else if step?.kind === "resume-team" && teamRun}
            <section class="card" aria-labelledby="team-resume">
              <h3 id="team-resume">{t("ticketsUiResume")}</h3>
              <p class="help">{t("teamRunsResumeHelp")}</p>
              <TeamOptions project={run.project} resume spent={teamRun.cost} required={teamRun.state === "budget"} bind:budget={teamBudget} />
              {@render failure()}
              <div class="buttons">
                <button class="btn sm" onclick={() => (step = null)}>{t("cancel")}</button>
                <button
                  class="btn sm primary"
                  disabled={!!busy || !!budgetError(teamBudget, teamRun.cost, teamRun.state === "budget")}
                  onclick={resumeTeam}
                >
                  {#if busy === "team"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("ticketsUiResume")}
                </button>
              </div>
            </section>
          {/if}

          {#if delivering}<Delivery {run} onclose={() => (delivering = false)} />{/if}

          {#if wants === "clean"}
            {@const merged = run.prState === "merged"}
            <section class="card ok-card" aria-labelledby="ready-title">
              <p class="ok" id="ready-title">
                {#if merged}<GitMerge size={14} aria-hidden="true" />{:else}<GitPullRequestClosed size={14} aria-hidden="true" />{/if}
                {t(merged ? "ticketsUiMergedTitle" : "ticketsUiClosedTitle")}
              </p>
              <p>{t("ticketsUiCleanHelp")}</p>
              <div class="buttons">
                <button class="btn sm primary" onclick={() => close(false, true)} disabled={!!busy}>
                  {#if busy === "cleanup"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<BrushCleaning
                      size={14}
                      aria-hidden="true"
                    />{/if}
                  {t("ticketsUiClean")}
                </button>
              </div>
            </section>
          {/if}

          <BaseUpdate {run} team={teamRun} />

          {#if teamRun}
            {#if triaging}
              <p class="notice" role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" />{t("ctoEntryTriage")}</p>
            {/if}
            <WaitCards run={teamRun} />
            {#if repos.length}
              {@const prs = repos.filter((r) => r.run.prUrl).length}
              {@const other = repos.find((r) => r.run.id === deliveringRepo)?.run}
              <section class="card" aria-labelledby="run-repos">
                <div class="repos-head">
                  <h3 id="run-repos">{t("teams2Repos")}</h3>
                  <span class="muted">{t("teams2ReposSummary", { prs, total: repos.length })}</span>
                </div>
                <p class="help">{t("teams2ReposHelp")}</p>
                <ul class="repos">
                  {#each repos as { run: r, state } (r.id)}
                    {@const [status, label] = repoLabel[state]}
                    {@const name = repoName(r.repo)}
                    <li>
                      <span class="repo-name">
                        <b>{name}</b>
                        {#if r.id === id}<span class="muted">{t("teams2RepoOwn")}</span>{/if}
                      </span>
                      <StatusPill {status} label={t(label)} />
                      {#if r.prUrl}
                        {@const url = r.prUrl}
                        <button class="btn link" onclick={() => openUrl(url)} title={t("ticketsUiOpenLink")}>
                          {prName(url)}<ExternalLink size={12} aria-hidden="true" />
                        </button>
                      {/if}
                      <span class="repo-acts">
                        {#if r.status === "delivered" && r.prState && r.id !== id}
                          <button class="btn sm" onclick={() => cleanRepo(r)} disabled={!!busy} title={t("teams2CleanRepo", { repo: name })}>
                            <BrushCleaning size={14} aria-hidden="true" />{t("ticketsUiClean")}
                          </button>
                        {:else if (r.status === "running" || r.status === "delivered") && !r.prState && !active}
                          <button
                            class="btn sm"
                            title={t("teams2DeliverRepo", { repo: name })}
                            onclick={() => (r.id === id ? (delivering = true) : (deliveringRepo = r.id))}
                          >
                            <Send size={14} aria-hidden="true" />{t("runDeliver")}
                          </button>
                        {/if}
                      </span>
                    </li>
                  {/each}
                </ul>
              </section>
              {#if other}<Delivery run={other} onclose={() => (deliveringRepo = null)} />{/if}
            {/if}
            <TeamRunDetail run={teamRun} />
          {:else if open && !session}
            <p class="muted small">{t("runNoSession")}</p>
          {/if}
        </div>

        <aside class="side">
          <section class="panel">
            <dl class="kv">
              <dt>{t("runMetaTicket")}</dt>
              <dd>
                {#if run.ticket.url}
                  <button class="btn link" onclick={() => openUrl(run.ticket.url)} title={t("ticketsUiOpenLink")}>
                    {run.ticket.key}<ExternalLink size={12} aria-hidden="true" />
                  </button>
                {:else}
                  {ticketKey(run.ticket.key)}
                {/if}
              </dd>
              <dt>{t("runMetaRepo")}</dt>
              <dd title={shortPath(run.repo)}>{repoName(run.repo)}</dd>
              {#if run.branch}
                <dt>{t("runMetaBranch")}</dt>
                <dd class="mono">{run.branch}</dd>
              {/if}
              <dt>{t("runMetaPr")}</dt>
              <dd>
                {#if run.prUrl}
                  {@const url = run.prUrl}
                  <button class="btn link" onclick={() => openUrl(url)} title={t("ticketsUiOpenLink")}>
                    {prName(url)}<ExternalLink size={12} aria-hidden="true" />
                  </button>
                {:else}
                  <span class="muted">{t("ux3t2NotYet")}</span>
                {/if}
              </dd>
              <dt>{t("runMetaCreated")}</dt>
              <dd>{ago(run.created)}</dd>
            </dl>
          </section>

          {#if run.prUrl}
            <div class="panel ci"><CiChecks {run} {session} /></div>
          {/if}

          {#if teamRun && spent}
            <section class="panel" aria-labelledby="run-budget">
              <h3 id="run-budget">{t("chatBudget")}</h3>
              <p class="figures">
                <b>{usd(teamRun.cost)}</b> {t("teamRunsSpent")}
                <span class="muted">
                  · {teamRun.budget !== null && spent.left !== null
                    ? t("chatBudgetLeft", { left: usd(spent.left), budget: usd(teamRun.budget) })
                    : t("teamRunsNoBudget")}
                </span>
              </p>
              {#if spent.spent !== null}
                <div class="bar" class:full={spent.spent >= 1}><span style="width: {spent.spent * 100}%"></span></div>
              {/if}
              <p class="muted small">{t("teamRunsProgress", { over: spent.over, total: spent.total })}</p>
            </section>
          {/if}

          <section class="panel" aria-labelledby="run-journal">
            <h3 id="run-journal">{t("runJournal")}</h3>
            <ol class="journal">
              {#each events as event (event.id)}
                {@const [compact, full] = when(event.at)}
                <li class:failed={event.kind === "failed"}>
                  <time datetime={new Date(event.at * 1000).toISOString()} title={full}>{compact}</time>
                  <span>{describe(event)}</span>
                </li>
              {:else}
                <li class="muted">{t("runJournalEmpty")}</li>
              {/each}
            </ol>
          </section>
        </aside>
      </div>
    </div>
  {/if}
</div>

<Popover bind:open={more} trigger={moreTrigger} label={t("ux3t2More")} align="end" width={260}>
  {#if run}
    <ul class="menu">
      {#if session && narrow}
        <li><button class="row" onclick={pick(() => openTile(run))}><SquareTerminal size={14} aria-hidden="true" />{t("runOpenTile")}</button></li>
      {/if}
      {#if review && narrow}
        {@const id = review.id}
        <li><button class="row" onclick={pick(() => showReview(id))}><Images size={14} aria-hidden="true" />{t("review5bTitle")}</button></li>
      {/if}
      {#if canResumeTeam}
        <li><button class="row" onclick={pick(() => (step = { kind: "resume-team" }))}><RotateCcw size={14} aria-hidden="true" />{t("ticketsUiResume")}</button></li>
      {:else if canResume}
        <li><button class="row" disabled={!!busy} onclick={pick(resume)}><RotateCcw size={14} aria-hidden="true" />{t("ticketsUiResume")}</button></li>
      {/if}
      {#if canTeam}
        <li><button class="row" onclick={pick(() => (step = { kind: "team" }))}><Users size={14} aria-hidden="true" />{t("teamRunsLaunchLong")}</button></li>
      {/if}
      {#if readyToClean(run)}
        <li><button class="row" disabled={!!busy} onclick={pick(() => close(false, true))}><BrushCleaning size={14} aria-hidden="true" />{t("ticketsUiClean")}</button></li>
      {/if}
      {#if run.status !== "closed"}
        <li><button class="row" onclick={pick(() => (step = { kind: "close" }))}><Archive size={14} aria-hidden="true" />{t("runClose")}</button></li>
      {/if}
      {#if canCopy}
        <li><button class="row" onclick={pick(copyPath)}><FolderInput size={14} aria-hidden="true" />{t("ux3t2CopyWorktree")}</button></li>
      {/if}
    </ul>
  {/if}
</Popover>

<ConfirmDialog
  bind:open={stopping}
  title={t("teamRunsStopTitle", { ticket: teamRun?.target.ticket ?? "" })}
  confirm={t("teamRunsStopConfirm")}
  danger
  onconfirm={stop}
>
  <p>{t("teamRunsStopText")}</p>
</ConfirmDialog>

<style>
  .run {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }

  /* A long title gives way to the actions instead of pushing them off the row. */
  .run :global(.view-header h1) {
    flex-shrink: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .back {
    margin-left: -6px;
  }

  .stop {
    color: var(--danger);
  }

  .page {
    flex: 1;
    min-height: 0;
    padding: 20px 28px 40px;
    overflow-y: auto;
    container-type: inline-size;
  }

  .grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    align-items: start;
    gap: 20px;
    max-width: 1120px;
  }

  @container (max-width: 760px) {
    .grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .main,
  .side {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  .timeline {
    container: stages / inline-size;
    display: flex;
    align-items: center;
    margin: 0;
    padding: 12px 14px;
    overflow-x: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  .timeline li {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .timeline li:last-child {
    flex: 0 0 auto;
  }

  .timeline li:not(:last-child)::after {
    content: "";
    flex: 1;
    min-width: 14px;
    height: 1px;
    margin: 0 8px;
    background: var(--border-strong);
  }

  .st {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 6px;
    height: 26px;
    white-space: nowrap;
  }

  /* Too narrow for every name: the stages done or to come keep their icon, the current one its name. */
  @container stages (max-width: 640px) {
    li:not([aria-current]) .label {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }
  }

  .done .st {
    color: var(--text);
  }

  .done .st :global(svg) {
    color: var(--done);
  }

  .failed .st {
    color: var(--danger);
  }

  .waiting .st,
  .working .st {
    padding: 0 10px 0 7px;
    border-radius: var(--radius-pill);
    font-weight: 650;
  }

  .waiting .st {
    background: var(--waiting);
    color: var(--waiting-text);
  }

  .working .st {
    background: var(--working-soft);
    color: var(--working);
  }

  .working .st :global(svg) {
    animation: spin 1.2s linear infinite;
  }

  @media (prefers-reduced-motion: reduce) {
    .working .st :global(svg) {
      animation: none;
    }
  }

  .panel,
  .card {
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    font-size: var(--text-sm);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .card.warn {
    border-color: var(--waiting-ring);
    background: var(--waiting-soft);
  }

  .ok-card {
    border-color: transparent;
    background: var(--done-soft);
  }

  h3 {
    margin: 0 0 10px;
    font-size: var(--text-base);
    font-weight: 650;
  }

  .card h3 {
    margin: 0;
  }

  .repos-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .repos {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg);
    list-style: none;
  }

  .repos li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    min-height: 44px;
    padding: 6px 10px;
  }

  .repos li + li {
    border-top: 1px solid var(--border);
  }

  .repo-name {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    min-width: 120px;
  }

  .repo-acts {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }

  p {
    margin: 0;
  }

  .help {
    line-height: 1.5;
  }

  .small {
    font-size: var(--text-sm);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }

  .ok,
  .refused {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
  }

  .ok {
    color: var(--done);
  }

  .refused {
    color: var(--warning);
  }

  .kv {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    align-items: baseline;
    gap: 7px 16px;
    margin: 0;
  }

  dt {
    color: var(--muted);
  }

  dd {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
  }

  /* A long pull request or ticket name wraps inside its panel. */
  .btn.link {
    justify-content: flex-start;
    max-width: 100%;
    overflow-wrap: anywhere;
    text-align: left;
    white-space: normal;
  }

  .ci :global(.checks) {
    margin: 0;
  }

  .figures {
    margin-bottom: 8px;
    line-height: 1.5;
  }

  .figures b {
    font-variant-numeric: tabular-nums;
  }

  .bar {
    height: 6px;
    margin-bottom: 8px;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--hover);
  }

  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
  }

  .bar.full span {
    background: var(--danger);
  }

  .journal {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .journal li {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr);
    gap: 8px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  .journal li.muted {
    display: block;
  }

  .journal time {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .journal .failed {
    color: var(--danger);
  }

  .menu {
    margin: 0;
    padding: 6px;
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 34px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    font-weight: 500;
    text-align: left;
  }

  .row:hover:not(:disabled) {
    background: var(--hover);
  }

  .row :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }
</style>
