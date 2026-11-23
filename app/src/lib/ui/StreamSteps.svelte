<script lang="ts" module>
  /** One action of an agent, in plain words: a verb, its object (a file, a command), when. */
  export type Step = { verb: string; object?: string; at?: string; state?: "now" | "ok" | "fail" };
</script>

<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import Dot from "@lucide/svelte/icons/dot";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { t } from "$lib/i18n";

  /**
   * An agent's actions as calm lines, verb in words and object in mono, the time on the right. While `live`, they show
   * as they come; once the turn is over they fold into one sentence (`summary`, or "n actions"), open on a click.
   */
  let { steps, live = false, summary }: { steps: Step[]; live?: boolean; summary?: string } = $props();

  let unfolded = $state(false);
  const open = $derived(live || unfolded);
  const id = `steps-${Math.random().toString(36).slice(2)}`;
  const icons = { now: LoaderCircle, ok: CircleCheck, fail: CircleX };
</script>

<div class="stream">
  {#if !live}
    <button class="fold" aria-expanded={open} aria-controls={id} onclick={() => (unfolded = !unfolded)}>
      <ChevronRight size={13} aria-hidden="true" class="caret" />
      <span>{summary ?? t("uiF1Actions", { n: steps.length })}</span>
    </button>
  {/if}
  {#if open}
    <ol {id} class="steps" aria-live={live ? "polite" : undefined}>
      {#each steps as step, i (i)}
        {@const Icon = step.state ? icons[step.state] : Dot}
        <li class="step {step.state ?? ''}">
          <Icon size={13} aria-hidden="true" class={step.state === "now" ? "spin" : undefined} />
          <span class="verb">{step.verb}</span>
          {#if step.object}<code class="object">{step.object}</code>{/if}
          {#if step.at}<span class="at">{step.at}</span>{/if}
        </li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  .stream {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .fold {
    display: inline-flex;
    align-self: flex-start;
    align-items: center;
    gap: 4px;
    max-width: 100%;
    padding: 2px 4px 2px 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--muted);
    font-size: var(--text-sm);
    text-align: left;
  }

  .fold:hover {
    color: var(--text);
  }

  .fold span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fold :global(.caret) {
    flex-shrink: 0;
    transition: transform var(--dur-fast) var(--ease-out);
  }

  .fold[aria-expanded="true"] :global(.caret) {
    transform: rotate(90deg);
  }

  .steps {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .step {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 24px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .step > :global(svg) {
    flex-shrink: 0;
  }

  .verb {
    color: var(--soft);
    font-weight: 500;
  }

  .object {
    overflow: hidden;
    color: var(--soft);
    font: 0.92em var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .at {
    margin-left: auto;
    padding-left: 8px;
    font-variant-numeric: tabular-nums;
  }

  .now,
  .now .verb {
    color: var(--text);
  }

  .now > :global(svg) {
    color: var(--working);
  }

  /* The action under way glints, once the eye has the rest; still under reduced motion. */
  @media (prefers-reduced-motion: no-preference) {
    .now .verb {
      background: linear-gradient(90deg, var(--text) 30%, var(--muted) 50%, var(--text) 70%) 0 0 / 200% 100%;
      -webkit-background-clip: text;
      background-clip: text;
      color: transparent;
      animation: glint 2.2s linear infinite;
    }
  }

  @keyframes glint {
    to {
      background-position: -200% 0;
    }
  }

  .ok > :global(svg) {
    color: var(--done);
  }

  .fail,
  .fail .verb,
  .fail > :global(svg) {
    color: var(--danger);
  }
</style>
