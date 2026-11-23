<script lang="ts" module>
  /**
   * A step and, once answered, its answer in a few words; `hint`: what to know of it before (current or to come);
   * `live`: what is running for it now, in a few words.
   */
  export type WizardStep = { id: string; label: string; answer?: string; hint?: string; live?: string };
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Check from "@lucide/svelte/icons/check";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { t } from "$lib/i18n";
  import { fixed } from "$lib/keymap.svelte";

  /**
   * Several decisions before anything is written, one per screen. On the left the steps with the answers already given
   * (a click goes back to change one); at the bottom Back and Continue, always visible; Enter continues, outside a
   * multi-line field and a button.
   */
  let {
    title,
    steps,
    current,
    onstep,
    onback,
    oncontinue,
    canContinue = true,
    busy = false,
    canBack,
    continueLabel,
    head,
    foot,
    secondary,
    aside,
    children,
  }: {
    title: string;
    steps: WizardStep[];
    current: number;
    onstep: (index: number) => void;
    onback: () => void;
    oncontinue: () => void;
    canContinue?: boolean;
    busy?: boolean;
    /** Back is off on the first step unless said otherwise (a step with pages of its own). */
    canBack?: boolean;
    continueLabel?: string;
    /** Above the steps: what is being set up. */
    head?: Snippet;
    /** Under the steps: how long is left, what is kept. */
    foot?: Snippet;
    /** Beside Continue: a way past the current screen. */
    secondary?: Snippet;
    aside?: Snippet;
    children: Snippet;
  } = $props();

  let page = $state<HTMLElement>();

  // A new screen starts at its top.
  $effect(() => {
    void current;
    page?.scrollTo({ top: 0 });
  });

  function onkeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    if (e.key !== "Enter" || e.shiftKey || e.isComposing || e.defaultPrevented) return;
    if (target.closest("textarea, button, a, summary, [contenteditable]")) return;
    if (!canContinue || busy) return;
    e.preventDefault();
    oncontinue();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (Enter is the keyboard path of Continue) -->
<div class="wizard" {onkeydown}>
  <nav class="steps" aria-label={title}>
    {@render head?.()}
    <ol>
      {#each steps as step, i (step.id)}
        {@const done = i < current}
        <li>
          <button
            class="step"
            class:done
            class:current={i === current}
            aria-current={i === current ? "step" : undefined}
            disabled={i > current}
            onclick={() => onstep(i)}
          >
            <span class="n" aria-hidden="true">{#if done}<Check size={12} strokeWidth={2.5} />{:else}{i + 1}{/if}</span>
            <span class="text">
              <span class="label">{step.label}</span>
              {#if done && step.answer}
                <span class="answer">{step.answer}</span>
              {:else if !done && step.hint}
                <span class="hint">{step.hint}</span>
              {/if}
              {#if step.live}
                <span class="live"><LoaderCircle size={12} aria-hidden="true" class="spin" /><span>{step.live}</span></span>
              {/if}
            </span>
            {#if done}<span class="edit">{t("cpEdit")}</span>{/if}
          </button>
        </li>
      {/each}
    </ol>
    {#if foot}<div class="foot">{@render foot()}</div>{/if}
  </nav>
  <section class="page" aria-label={steps[current]?.label} bind:this={page}>
    <div class="content">{@render children()}</div>
    {#if aside}<div class="aside">{@render aside()}</div>{/if}
  </section>
  <footer>
    <button class="btn back" onclick={onback} disabled={!(canBack ?? current > 0) || busy}>
      <ArrowLeft size={14} aria-hidden="true" />{t("uiF1Back")}
    </button>
    <span class="grow"></span>
    <span class="progress" role="img" aria-label={t("uiF1StepOf", { n: current + 1, total: steps.length })}>
      {#each steps as step, i (step.id)}<i class:d={i < current} class:c={i === current}></i>{/each}
    </span>
    <span class="grow"></span>
    {@render secondary?.()}
    <button class="btn primary lg go" onclick={oncontinue} disabled={!canContinue || busy} aria-busy={busy || undefined}>
      {#if busy}<LoaderCircle size={15} aria-hidden="true" class="spin" />{/if}
      {continueLabel ?? t("uiF1Continue")}<kbd>{fixed("Enter")}</kbd>
    </button>
  </footer>
</div>

<style>
  .wizard {
    display: grid;
    grid-template: minmax(0, 1fr) auto / 300px minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .steps {
    display: flex;
    grid-row: 1 / 2;
    flex-direction: column;
    gap: 20px;
    padding: 28px 16px 20px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--chrome);
  }

  ol {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .step {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    padding: 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: none;
    color: var(--muted);
    text-align: left;
    transition: background-color var(--dur-fast) ease;
  }

  .step:disabled {
    opacity: 1;
  }

  .step.done {
    color: var(--text);
  }

  .step.done:hover {
    background: var(--hover);
  }

  .step.current {
    border-color: var(--border);
    background: var(--panel);
    box-shadow: var(--shadow-1);
    color: var(--text);
  }

  .n {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 22px;
    height: 22px;
    border: 1.5px solid var(--border-strong);
    border-radius: 50%;
    background: var(--panel);
    font-size: var(--text-xs);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .done .n {
    border-color: transparent;
    background: var(--done-soft);
    color: var(--done);
  }

  .current .n {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
  }

  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .label {
    font-weight: 500;
    line-height: 22px;
  }

  .answer,
  .hint {
    color: var(--soft);
    font-size: var(--text-sm);
    line-height: 1.45;
  }

  .hint {
    color: var(--muted);
  }

  /* A long answer reads in three lines at most. */
  .answer {
    display: -webkit-box;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
  }

  .live {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin-top: 4px;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.4;
  }

  .live :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--working);
  }

  .live span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .edit {
    flex-shrink: 0;
    color: var(--accent-ink);
    font-size: var(--text-sm);
    font-weight: 500;
    line-height: 22px;
    opacity: 0;
    transition: opacity var(--dur-fast) ease;
  }

  .step.done:hover .edit,
  .step.done:focus-visible .edit {
    opacity: 1;
  }

  .foot {
    margin-top: auto;
    padding: 14px 10px 0;
    border-top: 1px solid var(--border);
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  .page {
    display: flex;
    gap: 32px;
    min-height: 0;
    padding: 40px 40px 48px;
    overflow-y: auto;
  }

  .content {
    flex: 1;
    max-width: 680px;
    min-width: 0;
    margin: 0 auto;
  }

  .aside {
    width: 280px;
    flex-shrink: 0;
  }

  footer {
    display: flex;
    grid-column: 1 / 3;
    align-items: center;
    gap: 10px;
    height: 68px;
    padding: 0 32px;
    border-top: 1px solid var(--border);
    background: var(--panel);
  }

  .grow {
    flex: 1;
  }

  .back {
    height: var(--h-lg);
    padding: 0 14px;
    border-radius: var(--radius-md);
  }

  .progress {
    display: flex;
    gap: 4px;
  }

  .progress i {
    width: 28px;
    height: 4px;
    border-radius: var(--radius-pill);
    background: var(--border);
    transition: background-color var(--dur) var(--ease-out);
  }

  .progress i.d {
    background: var(--done);
  }

  .progress i.c {
    background: var(--accent);
  }

  @media (max-width: 860px) {
    .wizard {
      grid-template-columns: 220px minmax(0, 1fr);
    }

    .page {
      padding: 28px 24px 40px;
    }
  }
</style>
