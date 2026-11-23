<script lang="ts">
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import { t } from "$lib/i18n";

  /** The one useful next step of a screen, neutral; `action` does it. Never more than one per screen. */
  let { text, action }: { text: string; action?: { label: string; run: () => void } } = $props();
</script>

<div class="next" role="note">
  <ArrowRight size={14} aria-hidden="true" />
  <p><span class="lead">{t("uiF1NextStep")}</span> {text}</p>
  {#if action}<button class="btn sm" onclick={action.run}>{action.label}</button>{/if}
</div>

<style>
  /* In a narrow column, the action goes under the sentence rather than squeezing it. */
  .next {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    min-height: 40px;
    padding: 6px 8px 6px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
    color: var(--soft);
  }

  .next > :global(svg) {
    flex-shrink: 0;
    color: var(--accent-ink);
  }

  p {
    flex: 1 1 180px;
    min-width: 0;
    margin: 0;
  }

  .lead {
    color: var(--text);
    font-weight: 600;
  }
</style>
