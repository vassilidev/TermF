<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MessageSquareText from "@lucide/svelte/icons/message-square-text";
  import MessagesSquare from "@lucide/svelte/icons/messages-square";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Send from "@lucide/svelte/icons/send";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import X from "@lucide/svelte/icons/x";
  import Delivery from "$lib/Delivery.svelte";
  import RunPlan from "$lib/RunPlan.svelte";
  import TeamQuestionnaire from "$lib/TeamQuestionnaire.svelte";
  import { stepName, ticketKey } from "$lib/TeamRunDetail.svelte";
  import TeamRouteEdit from "$lib/TeamRouteEdit.svelte";
  import TeamRouting from "$lib/TeamRouting.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { showConversation } from "$lib/agents.svelte";
  import { t } from "$lib/i18n";
  import { awaitsDelivery } from "$lib/run-logic";
  import { TRIAGE, asks, isForm, rounds, LEAVE, type Ask, type Question, type Step, type TeamRun } from "$lib/team-run-logic";
  import { upsertRun } from "$lib/team.svelte";
  import { tickets } from "$lib/tickets.svelte";

  /**
   * What a team run waits for from the user, one actionable card each, the Run page's and the Home's alike: `step`
   * keeps only that step's card; `context` names the ticket on each card, for a page that is not the run's own.
   * Once the team is done, the delivery left to the user is a card too.
   */
  let { run, step, context = false }: { run: TeamRun; step?: string; context?: boolean } = $props();

  /** The action under way: `<command>:<step>`. */
  let busy = $state("");
  let error = $state("");
  /** What the user writes back, by step. */
  let drafts = $state<Record<string, string>>({});
  /** The step whose "ask for changes" field is open. */
  let changing = $state<string | null>(null);
  let refusing = $state(false);
  let refusingRoute = $state(false);
  let editingRoute = $state(false);
  /** The review step whose acceptance as it is waits for confirmation. */
  let accepting = $state("");
  let confirmAccept = $state(false);
  let delivering = $state(false);

  const ticket = $derived(tickets.runs.find((r) => r.id === run.run));
  const waits = $derived(asks(run).filter((a) => step === undefined || a.step.key === step));
  const connector = $derived(run.steps.find((s) => s.kind === "connector"));
  const late = $derived(!!ticket && awaitsDelivery(ticket, run) && (step === undefined || step === connector?.key));

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

  const approve = (key: string) => act(`approve:${key}`, "team_approve", { step: key });

  async function revise(key: string) {
    if (await act(`revise:${key}`, "team_revise", { step: key, feedback: (drafts[key] ?? "").trim() })) {
      drafts[key] = "";
      changing = null;
    }
  }

  /** The delivery gate lets the team finish, then the delivery itself is previewed and confirmed. */
  async function deliver(key: string) {
    if (await approve(key)) delivering = true;
  }

  const spinning = (name: string) => busy === name;
  const optionLabel = (option: string) => (option === LEAVE ? t("reviewLoopLeaveOption") : option);

  /** The questions TermF asks about the findings of its review, said in the user's language from the findings. */
  function questionsOf(ask: Ask): Question[] {
    const last = ask.step.kind === "review" ? rounds(run, ask.step.key).at(-1) : undefined;
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

  function title(ask: Ask): string {
    const name = stepName(ask.step);
    if (ask.kind === "route") return t("ctoEntryAskRoute");
    if (ask.kind === "plan") return t("teamRunsAskPlan");
    if (ask.kind === "visual") return t("teamRunsAskVisual");
    if (ask.kind === "deliver") return t("teamRunsAskDeliver");
    if (ask.kind === "blocked") return t("teamRunsAskBlocked", { step: name });
    if (ask.kind === "gate") return t("teamRunsAskGate", { step: name });
    if (ask.step.key === TRIAGE) return t("ctoEntryAskQuestions");
    if (ask.step.plan) return t("teamRunsAskPlanQuestions");
    return ask.step.kind === "review" ? t("reviewLoopAsk") : t("teamRunsAskQuestions", { step: name });
  }
</script>

{#snippet spinner(name: string)}
  {#if spinning(name)}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
{/snippet}

{#snippet conversation(s: Step)}
  {#if s.session !== null}
    <button class="btn sm quiet" onclick={() => s.session !== null && showConversation(s.session)}>
      <MessagesSquare size={14} aria-hidden="true" />{t("chatThread")}
    </button>
  {/if}
{/snippet}

{#snippet head(text: string, s?: Step)}
  <header>
    <BellRing size={15} aria-hidden="true" />
    <b>{text}</b>
    {#if s}<span class="end">{@render conversation(s)}</span>{/if}
  </header>
  {#if context}<p class="where"><span class="mono">{ticketKey(run.target.ticket)}</span> · {run.target.title}</p>{/if}
{/snippet}

{#snippet feedback(ask: Ask, label: string, submit: string)}
  {@const key = ask.step.key}
  <label class="labelled">
    <span>{label}</span>
    <textarea class="field" rows="3" placeholder={t(ask.kind === "questions" ? "teamRunsAnswersHint" : "teamRunsFeedbackHint")} bind:value={drafts[key]}></textarea>
  </label>
  <div class="acts">
    {#if changing === key}
      <button class="btn sm" onclick={() => (changing = null)}>{t("cancel")}</button>
    {:else if ask.accept}
      <button class="btn sm" disabled={!!busy} onclick={() => approve(key)}>
        {@render spinner(`approve:${key}`)}{t("teamRunsAccept")}
      </button>
    {/if}
    <button class="btn sm primary" disabled={!!busy || (!(drafts[key] ?? "").trim() && ask.step.kind !== "shell")} onclick={() => revise(key)}>
      {@render spinner(`revise:${key}`)}{submit}
    </button>
  </div>
{/snippet}

{#snippet violations(s: Step)}
  {#if s.violations.length}
    <ul class="violations">
      {#each s.violations as v, i (i)}
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

{#snippet approveButton(key: string, label: string)}
  <button class="btn sm primary" disabled={!!busy} onclick={() => approve(key)}>
    {#if spinning(`approve:${key}`)}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Check size={14} aria-hidden="true" />{/if}
    {label}
  </button>
{/snippet}

{#snippet deliverButton(onclick: () => void, name: string)}
  <button class="btn sm primary" disabled={!!busy} {onclick}>
    {#if spinning(name)}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Send size={14} aria-hidden="true" />{/if}
    {t("runDeliver")}
  </button>
{/snippet}

{#if waits.length || late || delivering || error}
  <div class="waits">
    {#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
    {#each waits as ask (ask.step.key)}
      {@const key = ask.step.key}
      {#if !(ask.kind === "deliver" && delivering)}
        <article class="ask">
          {@render head(title(ask), ask.step)}
          {#if ask.kind === "route" && run.routing}
            <p class="help">{t("ctoEntryAskRouteHelp")}</p>
            {#if editingRoute}
              <TeamRouteEdit {run} routing={run.routing} onclose={() => (editingRoute = false)} />
            {:else}
              <div class="box"><TeamRouting routing={run.routing} /></div>
              <div class="acts">
                <button class="btn sm" disabled={!!busy} onclick={() => (refusingRoute = true)}>
                  <X size={14} aria-hidden="true" />{t("teamRunsRefuse")}
                </button>
                <button class="btn sm" disabled={!!busy} onclick={() => (editingRoute = true)}>
                  <Pencil size={14} aria-hidden="true" />{t("ctoEntryChange")}
                </button>
                {@render approveButton(key, t("ctoEntryApprove"))}
              </div>
            {/if}
          {:else if ask.kind === "questions" && isForm(ask.questions)}
            {#if editingRoute && key === TRIAGE && run.routing}
              <TeamRouteEdit {run} routing={run.routing} onclose={() => (editingRoute = false)} />
            {:else if changing === key}
              {@render feedback(ask, t("teamRunsAnswers"), t("teamRunsAnswer"))}
            {:else}
              <!-- Never stuck in the form: accept as it is, answer in free text, or set the path yourself. -->
              <TeamQuestionnaire {run} step={key} questions={questionsOf(ask)} {optionLabel}>
                {#if ask.accept}
                  <button
                    class="btn sm"
                    disabled={!!busy}
                    onclick={() => (ask.step.kind === "review" ? ((accepting = key), (confirmAccept = true)) : approve(key))}
                  >
                    {@render spinner(`approve:${key}`)}{t("teamRunsAccept")}
                  </button>
                {/if}
                <button class="btn sm" disabled={!!busy} onclick={() => (changing = key)}>
                  <MessageSquareText size={14} aria-hidden="true" />{t("ctoEntryFreeText")}
                </button>
                {#if key === TRIAGE}
                  <button class="btn sm" disabled={!!busy} onclick={() => (editingRoute = true)}>
                    <Pencil size={14} aria-hidden="true" />{t("ctoEntryChange")}
                  </button>
                {/if}
              </TeamQuestionnaire>
            {/if}
          {:else if ask.kind === "questions"}
            <ol class="questions">
              {#each ask.questions as question, i (i)}<li>{question.question}</li>{/each}
            </ol>
            {@render feedback(ask, t("teamRunsAnswers"), t("teamRunsAnswer"))}
          {:else if ask.kind === "blocked"}
            {#if ask.step.note}<p class="note mono">{ask.step.note}</p>{/if}
            {#if ask.step.report?.summary}<p>{ask.step.report.summary}</p>{/if}
            {@render violations(ask.step)}
            <p class="help">{t(ask.accept ? "teamRunsAskBlockedHelp" : "teamRunsAskBlockedPlanHelp")}</p>
            {@render feedback(ask, t("teamRunsFeedback"), t("teamRunsRetry"))}
          {:else}
            {#if ask.kind === "plan"}
              <p class="help">{t("teamRunsAskPlanHelp")}</p>
              {#if run.plan}<div class="box"><RunPlan {run} /></div>{/if}
            {:else if ask.kind === "visual"}
              <p class="help">{t("teamRunsAskVisualHelp")}</p>
            {/if}
            {#if changing === key}
              {@render feedback(ask, t("teamRunsFeedback"), t("chatSend"))}
            {:else}
              <div class="acts">
                {#if ask.kind === "visual"}
                  <button class="btn sm" disabled={!!busy} onclick={() => (refusing = true)}>
                    <X size={14} aria-hidden="true" />{t("teamRunsRefuse")}
                  </button>
                {/if}
                {#if ask.kind === "deliver"}
                  {@render deliverButton(() => deliver(key), `approve:${key}`)}
                {:else}
                  <button class="btn sm" disabled={!!busy} onclick={() => (changing = key)}>
                    <MessageSquareText size={14} aria-hidden="true" />{t("teamRunsAskChanges")}
                  </button>
                  {@render approveButton(
                    key,
                    t(ask.kind === "plan" ? "teamRunsApprovePlan" : ask.kind === "visual" ? "teamRunsValidate" : "teamRunsApprove"),
                  )}
                {/if}
              </div>
            {/if}
          {/if}
        </article>
      {/if}
    {/each}
    {#if delivering && ticket}
      <Delivery run={ticket} onclose={() => (delivering = false)} />
    {:else if late}
      <article class="ask">
        {@render head(t("teamRunsAskDeliver"))}
        <div class="acts">{@render deliverButton(() => (delivering = true), "")}</div>
      </article>
    {/if}
  </div>
{/if}

<ConfirmDialog bind:open={confirmAccept} title={t("reviewLoopAcceptTitle")} confirm={t("teamRunsAccept")} onconfirm={() => approve(accepting)}>
  <p>{t("reviewLoopAcceptText")}</p>
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
<ConfirmDialog bind:open={refusing} title={t("teamRunsRefuseTitle")} confirm={t("teamRunsRefuse")} danger onconfirm={() => act("stop", "team_stop", {})}>
  <p>{t("teamRunsRefuseText")}</p>
</ConfirmDialog>

<style>
  .waits {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .ask {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    border: 1px solid var(--waiting-ring);
    border-radius: var(--radius-lg);
    background: var(--waiting-soft);
    font-size: var(--text-sm);
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
    font-size: var(--text-base);
  }

  header :global(svg) {
    flex-shrink: 0;
  }

  header b {
    font-weight: 650;
  }

  .end {
    margin-left: auto;
  }

  p {
    margin: 0;
  }

  .where {
    margin-top: -6px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .help {
    line-height: 1.5;
  }

  /* What is to read inside the card: the plan, the path. */
  .box {
    padding: 12px 14px;
    border-radius: var(--radius);
    background: var(--panel);
  }

  .acts {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 6px;
  }

  textarea.field {
    height: auto;
    padding: 6px 8px;
    background: var(--panel);
    resize: vertical;
  }

  .questions {
    margin: 0;
    padding-left: 20px;
    line-height: 1.6;
  }

  .note {
    max-height: 120px;
    overflow-y: auto;
    color: var(--muted);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .violations {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
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
