<script module lang="ts">
  import type { Status } from "$lib/ui/StatusPill.svelte";
  import { t } from "$lib/i18n";
  import { stageNames } from "$lib/run-logic";
  import type { Step } from "$lib/team-run-logic";

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

  /** A task by its id, a shipped workflow step by its name, any other by its id. */
  export const stepName = (step: Step) => step.task ?? (step.key in stageNames ? t(stageNames[step.key]) : step.key);

  /** A run's ticket key, or what a request written in TermF is. */
  export const ticketKey = (ticket: string) => (ticket === "request" ? t("ctoEntryRequest") : ticket);
</script>

<script lang="ts">
  import MessagesSquare from "@lucide/svelte/icons/messages-square";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import RunPlan from "$lib/RunPlan.svelte";
  import TeamRouting from "$lib/TeamRouting.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { showConversation, usd } from "$lib/agents.svelte";
  import { ROUTE, asks, held, rounds, tally, type Finding, type Review, type TeamRun } from "$lib/team-run-logic";

  /** A team run's own part of the Run page: why this path, the answers given, the plan and the steps. */
  let { run }: { run: TeamRun } = $props();

  const waits = $derived(asks(run));
  const top = $derived(run.steps.filter((s) => s.kind !== "task"));
  const planFirst = $derived(!run.steps.some((s) => s.kind === "task" && s.state !== "pending"));
  /** The plan waiting for approval is read in its card. */
  const planAsked = $derived(waits.some((a) => a.kind === "plan") && !!run.plan);
  const routed = $derived(run.routing && !run.routing.questions.length && !waits.some((a) => a.kind === "route") ? run.routing : null);
  const routeGate = $derived(run.steps.find((s) => s.key === ROUTE));
  const tasksOf = (foreach: Step) => run.steps.filter((s) => s.kind === "task" && s.key.startsWith(`${foreach.key}:`));
  const taskOf = (step: Step) => (step.task === null ? undefined : run.plan?.tasks.find((task) => task.id === step.task));

  /** `waiting`: the round waits for the user, so nothing of it was sent yet. */
  function fate(f: Finding, waiting: boolean): string {
    if (f.fate === "sent") return t(waiting ? "reviewLoopWillSend" : "reviewLoopSent", { task: f.to ?? "" });
    const keys = { asked: "reviewLoopAsked", reported: "reviewLoopReported", left: "reviewLoopLeft", superseded: "reviewLoopSuperseded" } as const;
    return t(keys[f.fate]);
  }

  function roundLine(review: Review, waiting: boolean): string {
    const n = tally(review);
    if (!n.count) return t("reviewLoopClean");
    return [
      t("reviewLoopFindings", { count: n.count }),
      n.to.length ? t(waiting ? "reviewLoopWillBackTo" : "reviewLoopBackTo", { tasks: n.to.join(", ") }) : "",
      n.asked ? t("reviewLoopAskedCount", { count: n.asked }) : "",
      n.reported ? t("reviewLoopReportedCount", { count: n.reported }) : "",
      n.left ? t("reviewLoopLeftCount", { count: n.left }) : "",
      n.superseded ? t("reviewLoopSupersededCount", { count: n.superseded }) : "",
    ]
      .filter(Boolean)
      .join(" · ");
  }
</script>

{#snippet conversation(step: Step)}
  {#if step.session !== null}
    <button class="btn sm quiet" onclick={() => step.session !== null && showConversation(step.session)}>
      <MessagesSquare size={14} aria-hidden="true" />{t("chatThread")}
    </button>
  {/if}
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
  <!-- A review's report is its last round, shown with the others. -->
  {#if report && step.kind !== "review"}
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
  {#if step.kind === "review"}{@render reviewRounds(step)}{/if}
{/snippet}

{#snippet reviewRounds(step: Step)}
  {@const all = rounds(run, step.key)}
  {#each all as round, i (round.round)}
    {@const waiting = held(run, step, round)}
    <details class="round" open={i === all.length - 1}>
      <summary><b>{t("reviewLoopRound", { round: round.round })}</b> · {roundLine(round, waiting)}</summary>
      <p class="summary">{round.summary}</p>
      {#if round.findings.length}
        <ul class="findings">
          {#each round.findings as f (f.id)}
            <li class:nit={f.severity === "nit"}>
              <div class="finding-head">
                <span class="badge severity">{t(f.severity === "important" ? "reviewLoopImportant" : "reviewLoopNit")}</span>
                <span class="mono where">{f.file}:{f.line}</span>
                <span class="badge" class:asked={f.fate === "asked"}>{fate(f, waiting)}</span>
              </div>
              <p>{f.scenario}</p>
              <p class="muted">{t("reviewLoopFix", { fix: f.fix })}</p>
              {#if f.reason}<p class="muted">{t("reviewLoopWhy", { reason: f.reason })}</p>{/if}
            </li>
          {/each}
        </ul>
      {/if}
    </details>
  {/each}
{/snippet}

{#snippet planSection()}
  {#if !planAsked}
    <section class="panel" aria-labelledby="team-plan">
      <h3 id="team-plan">{t("teamRunsPlan")}</h3>
      <RunPlan {run} />
    </section>
  {/if}
{/snippet}

{#snippet stepsSection()}
  <section class="panel" aria-labelledby="team-steps">
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

{#if routed || run.answers?.length}
  <section class="panel" aria-labelledby="team-routing">
    {#if routed}
      <h3 id="team-routing">{t("ctoEntryWhy")}</h3>
      <TeamRouting routing={routed} />
      {#if routeGate?.state === "skipped"}<p class="muted small gate">{t("ctoEntryGateSkipped")}</p>{/if}
    {/if}
    {#if run.answers?.length}
      <h3 id={routed ? undefined : "team-routing"} class:next={routed}>{t("ctoEntryAnswered")}</h3>
      <dl class="pairs answers">
        {#each run.answers as a, i (i)}
          <dt>{a.question.question}</dt>
          <dd>
            {a.answer.length
              ? a.answer.map((v) => (a.question.kind === "bool" ? t(v === "yes" ? "obYes" : "obNo") : v)).join(", ")
              : "—"}
          </dd>
        {/each}
      </dl>
    {/if}
  </section>
{/if}

<!-- Until a task starts, the plan is what to read; then the steps at work are. -->
{#if planFirst}
  {@render planSection()}
  {@render stepsSection()}
{:else}
  {@render stepsSection()}
  {@render planSection()}
{/if}

<style>
  .panel {
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  h3 {
    margin: 0 0 10px;
    font-size: var(--text-base);
    font-weight: 650;
  }

  h3.next {
    margin-top: 16px;
  }

  .small {
    font-size: var(--text-sm);
  }

  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .step {
    padding: 8px 0;
    border-top: 1px solid var(--border);
  }

  .panel > .steps > .step:first-child {
    border-top: 0;
    padding-top: 0;
  }

  .nested {
    margin: 6px 0 0 18px;
    padding-left: 12px;
    border-left: 2px solid var(--border);
  }

  .nested .step:first-child {
    border-top: 0;
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
    text-align: right;
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

  .round {
    margin-top: 6px;
  }

  .findings {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }

  .findings li {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-left: 3px solid var(--danger);
    border-radius: var(--radius);
  }

  .findings li.nit {
    border-left-color: var(--border);
  }

  .findings p {
    margin: 4px 0 0;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .finding-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .where {
    overflow-wrap: anywhere;
  }

  .severity {
    color: var(--danger);
  }

  .nit .severity {
    color: var(--muted);
  }

  .badge.asked {
    border-color: var(--warning);
    color: var(--warning);
  }
</style>
