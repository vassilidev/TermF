<script module lang="ts">
  import type { Component } from "svelte";
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import CircleQuestionMark from "@lucide/svelte/icons/circle-question-mark";
  import Eye from "@lucide/svelte/icons/eye";
  import ListChecks from "@lucide/svelte/icons/list-checks";
  import Route from "@lucide/svelte/icons/route";
  import Send from "@lucide/svelte/icons/send";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { t } from "$lib/i18n";
  import { stepName } from "$lib/TeamRunDetail.svelte";
  import { TRIAGE, type Ask, type AskKind } from "$lib/team-run-logic";

  export const askIcons: Record<AskKind, Component> = {
    route: Route,
    plan: ListChecks,
    questions: CircleQuestionMark,
    visual: Eye,
    deliver: Send,
    gate: BellRing,
    blocked: TriangleAlert,
  };

  /** What a step waiting for the user asks, in the words of the run's detail. */
  export function askTitle(ask: Ask): string {
    const name = stepName(ask.step);
    switch (ask.kind) {
      case "route":
        return t("ctoEntryAskRoute");
      case "plan":
        return t("teamRunsAskPlan");
      case "visual":
        return t("teamRunsAskVisual");
      case "deliver":
        return t("teamRunsAskDeliver");
      case "questions":
        if (ask.step.key === TRIAGE) return t("ctoEntryAskQuestions");
        if (ask.step.plan) return t("teamRunsAskPlanQuestions");
        return ask.step.kind === "review" ? t("reviewLoopAsk") : t("teamRunsAskQuestions", { step: name });
      case "blocked":
        return t("teamRunsAskBlocked", { step: name });
      case "gate":
        return t("teamRunsAskGate", { step: name });
    }
  }
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MessageSquareText from "@lucide/svelte/icons/message-square-text";
  import Pencil from "@lucide/svelte/icons/pencil";
  import X from "@lucide/svelte/icons/x";
  import TeamQuestionnaire from "$lib/TeamQuestionnaire.svelte";
  import TeamRouteEdit from "$lib/TeamRouteEdit.svelte";
  import TeamRouting from "$lib/TeamRouting.svelte";
  import { ticketKey } from "$lib/TeamRunDetail.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { usd } from "$lib/agents.svelte";
  import { LEAVE, isForm, progress, rounds, type Question, type TeamRun } from "$lib/team-run-logic";
  import { showTeamRun, upsertRun } from "$lib/team.svelte";
  import { tickets, upsert, type Delivery, type DeliveryPreview } from "$lib/tickets.svelte";

  /**
   * A step of a team run waiting for the user, decided in place: in the CTO's thread on the Home. `primary`: the
   * screen's main action is this card's. `ondelivered`: the delivery went through, the card goes with its step.
   */
  let {
    run,
    ask,
    primary,
    ondelivered,
  }: { run: TeamRun; ask: Ask; primary: boolean; ondelivered: (text: string, url: string | null, warning: string | null) => void } =
    $props();

  let busy = $state("");
  let error = $state("");
  let draft = $state("");
  let changing = $state(false);
  let editingRoute = $state(false);
  let answering = $state(false);
  let refusing = $state(false);
  let confirmAccept = $state(false);
  let preview = $state<DeliveryPreview | null>(null);

  const key = $derived(ask.step.key);
  const Icon = $derived(askIcons[ask.kind]);
  const form = $derived(ask.kind === "questions" && isForm(ask.questions));
  const ticketRun = $derived(tickets.runs.find((r) => r.id === run.run));
  const plan = $derived(run.plan);
  const left = $derived(progress(run).left);

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

  const approve = () => act("approve", "team_approve", { step: key });

  async function revise() {
    if (await act("revise", "team_revise", { step: key, feedback: draft.trim() })) {
      draft = "";
      changing = false;
    }
  }

  /** Two steps, as in the ticket run: what goes out first, then the delivery itself once the team is done. */
  async function showPreview() {
    busy = "preview";
    error = "";
    try {
      preview = await invoke<DeliveryPreview>("tickets_deliver_preview", { id: run.run });
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  async function deliver(p: DeliveryPreview) {
    if (!(await approve())) return;
    busy = "deliver";
    try {
      const again = !!ticketRun?.prUrl;
      const delivery = await invoke<Delivery>("tickets_deliver", { id: run.run, confirm: p.fingerprint });
      upsert(delivery.run);
      ondelivered(t(again ? "deliveredAgain" : "delivered"), delivery.run.prUrl, delivery.warning);
    } catch (e) {
      // The team is done: the delivery is tried again from the ticket run.
      ondelivered("", null, String(e));
    } finally {
      busy = "";
    }
  }

  const optionLabel = (option: string) => (option === LEAVE ? t("reviewLoopLeaveOption") : option);

  /** The questions TermF asks about the findings of its review, said in the user's language from the findings. */
  function questionsOf(): Question[] {
    const last = ask.step.kind === "review" ? rounds(run, key).at(-1) : undefined;
    if (!last) return ask.questions;
    const role = (task: string) => run.plan?.tasks.find((x) => x.id === task)?.role ?? "";
    return ask.questions.map((q) => {
      const f = last.findings.find((x) => x.id === q.id);
      if (!f) return q;
      const severity = t(f.severity === "important" ? "reviewLoopImportant" : "reviewLoopNit").toLowerCase();
      if (q.kind === "text") {
        const question = t("reviewLoopNoTask", { id: f.id, severity, where: `${f.file}:${f.line}`, scenario: f.scenario });
        return { ...q, question, why: t("reviewLoopFix", { fix: f.fix }) };
      }
      return {
        ...q,
        question: t("reviewLoopWho", { id: f.id, severity, where: `${f.file}:${f.line}`, scenario: f.scenario }),
        why: t(f.to ? "reviewLoopWhyLast" : "reviewLoopWhyNoTask", { fix: f.fix }),
        implications: q.options.map((o) => (o === LEAVE ? t("reviewLoopLeaveImplies") : t("reviewLoopFixedBy", { task: o, role: role(o) }))),
      };
    });
  }

  const unique = (list: string[]) => [...new Set(list)].join(", ");
</script>

{#snippet spinner(name: string)}
  {#if busy === name}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
{/snippet}

{#snippet feedback(label: string, submit: string, accept: boolean)}
  <label class="labelled">
    <span>{label}</span>
    <textarea class="field" rows="3" placeholder={t(ask.kind === "questions" ? "teamRunsAnswersHint" : "teamRunsFeedbackHint")} bind:value={draft}></textarea>
  </label>
  <div class="acts">
    {#if changing}
      <button class="btn sm" onclick={() => (changing = false)}>{t("cancel")}</button>
    {:else if accept}
      <button class="btn sm" disabled={!!busy} onclick={approve}>{@render spinner("approve")}{t("teamRunsAccept")}</button>
    {/if}
    <button
      class="btn sm"
      class:primary
      disabled={!!busy || (!draft.trim() && ask.step.kind !== "shell")}
      onclick={revise}
    >
      {@render spinner("revise")}{submit}
    </button>
  </div>
{/snippet}

<article class="ask" data-wait="ask:{run.run}:{key}" tabindex="-1" aria-labelledby="ask-{run.run}-{key}">
  <header>
    <Icon size={15} aria-hidden="true" />
    <b id="ask-{run.run}-{key}">{askTitle(ask)}</b>
    <span class="where"><span class="mono">{ticketKey(run.target.ticket)}</span> · {run.target.title}</span>
  </header>

  {#if ask.kind === "route" && run.routing}
    {#if editingRoute}
      <TeamRouteEdit {run} routing={run.routing} onclose={() => (editingRoute = false)} />
    {:else}
      <TeamRouting routing={run.routing} />
    {/if}
  {:else if form}
    {#if editingRoute && run.routing}
      <TeamRouteEdit {run} routing={run.routing} onclose={() => (editingRoute = false)} />
    {:else if changing}
      {@render feedback(t("teamRunsAnswers"), t("teamRunsAnswer"), false)}
    {:else if primary || answering}
      <!-- Never stuck in the form: accept as it is, answer in free text, or set the path yourself. -->
      <TeamQuestionnaire {run} step={key} questions={questionsOf()} {optionLabel}>
        {#if ask.accept}
          <button
            class="btn sm"
            disabled={!!busy}
            onclick={() => (ask.step.kind === "review" ? (confirmAccept = true) : approve())}
          >
            {@render spinner("approve")}{t("teamRunsAccept")}
          </button>
        {/if}
        <button class="btn sm" disabled={!!busy} onclick={() => (changing = true)}>
          <MessageSquareText size={14} aria-hidden="true" />{t("ctoEntryFreeText")}
        </button>
        {#if ask.step.key === "triage"}
          <button class="btn sm" disabled={!!busy} onclick={() => (editingRoute = true)}>
            <Pencil size={14} aria-hidden="true" />{t("ctoEntryChange")}
          </button>
        {/if}
      </TeamQuestionnaire>
    {:else}
      <ol class="questions">
        {#each questionsOf() as question (question.id)}<li>{question.question}</li>{/each}
      </ol>
      <div class="acts">
        <button class="btn sm" onclick={() => (answering = true)}>{t("teamRunsAnswer")}</button>
      </div>
    {/if}
  {:else if ask.kind === "plan" && plan}
    {#if plan.summary}<p class="summary">{plan.summary}</p>{/if}
    <p class="facts">
      {#if plan.tasks.length}
        <span><b>{plan.tasks.length}</b> {t("teamRunsTasks").toLowerCase()} · {unique(plan.tasks.map((x) => x.role))}</span>
        <span>{unique(plan.tasks.map((x) => x.repo))}</span>
      {/if}
      {#if plan.acceptance.length}<span>{t("teamRunsCovers", { ids: plan.acceptance.map((a) => a.id).join(", ") })}</span>{/if}
      {#if run.budget !== null && left !== null}
        <span>{t("chatBudgetLeft", { left: usd(left), budget: usd(run.budget) })}</span>
      {/if}
    </p>
  {:else if ask.kind === "visual"}
    <p class="help">{t("teamRunsAskVisualHelp")}</p>
  {:else if ask.kind === "questions"}
    <ol class="questions">
      {#each ask.questions as question, i (i)}<li>{question.question}</li>{/each}
    </ol>
  {:else if ask.kind === "blocked"}
    {#if ask.step.note}<p class="note mono">{ask.step.note}</p>{/if}
    {#if ask.step.report?.summary}<p>{ask.step.report.summary}</p>{/if}
    <p class="help">{t(ask.accept ? "teamRunsAskBlockedHelp" : "teamRunsAskBlockedPlanHelp")}</p>
  {:else if ask.kind === "deliver" && preview}
    {@const p = preview}
    <p class="facts">
      {#if p.files.length}<span>{t("deliverFiles", { count: p.files.length })}</span>{/if}
      {#if p.commits.length}<span>{t("deliverCommits", { count: p.commits.length })}</span>{/if}
      {#if !p.files.length && !p.commits.length}<span>{t("deliverNothing")}</span>{/if}
    </p>
    {#if ticketRun}
      <p class="help">
        {ticketRun.prUrl
          ? t("deliverHowAgain", { branch: ticketRun.branch ?? "" })
          : t("deliverHow", { branch: ticketRun.branch ?? "", target: (ticketRun.base ?? "").replace(/^origin\//, "") })}
      </p>
    {/if}
  {/if}

  {#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}

  {#if ask.kind === "route"}
    {#if !editingRoute}
      <div class="acts">
        <button class="btn sm" disabled={!!busy} onclick={() => (refusing = true)}>
          <X size={14} aria-hidden="true" />{t("teamRunsRefuse")}
        </button>
        <button class="btn sm" disabled={!!busy} onclick={() => (editingRoute = true)}>
          <Pencil size={14} aria-hidden="true" />{t("ctoEntryChange")}
        </button>
        <button class="btn sm" class:primary disabled={!!busy} onclick={approve}>
          {#if busy === "approve"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Check size={14} aria-hidden="true" />{/if}
          {t("ctoEntryApprove")}
        </button>
      </div>
    {/if}
  {:else if form}
    <!-- The questionnaire sends its own answers. -->
  {:else if ask.kind === "questions"}
    {@render feedback(t("teamRunsAnswers"), t("teamRunsAnswer"), false)}
  {:else if ask.kind === "blocked"}
    {@render feedback(t("teamRunsFeedback"), t("teamRunsRetry"), ask.accept)}
  {:else if changing}
    {@render feedback(t("teamRunsFeedback"), t("chatSend"), false)}
  {:else if ask.kind === "deliver"}
    <div class="acts">
      {#if preview}
        <button class="btn sm" onclick={() => (preview = null)}>{t("cancel")}</button>
        <button class="btn sm" class:primary disabled={!!busy} onclick={() => preview && deliver(preview)}>
          {@render spinner("approve")}{@render spinner("deliver")}{t("deliverConfirm")}
        </button>
      {:else}
        <button class="btn sm" disabled={!!busy} onclick={approve}>{@render spinner("approve")}{t("teamRunsApprove")}</button>
        <button class="btn sm" class:primary disabled={!!busy || !ticketRun} onclick={showPreview}>
          {#if busy === "preview"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Send size={14} aria-hidden="true" />{/if}
          {t("runDeliver")}
        </button>
      {/if}
    </div>
  {:else}
    <div class="acts">
      {#if ask.kind === "plan"}
        <button class="btn sm quiet left" onclick={() => showTeamRun(run.run)}>{t("teamRunsShow")}</button>
      {/if}
      {#if ask.kind === "visual"}
        <button class="btn sm" disabled={!!busy} onclick={() => (refusing = true)}>
          <X size={14} aria-hidden="true" />{t("teamRunsRefuse")}
        </button>
      {/if}
      <button class="btn sm" disabled={!!busy} onclick={() => (changing = true)}>
        <MessageSquareText size={14} aria-hidden="true" />{t("teamRunsAskChanges")}
      </button>
      <button class="btn sm" class:primary disabled={!!busy} onclick={approve}>
        {#if busy === "approve"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Check size={14} aria-hidden="true" />{/if}
        {t(ask.kind === "plan" ? "teamRunsApprovePlan" : ask.kind === "visual" ? "teamRunsValidate" : "teamRunsApprove")}
      </button>
    </div>
  {/if}
</article>

<ConfirmDialog bind:open={confirmAccept} title={t("reviewLoopAcceptTitle")} confirm={t("teamRunsAccept")} onconfirm={approve}>
  <p>{t("reviewLoopAcceptText")}</p>
</ConfirmDialog>
<ConfirmDialog
  bind:open={refusing}
  title={t(ask.kind === "route" ? "ctoEntryRefuseTitle" : "teamRunsRefuseTitle")}
  confirm={t("teamRunsRefuse")}
  danger
  onconfirm={() => act("stop", "team_stop", {})}
>
  <p>{t(ask.kind === "route" ? "ctoEntryRefuseText" : "teamRunsRefuseText")}</p>
</ConfirmDialog>

<style>
  .ask {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    border: 1px solid var(--waiting-ring);
    border-radius: var(--radius-lg);
    background: var(--waiting-soft);
    scroll-margin: 24px;
  }

  .ask:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
  }

  header :global(svg) {
    flex-shrink: 0;
    color: var(--waiting-ring);
  }

  header b {
    font-weight: 650;
  }

  .where {
    min-width: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .where .mono {
    font-size: var(--text-xs);
  }

  p {
    margin: 0;
  }

  .summary,
  .help {
    line-height: 1.5;
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .facts b {
    color: var(--text);
    font-weight: 600;
  }

  .note {
    max-height: 120px;
    overflow: auto;
    font-size: var(--text-sm);
    white-space: pre-wrap;
  }

  .questions {
    margin: 0;
    padding-left: 20px;
    line-height: 1.6;
  }

  .acts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
  }

  .acts .left {
    margin-right: auto;
  }

  textarea.field {
    height: auto;
    padding: 6px 8px;
    resize: vertical;
    background: var(--panel);
  }
</style>
