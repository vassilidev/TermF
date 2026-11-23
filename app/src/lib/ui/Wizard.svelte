<script lang="ts" module>
  /** A step and, once answered, its answer in a few words. */
  export type WizardStep = { id: string; label: string; answer?: string };
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import { t } from "$lib/i18n";
  import { fixed } from "$lib/keymap.svelte";

  /**
   * Several decisions before anything is written, one per screen. On the left the steps with the answers already given
   * (a click goes back to change one); at the bottom Back and Continue, always visible; Enter continues, outside a
   * multi-line field.
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
    continueLabel,
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
    continueLabel?: string;
    aside?: Snippet;
    children: Snippet;
  } = $props();

  function onkeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    if (e.key !== "Enter" || e.shiftKey || e.isComposing || target.closest("textarea, button, a, [contenteditable]")) return;
    if (!canContinue || busy) return;
    e.preventDefault();
    oncontinue();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (Enter is the keyboard path of Continue) -->
<div class="wizard" {onkeydown}>
  <nav class="steps" aria-label={title}>
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
            <span class="n" aria-hidden="true">{#if done}<Check size={12} />{:else}{i + 1}{/if}</span>
            <span class="text">
              <span class="label">{step.label}</span>
              {#if done && step.answer}<span class="answer">{step.answer}</span>{/if}
            </span>
          </button>
        </li>
      {/each}
    </ol>
  </nav>
  <section class="page" aria-label={steps[current]?.label}>
    <div class="content">{@render children()}</div>
    {#if aside}<div class="aside">{@render aside()}</div>{/if}
  </section>
  <footer>
    <span class="where">{t("uiF1StepOf", { n: current + 1, total: steps.length })}</span>
    <button class="btn" onclick={onback} disabled={current === 0}>{t("uiF1Back")}</button>
    <button class="btn primary" onclick={oncontinue} disabled={!canContinue || busy} aria-busy={busy || undefined}>
      {continueLabel ?? t("uiF1Continue")}<kbd>{fixed("Enter")}</kbd>
    </button>
  </footer>
</div>

<style>
  .wizard {
    display: grid;
    grid-template: minmax(0, 1fr) auto / 260px minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .steps {
    grid-row: 1 / 3;
    padding: 24px 12px;
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
    padding: 8px 10px;
    border: 1px solid transparent;
    border-radius: var(--radius);
    background: none;
    color: var(--muted);
    text-align: left;
    transition: background-color var(--dur-fast) ease;
  }

  .step:disabled {
    opacity: 1;
  }

  .step.done {
    color: var(--soft);
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
    flex-direction: column;
    min-width: 0;
  }

  .label {
    font-weight: 500;
    line-height: 22px;
  }

  .answer {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .page {
    display: flex;
    gap: 32px;
    min-height: 0;
    padding: 32px 40px;
    overflow-y: auto;
  }

  .content {
    flex: 1;
    max-width: 640px;
    min-width: 0;
  }

  .aside {
    width: 280px;
    flex-shrink: 0;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 64px;
    padding: 0 32px 0 40px;
    border-top: 1px solid var(--border);
    background: var(--panel);
  }

  .where {
    flex: 1;
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  footer .btn {
    height: var(--h-lg);
    padding: 0 16px;
    border-radius: var(--radius-md);
  }
</style>
