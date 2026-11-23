<script lang="ts" module>
  export type ChecklistItem = { id: string; label: string; done: boolean; run?: () => void; action?: string };
</script>

<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import X from "@lucide/svelte/icons/x";
  import { t } from "$lib/i18n";

  /**
   * A setup done in several goes, over several screens: what is done, what is left, the next item's action. The caller
   * hides it once everything is done; `ondismiss` lets the user put it away sooner.
   */
  let { title, items, ondismiss }: { title: string; items: ChecklistItem[]; ondismiss?: () => void } = $props();

  const done = $derived(items.filter((item) => item.done).length);
  const next = $derived(items.find((item) => !item.done));
</script>

<section class="checklist" aria-label={title}>
  <header>
    <h3>{title}</h3>
    <span class="count">{t("uiF1Progress", { done, total: items.length })}</span>
    {#if ondismiss}
      <button class="icon-btn sm" onclick={ondismiss} aria-label={t("dismiss")} title={t("dismiss")}><X size={14} aria-hidden="true" /></button>
    {/if}
  </header>
  <div class="bar" role="progressbar" aria-valuemin={0} aria-valuemax={items.length} aria-valuenow={done} aria-label={title}>
    <span style:width="{items.length ? (done / items.length) * 100 : 0}%"></span>
  </div>
  <ol>
    {#each items as item (item.id)}
      <li class:done={item.done} class:next={item === next}>
        <span class="box" aria-hidden="true">{#if item.done}<Check size={11} strokeWidth={3} />{/if}</span>
        <span class="label">{item.label}{#if item.done}<span class="sr-only"> ({t("uiF1Done")})</span>{/if}</span>
        {#if item === next && item.run}
          <button class="btn sm primary" onclick={item.run}>{item.action ?? t("uiF1Continue")}</button>
        {/if}
      </li>
    {/each}
  </ol>
</section>

<style>
  .checklist {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  h3 {
    flex: 1;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .count {
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .bar {
    height: 4px;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--selected);
  }

  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--done);
    transition: width var(--dur-slow) var(--ease-out);
  }

  ol {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 32px;
    color: var(--soft);
  }

  .box {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 16px;
    height: 16px;
    border: 1.5px solid var(--border-strong);
    border-radius: 50%;
  }

  .done .box {
    border-color: transparent;
    background: var(--done);
    color: var(--panel);
  }

  .done .label {
    color: var(--muted);
    text-decoration: line-through;
    text-decoration-color: var(--border-strong);
  }

  .next .label {
    color: var(--text);
    font-weight: 500;
  }

  .label {
    flex: 1;
    min-width: 0;
  }
</style>
