<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import Search from "@lucide/svelte/icons/search";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import StreamSteps from "$lib/ui/StreamSteps.svelte";
  import { lang, t } from "$lib/i18n";
  import { draft } from "$lib/newproject.svelte";
  import { analyzing, follow, readText, reads } from "$lib/onboarding.svelte";
  import { pick, type Analysis, type Question } from "$lib/onboarding-logic";
  import { approxUsd } from "$lib/usage.svelte";

  /** Claude Code's questions, one card at a time: the proposed answer marked and backed, why it is asked, the CTO's advice. */
  let { questions, analysis }: { questions: Question[]; analysis: Analysis } = $props();

  type Advice = { answer: string[]; why: string; cost: number | null } | { error: string };
  let advice = $state<Record<string, Advice>>({});
  const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD" });

  const at = $derived(Math.min(draft.question, questions.length - 1));
  const q = $derived(questions[at]);
  const value = (q: Question) => draft.answers[q.id] ?? q.default;
  const label = (q: Question, option: string) =>
    q.kind === "bool" ? t(option === "yes" ? "obYes" : "obNo") : option;
  const options = (q: Question) => (q.kind === "bool" ? ["yes", "no"] : q.options);
  const shown = (q: Question) => {
    const v = value(q);
    if (q.kind === "multi" && !v.length) return t("cpNone");
    return v.map((o) => label(q, o)).join(", ") || t("cpNone");
  };

  function choose(q: Question, option: string) {
    draft.answers[q.id] = pick(q, value(q), option);
  }

  async function ask(q: Question) {
    delete advice[q.id];
    try {
      advice[q.id] = await follow(`advice:${q.id}`, () => invoke<Advice>("setup_advise", { analysis, question: q.id }));
    } catch (e) {
      advice[q.id] = { error: String(e) };
    }
  }
</script>

{#each questions.slice(0, at) as done, i (done.id)}
  <div class="q-done">
    <CircleCheck size={16} aria-hidden="true" />
    <p><span>{done.text}</span> <b>{shown(done)}</b></p>
    <button class="btn sm quiet" onclick={() => (draft.question = i)}>{t("cpEdit")}</button>
  </div>
{/each}

{#if q}
  {@const given = value(q)}
  {@const got = advice[q.id]}
  {@const live = reads[`advice:${q.id}`]}
  <section class="q" aria-labelledby="q-text">
    <h3 id="q-text">{q.text}</h3>
    {#if q.why}<p class="why">{t("cpWhy", { why: q.why })}</p>{/if}
    {#if q.kind === "text"}
      <textarea
        class="field"
        rows="2"
        aria-labelledby="q-text"
        value={given[0] ?? ""}
        oninput={(e) => (draft.answers[q.id] = e.currentTarget.value.trim() ? [e.currentTarget.value] : [])}
      ></textarea>
      {#if q.default.length}<p class="proof"><Search size={12} aria-hidden="true" />{t("cpProposedText", { answer: q.default[0] })}</p>{/if}
    {:else}
      <div class="opts" role={q.kind === "multi" ? "group" : "radiogroup"} aria-labelledby="q-text">
        {#each options(q) as option (option)}
          {@const on = given.includes(option)}
          {@const proposed = q.default.includes(option)}
          <label class="opt" class:on>
            <input
              type={q.kind === "multi" ? "checkbox" : "radio"}
              name={q.id}
              checked={on}
              onchange={() => choose(q, option)}
            />
            <span class="t">
              <span class="name">{label(q, option)}{#if proposed}<span class="proposed"><Sparkles size={11} aria-hidden="true" />{t("setupRecommended")}</span>{/if}</span>
              {#if proposed && q.proof}<span class="proof"><Search size={12} aria-hidden="true" />{q.proof}</span>{/if}
            </span>
          </label>
        {/each}
      </div>
    {/if}
    <div class="unsure">
      {#if analyzing[`advice:${q.id}`]}
        <div class="asking">
          <p><Sparkles size={14} aria-hidden="true" />{t("cpCtoReading")}</p>
          {#if live?.last.length}
            <StreamSteps live steps={live.last.map((r, i) => ({ verb: readText(r), state: i === live.last.length - 1 ? "now" : "ok" }))} />
          {/if}
        </div>
      {:else if got && "answer" in got}
        <div class="advice" role="status">
          <p>
            <Sparkles size={14} aria-hidden="true" />
            <b>{t("cpCtoAdvises", { answer: got.answer.map((o) => label(q, o)).join(", ") })}</b>
          </p>
          <p class="said">{got.why}{#if got.cost !== null}<span class="muted">{" · "}{t("convCost", { cost: approxUsd(money.format(got.cost)) })}</span>{/if}</p>
          {#if got.answer.join() !== given.join()}
            <button class="btn sm" onclick={() => (draft.answers[q.id] = [...got.answer])}>{t("cpTakeAdvice")}</button>
          {/if}
        </div>
      {:else}
        {#if got && "error" in got}<p class="notice error" role="alert">{got.error}</p>{/if}
        <span>{t("cpUnsure")}</span>
        <button class="btn link" onclick={() => ask(q)}><Sparkles size={13} aria-hidden="true" />{t("cpAskCto")}</button>
      {/if}
    </div>
  </section>
  <p class="q-next">
    {at < questions.length - 1 ? t("cpNext", { what: questions[at + 1].text }) : t("cpNext", { what: t("cpStepReady") })}
  </p>
{/if}

<style>
  .q-done {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
    padding: 10px 12px 10px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
  }

  .q-done :global(svg) {
    flex-shrink: 0;
    color: var(--done);
  }

  .q-done p {
    flex: 1;
    min-width: 0;
    margin: 0;
  }

  .q-done p span {
    color: var(--muted);
  }

  .q-done b {
    font-weight: 500;
  }

  .q {
    margin-top: 4px;
    padding: 22px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-2);
  }

  h3 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
    letter-spacing: -0.01em;
    line-height: 1.35;
  }

  .why {
    margin: 6px 0 0;
    color: var(--muted);
  }

  .opts {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 16px;
  }

  .opt {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 13px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition:
      border-color var(--dur-fast) ease,
      background-color var(--dur-fast) ease;
  }

  .opt:hover {
    border-color: var(--border-strong);
  }

  .opt.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .opt:has(input:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }

  .opt input {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    margin: 2px 0 0;
    accent-color: var(--accent);
  }

  .t {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .name {
    font-weight: 500;
  }

  .proposed {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    margin-left: 8px;
    color: var(--accent-ink);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .proof {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.45;
  }

  .proof :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  textarea {
    width: 100%;
    height: auto;
    min-height: 64px;
    padding: 10px 12px;
    margin-top: 14px;
    resize: vertical;
  }

  textarea + .proof {
    margin-top: 8px;
  }

  .unsure {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
    color: var(--muted);
  }

  .unsure .btn.link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .unsure .notice {
    flex-basis: 100%;
  }

  .asking,
  .advice {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 8px;
  }

  .asking p,
  .advice p {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--text);
  }

  .asking p :global(svg),
  .advice p :global(svg) {
    color: var(--accent-ink);
  }

  .advice .said {
    display: block;
    color: var(--soft);
    line-height: 1.5;
  }

  .advice .btn {
    align-self: flex-start;
  }

  .q-next {
    margin: 12px 0 0;
    padding: 13px 14px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-md);
    color: var(--muted);
  }
</style>
