<script module lang="ts">
  import { t } from "$lib/i18n";

  type Key = Parameters<typeof t>[0];
  const themeKeys: Record<string, Key> = {
    product: "ctoEntryThemeProduct",
    scope: "ctoEntryThemeScope",
    interface: "ctoEntryThemeInterface",
    technical: "ctoEntryThemeTechnical",
    security: "ctoEntryThemeSecurity",
    delivery: "ctoEntryThemeDelivery",
    other: "ctoEntryThemeOther",
  };

  export const themeLabel = (theme: string) => (themeKeys[theme] ? t(themeKeys[theme]) : theme);
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Send from "@lucide/svelte/icons/send";
  import QuestionInput from "$lib/ui/QuestionInput.svelte";
  import type { Snippet } from "svelte";
  import { byTheme, proposed, unanswered, withOthers, type Question, type TeamRun } from "$lib/team-run-logic";
  import { upsertRun } from "$lib/team.svelte";

  /**
   * A step's questions as one form, grouped by theme, the proposed answers filled in, sent all at once; a choice
   * also takes an answer of one's own. `children`: the step's other actions, beside Send.
   */
  let { run, step, questions, children }: { run: TeamRun; step: string; questions: Question[]; children?: Snippet } =
    $props();

  // svelte-ignore state_referenced_locally
  let answers = $state<Record<string, string[]>>(proposed(questions));
  let others = $state<Record<string, string>>({});
  let note = $state("");
  let busy = $state(false);
  let error = $state("");

  const given = $derived(withOthers(questions, answers, others));
  const missing = $derived(unanswered(questions, given));

  async function send() {
    busy = true;
    error = "";
    try {
      upsertRun(await invoke<TeamRun>("team_answer", { run: run.run, step, answers: given, note: note.trim() || undefined }));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<p class="help">{t("ctoEntryQuestionsHelp")}</p>
{#each byTheme(questions) as [theme, list] (theme)}
  <fieldset>
    <legend>{themeLabel(theme)}</legend>
    {#each list as q (q.id)}
      <div class="question">
        <span class="q-text">{q.question}</span>
        {#if q.why}<span class="why">{t("ctoEntryWhyItMatters", { why: q.why })}</span>{/if}
        <QuestionInput kind={q.kind} options={q.options} label={q.question} value={answers[q.id] ?? []} onchange={(v) => (answers[q.id] = v)} />
        {#if q.kind === "choice" || q.kind === "multi"}
          <input class="field other" placeholder={t("ctoEntryOther")} aria-label={t("ctoEntryOther")} bind:value={others[q.id]} />
        {/if}
        {#if q.implications.length === q.options.length && q.implications.some((i) => i.trim())}
          <ul class="implications">
            {#each q.options as option, i (option)}
              {#if q.implications[i]?.trim()}<li><b>{option}</b> · {q.implications[i]}</li>{/if}
            {/each}
          </ul>
        {/if}
      </div>
    {/each}
  </fieldset>
{/each}
<label class="labelled">
  <span>{t("ctoEntryNote")}</span>
  <textarea class="field" rows="2" bind:value={note}></textarea>
</label>
{#if missing.length}<p class="notice">{t("ctoEntryMissing")}</p>{/if}
{#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
<div class="buttons">
  {@render children?.()}
  <button class="btn sm primary" disabled={busy || missing.length > 0} onclick={send}>
    {#if busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Send size={14} aria-hidden="true" />{/if}
    {t("ctoEntrySend")}
  </button>
</div>

<style>
  .help {
    margin: 0;
    line-height: 1.5;
  }

  fieldset {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin: 0;
    padding: 12px 14px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  legend {
    padding: 0 6px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .question {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  .q-text {
    font-weight: 500;
  }

  .why {
    color: var(--muted);
    line-height: 1.45;
  }

  .implications {
    margin: 0;
    padding-left: 18px;
    color: var(--muted);
    line-height: 1.5;
  }

  .implications b {
    color: var(--text);
    font-weight: 500;
  }

  textarea.field {
    height: auto;
    padding: 6px 8px;
    resize: vertical;
    background: var(--panel);
  }

  .other {
    width: min(100%, 480px);
    background: var(--panel);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
