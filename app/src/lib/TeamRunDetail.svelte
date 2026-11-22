<script module lang="ts">
  import type { Status } from "$lib/ui/StatusPill.svelte";
  import { t } from "$lib/i18n";
  import type { RunState, Step } from "$lib/team-run-logic";

  /** A team run's state as a pill: the status, and its own label where the shared one would not fit. */
  export function runPill(state: RunState): [Status, string | undefined] {
    const pills: Record<RunState, [Status, string | undefined]> = {
      running: ["running", undefined],
      waiting: ["waiting", undefined],
      paused: ["idle", t("teamRunsPaused")],
      stopped: ["stopped", undefined],
      budget: ["budget", undefined],
      done: ["done", undefined],
    };
    return pills[state];
  }

  export function stepPill(step: Step): [Status, string] {
    switch (step.state) {
      case "pending":
        return ["starting", t("teamRunsStepPending")];
      case "running":
        return ["working", t("teamRunsStepRunning")];
      case "waiting":
        return ["waiting", t("statusWaiting")];
      case "blocked":
        return ["failed", t(step.report?.status === "failed" ? "teamRunsStepFailed" : "teamRunsStepBlocked")];
      case "done":
        return ["done", t("teamRunsStepDone")];
      case "skipped":
        return ["off", t("teamRunsStepSkipped")];
    }
  }

  /** A task by its id, a workflow step by its name. */
  export const stepName = (step: Step) => step.task ?? step.key;

  /** A run's ticket key, or what a request written in TermF is. */
  export const ticketKey = (ticket: string) => (ticket === "request" ? t("ctoEntryRequest") : ticket);
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleStop from "@lucide/svelte/icons/circle-stop";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MessageSquareText from "@lucide/svelte/icons/message-square-text";
  import MessagesSquare from "@lucide/svelte/icons/messages-square";
  import Pencil from "@lucide/svelte/icons/pencil";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import Ticket from "@lucide/svelte/icons/ticket";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import X from "@lucide/svelte/icons/x";
  import TeamOptions from "$lib/TeamOptions.svelte";
  import TeamQuestionnaire from "$lib/TeamQuestionnaire.svelte";
  import TeamRouteEdit from "$lib/TeamRouteEdit.svelte";
  import TeamRouting from "$lib/TeamRouting.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { showConversation, usd } from "$lib/agents.svelte";
  import { app } from "$lib/state.svelte";
  import { ROUTE, TRIAGE, asks, budgetError, isForm, levels, progress, type Ask, type TeamRun } from "$lib/team-run-logic";
  import { upsertRun } from "$lib/team.svelte";
  import { showTickets, tickets } from "$lib/tickets.svelte";

  let { run }: { run: TeamRun } = $props();

  /** The action under way: `<command>:<step>`. */
  let busy = $state("");
  let error = $state("");
  /** What the user writes back, by step. */
  let drafts = $state<Record<string, string>>({});
  /** The step whose "ask for changes" field is open. */
  let changing = $state<string | null>(null);
  let stopping = $state(false);
  let refusing = $state(false);
  let refusingRoute = $state(false);
  let editingRoute = $state(false);
  let resuming = $state(false);
  let budget = $state<number | null>(null);

  const waits = $derived(asks(run));
  const done = $derived(progress(run));
  const active = $derived(run.state === "running" || run.state === "waiting");
  const project = $derived(app.workspace?.projects.find((p) => p.id === run.target.project)?.name ?? run.target.project);
  const tasks = $derived(levels(run.plan?.tasks ?? []));
  const top = $derived(run.steps.filter((s) => s.kind !== "task"));
  const planFirst = $derived(!run.steps.some((s) => s.kind === "task" && s.state !== "pending"));
  const routed = $derived(run.routing && !run.routing.questions.length && !waits.some((a) => a.kind === "route") ? run.routing : null);
  const routeGate = $derived(run.steps.find((s) => s.key === ROUTE));
  const tasksOf = (foreach: Step) => run.steps.filter((s) => s.kind === "task" && s.key.startsWith(`${foreach.key}:`));
  const taskStep = (id: string) => run.steps.find((s) => s.kind === "task" && s.task === id);
  const taskOf = (step: Step) => (step.task === null ? undefined : run.plan?.tasks.find((task) => task.id === step.task));

  async function act(name: string, command: string, args: Record<string, unknown>): Promise<boolean> {
    busy = name;
    error = "";
    try {
      const answer = await invoke<TeamRun | TeamRun[]>(command, { run: run.run, ...args });
      for (const r of [answer].flat()) upsertRun(r);
      return true;
    } catch (e) {
      error = String(e);
      return false;
    } finally {
      busy = "";
    }
  }

  const approve = (step: string) => act(`approve:${step}`, "team_approve", { step });

  async function revise(step: string) {
    if (await act(`revise:${step}`, "team_revise", { step, feedback: (drafts[step] ?? "").trim() })) {
      drafts[step] = "";
      changing = null;
    }
  }

  async function resume() {
    if (await act("resume", "team_start", { options: { budget: budget ?? undefined } })) {
      resuming = false;
      budget = null;
    }
  }

  function ticketRun() {
    Object.assign(tickets, { tab: "runs", selected: run.run });
    showTickets();
  }

  const spinning = (name: string) => busy === name;
</script>

{#snippet spinner(name: string)}
  {#if spinning(name)}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
{/snippet}

{#snippet conversation(step: Step)}
  {#if step.session !== null}
    <button class="btn sm quiet" onclick={() => step.session !== null && showConversation(step.session)}>
      <MessagesSquare size={14} aria-hidden="true" />{t("chatThread")}
    </button>
  {/if}
{/snippet}

{#snippet feedback(ask: Ask, label: string, submit: string)}
  {@const key = ask.step.key}
  <label class="labelled">
    <span>{label}</span>
    <textarea class="field" rows="3" placeholder={t(ask.kind === "questions" ? "teamRunsAnswersHint" : "teamRunsFeedbackHint")} bind:value={drafts[key]}></textarea>
  </label>
  <div class="buttons">
    {#if ask.kind !== "questions" && ask.kind !== "blocked"}
      <button class="btn sm" onclick={() => (changing = null)}>{t("cancel")}</button>
    {:else if ask.accept}
      <button class="btn sm" disabled={!!busy} onclick={() => approve(key)}>
        {@render spinner(`approve:${key}`)}{t("teamRunsAccept")}
      </button>
    {/if}
    <button
      class="btn sm primary"
      disabled={!!busy || (!(drafts[key] ?? "").trim() && ask.step.kind !== "shell")}
      onclick={() => revise(key)}
    >
      {@render spinner(`revise:${key}`)}{submit}
    </button>
  </div>
{/snippet}

{#snippet violations(step: Step)}
  {#if step.violations.length}
    <ul class="violations">
      {#each step.violations as v, i (i)}
        <li class:blocking={v.blocking}>
          <TriangleAlert size={13} aria-hidden="true" />
          <span class="badge">{v.rule} · {t(v.blocking ? "teamRunsViolationBlock" : "teamRunsViolationWarn")}</span>
          {#if v.file}<span class="mono">{v.file}</span>{/if}
          <span>{v.message}</span>
        </li>
      {/each}
    </ul>
  {/if}
{/snippet}

{#snippet stepRow(step: Step)}
  {@const [status, label] = stepPill(step)}
  {@const report = step.report}
  {@const task = taskOf(step)}
  <div class="step-head">
    <span><StatusPill {status} {label} /></span>
    <span class="step-name">
      <b>{stepName(step)}</b>
      {#if task}<span class="goal">{task.goal}</span>{/if}
    </span>
    <span class="step-tags">
      {#if step.attempt > 1}<span class="muted small">{t("teamRunsAttempt", { n: step.attempt })}</span>{/if}
      {#if task?.squad}<span class="badge">{t("teamRunsSquad", { name: task.squad })}</span>{/if}
      {#if step.role}<span class="badge">{step.role}</span>{/if}
    </span>
    <span class="cost">{step.cost ? usd(step.cost) : ""}</span>
    <span class="open">{@render conversation(step)}</span>
  </div>
  {#if step.note && step.state !== "done"}<p class="note mono">{step.note}</p>{/if}
  {#if report}
    {#if report.summary}<p class="summary">{report.summary}</p>{/if}
    {#if step.files.length || report.covers.length || report.text_changes.length || report.out_of_scope_findings.length || step.feedback}
      <details>
        <summary>{t("teamRunsReport")}</summary>
        <dl class="pairs">
          {#if step.files.length}
            <dt>{t("teamRunsFiles")}</dt>
            <dd class="mono">{step.files.join(", ")}</dd>
          {/if}
          {#if report.covers.length}
            <dt>{t("teamRunsCovered")}</dt>
            <dd>{report.covers.join(", ")}</dd>
          {/if}
          {#if report.text_changes.length}
            <dt>{t("teamRunsTextChanges")}</dt>
            <dd>
              {#each report.text_changes as change, i (i)}
                <div><span class="mono">{change.file}</span> « {change.text} » · {change.justified_by}</div>
              {/each}
            </dd>
          {/if}
          {#if report.out_of_scope_findings.length}
            <dt>{t("teamRunsFindings")}</dt>
            <dd>{report.out_of_scope_findings.join(" · ")}</dd>
          {/if}
          {#if step.feedback}
            <dt>{t("teamRunsFeedbackGiven")}</dt>
            <dd>{step.feedback}</dd>
          {/if}
        </dl>
      </details>
    {/if}
  {/if}
  {@render violations(step)}
{/snippet}

<header>
  <div class="title">
    <StatusPill status={runPill(run.state)[0]} label={runPill(run.state)[1]} />
    <span class="mono muted">{ticketKey(run.target.ticket)}</span>
    <span class="muted small">{project} · {run.target.repo} · {t("teamRunsWorkflowName", { name: run.workflow })}</span>
  </div>
  <h2>{run.target.title}</h2>
  <div class="actions">
    {#if active}
      <button class="btn sm" onclick={() => (stopping = true)} disabled={!!busy}>
        <CircleStop size={14} aria-hidden="true" />{t("teamRunsStop")}
      </button>
    {:else if run.state !== "done"}
      <button class="btn sm" aria-expanded={resuming} onclick={() => (resuming = !resuming)}>
        <RotateCcw size={14} aria-hidden="true" />{t("ticketsUiResume")}
      </button>
    {/if}
    <button class="btn sm" onclick={ticketRun}><Ticket size={14} aria-hidden="true" />{t("teamRunsTicketRun")}</button>
  </div>
</header>

{#if run.note && !active}
  <p class="notice"><CircleAlert size={14} aria-hidden="true" />{run.note}</p>
{/if}
{#if error}
  <p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>
{/if}

{#if resuming}
  <section class="ask" aria-labelledby="team-resume">
    <h3 id="team-resume">{t("ticketsUiResume")}</h3>
    <p class="help">{t("teamRunsResumeHelp")}</p>
    <TeamOptions project={run.target.project} resume spent={run.cost} required={run.state === "budget"} bind:budget />
    <div class="buttons">
      <button class="btn sm" onclick={() => (resuming = false)}>{t("cancel")}</button>
      <button class="btn sm primary" disabled={!!busy || !!budgetError(budget, run.cost, run.state === "budget")} onclick={resume}>
        {@render spinner("resume")}{t("ticketsUiResume")}
      </button>
    </div>
  </section>
{/if}

<section class="budget" aria-label={t("chatBudget")}>
  <div class="figures">
    <span><b>{usd(run.cost)}</b> {t("teamRunsSpent")}</span>
    <span class="muted">
      {run.budget !== null && done.left !== null
        ? t("chatBudgetLeft", { left: usd(done.left), budget: usd(run.budget) })
        : t("teamRunsNoBudget")}
    </span>
    <span class="muted progress">{t("teamRunsProgress", { over: done.over, total: done.total })}</span>
  </div>
  {#if done.spent !== null}
    <div class="bar" class:full={done.spent >= 1}><span style="width: {done.spent * 100}%"></span></div>
  {/if}
</section>

{#if waits.length}
  <section class="waits" aria-labelledby="team-waits">
    <h3 id="team-waits">{t("teamRunsWaits")} <small>{waits.length}</small></h3>
    {#each waits as ask (ask.step.key)}
      {@const key = ask.step.key}
      {@const name = stepName(ask.step)}
      <article class="ask warn">
        <div class="ask-head">
          <b>
            {#if ask.kind === "route"}{t("ctoEntryAskRoute")}
            {:else if ask.kind === "plan"}{t("teamRunsAskPlan")}
            {:else if ask.kind === "visual"}{t("teamRunsAskVisual")}
            {:else if ask.kind === "deliver"}{t("teamRunsAskDeliver")}
            {:else if ask.kind === "questions"}{ask.step.key === TRIAGE
                ? t("ctoEntryAskQuestions")
                : ask.step.plan
                  ? t("teamRunsAskPlanQuestions")
                  : t("teamRunsAskQuestions", { step: name })}
            {:else if ask.kind === "blocked"}{t("teamRunsAskBlocked", { step: name })}
            {:else}{t("teamRunsAskGate", { step: name })}{/if}
          </b>
          {@render conversation(ask.step)}
        </div>
        {#if ask.kind === "route" && run.routing}
          <p class="help">{t("ctoEntryAskRouteHelp")}</p>
          {#if editingRoute}
            <TeamRouteEdit {run} routing={run.routing} onclose={() => (editingRoute = false)} />
          {:else}
            <TeamRouting routing={run.routing} />
          {/if}
        {:else if ask.kind === "questions" && isForm(ask.questions)}
          <TeamQuestionnaire {run} step={key} questions={ask.questions} />
        {:else if ask.kind === "plan"}
          <p class="help">{t("teamRunsAskPlanHelp")}</p>
        {:else if ask.kind === "visual"}
          <p class="help">{t("teamRunsAskVisualHelp")}</p>
        {:else if ask.kind === "deliver"}
          <p class="help">{t("teamRunsAskDeliverHelp", { run: run.run })}</p>
        {:else if ask.kind === "questions"}
          <ol class="questions">
            {#each ask.questions as question, i (i)}<li>{question.question}</li>{/each}
          </ol>
        {:else if ask.kind === "blocked"}
          {#if ask.step.note}<p class="note mono">{ask.step.note}</p>{/if}
          {#if ask.step.report?.summary}<p>{ask.step.report.summary}</p>{/if}
          {@render violations(ask.step)}
          <p class="help">{t(ask.accept ? "teamRunsAskBlockedHelp" : "teamRunsAskBlockedPlanHelp")}</p>
        {/if}

        {#if ask.kind === "route"}
          {#if !editingRoute}
            <div class="buttons">
              <button class="btn sm" disabled={!!busy} onclick={() => (refusingRoute = true)}>
                <X size={14} aria-hidden="true" />{t("teamRunsRefuse")}
              </button>
              <button class="btn sm" disabled={!!busy} onclick={() => (editingRoute = true)}>
                <Pencil size={14} aria-hidden="true" />{t("ctoEntryChange")}
              </button>
              <button class="btn sm primary" disabled={!!busy} onclick={() => approve(key)}>
                {#if spinning(`approve:${key}`)}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Check
                    size={14}
                    aria-hidden="true"
                  />{/if}
                {t("ctoEntryApprove")}
              </button>
            </div>
          {/if}
        {:else if ask.kind === "questions" && isForm(ask.questions)}
          <!-- The questionnaire sends its own answers. -->
        {:else if ask.kind === "questions"}
          {@render feedback(ask, t("teamRunsAnswers"), t("teamRunsAnswer"))}
        {:else if ask.kind === "blocked"}
          {@render feedback(ask, t("teamRunsFeedback"), t("teamRunsRetry"))}
        {:else if changing === key}
          {@render feedback(ask, t("teamRunsFeedback"), t("chatSend"))}
        {/if}

        {#if ask.kind !== "route" && ask.kind !== "questions" && ask.kind !== "blocked" && changing !== key}
          <div class="buttons">
            {#if ask.kind === "visual"}
              <button class="btn sm" disabled={!!busy} onclick={() => (refusing = true)}>
                <X size={14} aria-hidden="true" />{t("teamRunsRefuse")}
              </button>
            {/if}
            {#if ask.kind === "deliver"}
              <button class="btn sm" onclick={ticketRun}><Ticket size={14} aria-hidden="true" />{t("teamRunsTicketRun")}</button>
            {:else}
              <button class="btn sm" disabled={!!busy} onclick={() => (changing = key)}>
                <MessageSquareText size={14} aria-hidden="true" />{t("teamRunsAskChanges")}
              </button>
            {/if}
            <button class="btn sm primary" disabled={!!busy} onclick={() => approve(key)}>
              {#if spinning(`approve:${key}`)}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Check
                  size={14}
                  aria-hidden="true"
                />{/if}
              {t(ask.kind === "plan" ? "teamRunsApprovePlan" : ask.kind === "visual" ? "teamRunsValidate" : "teamRunsApprove")}
            </button>
          </div>
        {/if}
      </article>
    {/each}
  </section>
{/if}

{#if routed}
  <section aria-labelledby="team-routing">
    <h3 id="team-routing">{t("ctoEntryWhy")}</h3>
    <TeamRouting routing={routed} />
    {#if routeGate?.state === "skipped"}<p class="muted small gate">{t("ctoEntryGateSkipped")}</p>{/if}
  </section>
{/if}
{#if run.answers?.length}
  <section aria-labelledby="team-answers">
    <h3 id="team-answers">{t("ctoEntryAnswered")}</h3>
    <dl class="pairs answers">
      {#each run.answers as a, i (i)}
        <dt>{a.question.question}</dt>
        <dd>{a.answer.length ? a.answer.join(", ") : "—"}</dd>
      {/each}
    </dl>
  </section>
{/if}

{#snippet planSection()}
<section aria-labelledby="team-plan">
  <h3 id="team-plan">{t("teamRunsPlan")}</h3>
  {#if run.plan}
    {@const plan = run.plan}
    {#if plan.summary}<p class="plan-summary">{plan.summary}</p>{/if}
    <div class="scope">
      {#if plan.acceptance.length}
        <div>
          <h4>{t("teamRunsCriteria")}</h4>
          <ul class="criteria">
            {#each plan.acceptance as criterion (criterion.id)}
              <li><span class="mono">{criterion.id}</span>{criterion.text}</li>
            {/each}
          </ul>
        </div>
      {/if}
      {#if plan.out_of_scope.length}
        <div>
          <h4>{t("teamRunsOutOfScope")}</h4>
          <ul class="plain">
            {#each plan.out_of_scope as item, i (i)}<li>{item}</li>{/each}
          </ul>
        </div>
      {/if}
    </div>
    {#if plan.questions.length && !waits.some((a) => a.kind === "questions" && a.step.plan)}
      <h4>{t("teamRunsQuestions")}</h4>
      <ol class="questions">
        {#each plan.questions as question, i (i)}<li>{question.question}</li>{/each}
      </ol>
    {/if}
    {#if plan.tasks.length}
      <h4>{t("teamRunsTasks")} <small class="muted">{t("teamRunsLevelsHelp")}</small></h4>
      <ol class="levels">
        {#each tasks as level, n (n)}
          <li>
            <span class="level">{t("teamRunsLevel", { n: n + 1 })}</span>
            <ul class="tasks">
              {#each level as task (task.id)}
                {@const step = taskStep(task.id)}
                <li class="task">
                  <div class="task-head">
                    <span class="mono id">{task.id}</span>
                    <b>{task.goal}</b>
                    {#if step}
                      {@const [status, label] = stepPill(step)}
                      <StatusPill {status} {label} />
                    {/if}
                  </div>
                  <div class="tags">
                    <span class="badge">{task.role}</span>
                    {#if task.squad}<span class="badge">{t("teamRunsSquad", { name: task.squad })}</span>{/if}
                    <span class="badge">{task.repo}</span>
                    <span class="badge size">{task.size}</span>
                    {#if task.ui_impact}<span class="badge">{t("teamRunsUiImpact")}</span>{/if}
                    {#if task.covers.length}<span class="muted">{t("teamRunsCovers", { ids: task.covers.join(", ") })}</span>{/if}
                    {#if task.depends_on.length}<span class="muted">{t("teamRunsDependsOn", { ids: task.depends_on.join(", ") })}</span>{/if}
                  </div>
                  {#if task.files_hint.length}<p class="mono muted hint">{task.files_hint.join("  ")}</p>{/if}
                </li>
              {/each}
            </ul>
          </li>
        {/each}
      </ol>
    {/if}
  {:else}
    <p class="muted small">{t("teamRunsNoPlan")}</p>
  {/if}
</section>
{/snippet}

{#snippet stepsSection()}
<section aria-labelledby="team-steps">
  <h3 id="team-steps">{t("teamRunsSteps")}</h3>
  <ol class="steps">
    {#each top as step (step.key)}
      <li class="step">
        {@render stepRow(step)}
        {#if step.kind === "foreach"}
          <ol class="steps nested">
            {#each tasksOf(step) as task (task.key)}
              <li class="step">{@render stepRow(task)}</li>
            {/each}
          </ol>
        {/if}
      </li>
    {/each}
  </ol>
</section>
{/snippet}

<!-- Until a task starts, the plan is what to read; then the steps at work are. -->
{#if planFirst}
  {@render planSection()}
  {@render stepsSection()}
{:else}
  {@render stepsSection()}
  {@render planSection()}
{/if}

<ConfirmDialog
  bind:open={stopping}
  title={t("teamRunsStopTitle", { ticket: run.target.ticket })}
  confirm={t("teamRunsStopConfirm")}
  danger
  onconfirm={() => act("stop", "team_stop", {})}
>
  <p>{t("teamRunsStopText")}</p>
</ConfirmDialog>
<ConfirmDialog
  bind:open={refusingRoute}
  title={t("ctoEntryRefuseTitle")}
  confirm={t("teamRunsRefuse")}
  danger
  onconfirm={() => act("stop", "team_stop", {})}
>
  <p>{t("ctoEntryRefuseText")}</p>
</ConfirmDialog>
<ConfirmDialog
  bind:open={refusing}
  title={t("teamRunsRefuseTitle")}
  confirm={t("teamRunsRefuse")}
  danger
  onconfirm={() => act("stop", "team_stop", {})}
>
  <p>{t("teamRunsRefuseText")}</p>
</ConfirmDialog>

<style>
  header {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  h3 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 24px 0 8px;
    font-size: var(--text-md);
    font-weight: 650;
  }

  h3 small {
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 400;
  }

  h4 {
    margin: 14px 0 6px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  h4 small {
    margin-left: 6px;
    font-weight: 400;
  }

  .small {
    font-size: var(--text-sm);
  }

  .actions,
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .actions {
    margin-top: 8px;
  }

  .buttons {
    justify-content: flex-end;
  }

  header + .notice,
  .notice.error {
    margin-top: 12px;
  }

  .budget {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 16px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    font-size: var(--text-sm);
  }

  .figures {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 14px;
  }

  .figures b {
    font-size: var(--text-md);
    font-variant-numeric: tabular-nums;
  }

  .progress {
    margin-left: auto;
  }

  .bar {
    height: 6px;
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

  .waits {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .waits h3 {
    margin-bottom: 0;
  }

  .ask {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 12px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    font-size: var(--text-sm);
  }

  .waits .ask {
    margin-top: 0;
  }

  .ask.warn {
    border-color: var(--waiting-ring);
    background: var(--waiting-soft);
  }

  .ask p,
  .ask h3 {
    margin: 0;
  }

  .ask-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: var(--text-base);
  }

  .help {
    color: var(--muted);
    line-height: 1.5;
  }

  .ask .help {
    color: var(--text);
  }

  textarea.field {
    height: auto;
    padding: 6px 8px;
    resize: vertical;
  }

  .ask .field {
    background: var(--panel);
  }

  .questions {
    margin: 0;
    padding-left: 20px;
    line-height: 1.6;
  }

  .plan-summary {
    margin: 0;
    line-height: 1.5;
  }

  .scope {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 0 24px;
  }

  .criteria,
  .plain {
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
    line-height: 1.6;
  }

  .criteria li {
    display: flex;
    gap: 8px;
  }

  .criteria .mono {
    flex-shrink: 0;
    width: 34px;
    color: var(--muted);
    line-height: 1.9;
  }

  .plain li::before {
    content: "–  ";
    color: var(--muted);
  }

  .levels {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .level {
    display: block;
    margin-bottom: 4px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }

  .tasks {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  .task {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
  }

  .task + .task {
    border-top: 1px solid var(--border);
  }

  .task-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .task-head b {
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }

  .id {
    color: var(--muted);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
    font-size: var(--text-sm);
  }

  .size {
    color: var(--text);
    font-weight: 700;
  }

  .hint {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .step {
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
  }

  .nested {
    margin: 6px 0 0 18px;
    padding-left: 12px;
    border-left: 2px solid var(--border);
  }

  .nested .step:last-child {
    border-bottom: 0;
  }

  /* Fixed columns on the right: costs and conversations line up, nested tasks included. */
  .step-head {
    display: grid;
    grid-template-columns: minmax(96px, max-content) minmax(0, 1fr) auto 64px 124px;
    align-items: center;
    gap: 10px;
    min-height: 28px;
  }

  .step-tags {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .cost {
    text-align: right;
  }

  .open {
    display: flex;
    justify-content: flex-end;
  }

  .step-name {
    display: flex;
    flex: 1;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .step-name b {
    font-weight: 600;
    white-space: nowrap;
  }

  .goal {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cost {
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .note,
  .summary {
    margin: 6px 0 0;
    font-size: var(--text-sm);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .note {
    max-height: 120px;
    overflow-y: auto;
    color: var(--muted);
    white-space: pre-wrap;
  }

  details {
    margin-top: 4px;
    font-size: var(--text-sm);
  }

  summary {
    width: fit-content;
    color: var(--muted);
    cursor: pointer;
  }

  .pairs {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 4px 14px;
    margin: 6px 0 0;
  }

  .pairs dt {
    color: var(--muted);
  }

  .pairs dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .answers {
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
    font-size: var(--text-sm);
  }

  .gate {
    margin: 8px 0 0;
  }

  .violations {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .violations li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    color: var(--warning);
  }

  .violations li.blocking {
    color: var(--danger);
  }

  .violations li span:last-child {
    color: var(--text);
  }
</style>
