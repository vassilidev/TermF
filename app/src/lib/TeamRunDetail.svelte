<script module lang="ts">
  import type { Status } from "$lib/ui/StatusPill.svelte";
  import { t } from "$lib/i18n";
  import type { Step } from "$lib/team-run-logic";
  import { stepTitle } from "$lib/team.svelte";

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

  export const stepName = (step: Step) => stepTitle(step.key, step.task);

  /** A run's ticket key, or what a request written in TermF is. */
  export const ticketKey = (ticket: string) => (ticket === "request" ? t("ctoEntryRequest") : ticket);
</script>

<script lang="ts">
  import MessagesSquare from "@lucide/svelte/icons/messages-square";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import DoneCriteria from "$lib/DoneCriteria.svelte";
  import RunPlan from "$lib/RunPlan.svelte";
  import TeamRouting from "$lib/TeamRouting.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { showConversation } from "$lib/agents.svelte";
  import { named, stageNames, stageOf } from "$lib/run-logic";
  import { ROUTE, asks, escalations, held, rounds, tally, type Finding, type Review, type TeamRun } from "$lib/team-run-logic";

  /**
   * A team run's own part of the Run page: the steps of one stage of its timeline (`stage`; the tasks by default, as a
   * list), the plan, why this path and the answers given.
   */
  let { run, stage = "tasks" }: { run: TeamRun; stage?: string } = $props();

  const waits = $derived(asks(run));
  /** The CTO's escalation sits under the step it decides on, in that step's stage. */
  const stageOfStep = (s: Step) => stageOf(run.steps.find((x) => x.key === s.escalates) ?? s);
  /** The tasks stage lists its tasks; any other its steps. */
  const top = $derived(
    stage === "tasks"
      ? run.steps.filter((s) => s.kind === "task")
      : run.steps.filter((s) => s.kind !== "task" && !s.escalates && stageOfStep(s) === stage),
  );
  const waiting = (id: string) => run.steps.some((s) => s.task === id && s.state !== "done" && s.state !== "skipped");
  const planFirst = $derived(!run.steps.some((s) => s.kind === "task" && s.state !== "pending"));
  /** The plan waiting for approval is read in its card. */
  const planAsked = $derived(waits.some((a) => a.kind === "plan") && !!run.plan);
  const routed = $derived(run.routing && !run.routing.questions.length && !waits.some((a) => a.kind === "route") ? run.routing : null);
  const routeGate = $derived(run.steps.find((s) => s.key === ROUTE));
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
    <button class="btn sm quiet" title={t("chatThread")} onclick={() => step.session !== null && showConversation(step.session)}>
      <MessagesSquare size={14} aria-hidden="true" /><span class="label">{t("chatThread")}</span>
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
  {@const cto = escalations(run, step)}
  {@const after = task && step.state === "pending" ? task.depends_on.filter(waiting) : []}
  <div class="step-head">
    <span class="step-name">
      <b>{task ? task.id : stepName(step)}</b>
      {#if task}<span class="goal" title={task.goal}>{task.goal}</span>{/if}
      <span class="sub">
        {[
          step.role,
          task?.repo,
          task?.squad ? t("teamRunsSquad", { name: task.squad }) : "",
          step.attempt > 1 ? t("teamRunsAttempt", { n: step.attempt }) : "",
          step.escalated ? t("disciplineEscalated") : "",
          after.length ? t("uiRunStartsAfter", { tasks: after.join(", ") }) : "",
        ]
          .filter(Boolean)
          .join(" · ")}
      </span>
    </span>
    <span class="open">{@render conversation(step)}</span>
    <span class="pill"><StatusPill {status} {label} /></span>
  </div>
  {#if step.note && step.state !== "done"}<p class="note mono">{step.note}</p>{/if}
  {#if step.kind === "update" && step.state !== "done" && step.files.length}
    <p class="note">{t("integratorConflicts", { files: step.files.join(", ") })}</p>
  {/if}
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
  {#if step.loops?.length}
    <ul class="violations">
      {#each step.loops as what, i (i)}
        <li><TriangleAlert size={13} aria-hidden="true" /><span>{t("loopsTeamFlag", { what })}</span></li>
      {/each}
    </ul>
  {/if}
  {#if step.kind === "review"}{@render reviewRounds(step)}{/if}
  <!-- While the gate waits, its card shows the definition of done. -->
  {#if step.kind === "connector" && step.state === "done" && run.done}<DoneCriteria done={run.done} />{/if}
  {#if cto.length}
    <ol class="steps nested">
      {#each cto as e (e.key)}<li class="step">{@render stepRow(e)}</li>{/each}
    </ol>
  {/if}
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
            <li class:important={f.severity === "important"}>
              <div class="finding-head">
                {#if f.severity === "important"}<TriangleAlert size={13} aria-hidden="true" class="mark" />{/if}
                <span class="badge" class:danger={f.severity === "important"}>{t(f.severity === "important" ? "reviewLoopImportant" : "reviewLoopNit")}</span>
                <span class="mono where">{f.file}:{f.line}</span>
                <span class="badge" class:warn={f.fate === "asked"}>{fate(f, waiting)}</span>
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
  {#if top.length}
    {@const name = named(stageNames, stage)}
    <section class="stage" aria-labelledby="team-steps">
      <h3 id="team-steps" class="h-sec">{name ? t(name) : stage} <span class="n">{top.length}</span></h3>
      <ol class="steps">
        {#each top as step (step.key)}
          <li class="step">{@render stepRow(step)}</li>
        {/each}
      </ol>
    </section>
  {/if}
{/snippet}

<!-- Until a task starts, the plan is what to read (it lists the tasks); then the tasks at work are, the plan folded under them. -->
{#if planFirst && stage === "tasks" && run.plan}
  {@render planSection()}
{:else}
  {@render stepsSection()}
  {#if !planAsked && run.plan && stage === "tasks"}
    <details class="panel plan">
      <summary>{t("teamRunsPlan")}</summary>
      <RunPlan {run} />
    </details>
  {/if}
{/if}

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

<style>
  .panel {
    container-type: inline-size;
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

  .stage {
    container-type: inline-size;
  }

  /* The steps of a stage as one list, like the tasks of the mock: a row each, the state on the right. */
  .steps {
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
    list-style: none;
  }

  .step {
    padding: 10px 14px;
    border-top: 1px solid var(--border);
  }

  .step:first-child {
    border-top: 0;
  }

  .nested {
    margin: 8px 0 0 18px;
    border-radius: var(--radius);
  }

  .step-head {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 10px;
    min-height: 28px;
  }

  .open {
    display: flex;
    justify-content: flex-end;
  }

  /* A narrow run page keeps the task's goal readable: the conversation button folds to its icon. */
  @container (max-width: 640px) {
    .open .label {
      display: none;
    }
  }

  .step-name {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: baseline;
    column-gap: 8px;
    min-width: 0;
  }

  .step-name b {
    font-weight: 600;
    white-space: nowrap;
  }

  .goal {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub {
    grid-column: 1 / -1;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub:empty {
    display: none;
  }

  .plan summary {
    color: var(--text);
    font-weight: 600;
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
    border-radius: var(--radius);
  }

  /* An important finding stands out from the nits around it: a tinted frame and its icon. */
  .findings li.important {
    border-color: color-mix(in srgb, var(--danger) 45%, var(--border));
  }

  .finding-head :global(.mark) {
    color: var(--danger);
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
</style>
