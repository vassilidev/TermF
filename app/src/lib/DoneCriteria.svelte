<script lang="ts">
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import { t } from "$lib/i18n";
  import type { Done } from "$lib/team-run-logic";

  /** The definition of done as the delivery gate read it: each criterion, whether it holds, and what holds it. */
  let { done }: { done: Done } = $props();

  const labels: Record<string, Parameters<typeof t>[0]> = {
    checks: "disciplineDoneChecks",
    review: "disciplineDoneReview",
    security: "disciplineDoneSecurity",
    visual: "disciplineDoneVisual",
    docs: "disciplineDoneDocs",
  };
  const label = (id: string) => (Object.hasOwn(labels, id) ? t(labels[id]) : id);
</script>

<div class="done">
  <b class="title">{t("disciplineDoneTitle")}</b>
  <ul>
    {#each done.criteria as c (c.id)}
      <li class:unmet={!c.met}>
        {#if c.met}<CircleCheck size={14} aria-hidden="true" />{:else}<CircleX size={14} aria-hidden="true" />{/if}
        <span class="label">{label(c.id)}<span class="sr-only">, {t(c.met ? "disciplineDoneMet" : "disciplineDoneUnmet")}</span></span>
        <!-- What holds a criterion matters; a met one needs no word. -->
        {#if !c.met && c.note}<span class="note">{c.note}</span>{/if}
      </li>
    {/each}
  </ul>
  {#if done.overridden}<p class="overridden">{t("disciplineDoneOverridden", { reason: done.overridden })}</p>{/if}
</div>

<style>
  .done {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--panel);
    font-size: var(--text-sm);
  }

  .title {
    font-weight: 600;
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: grid;
    grid-template-columns: 14px auto 1fr;
    align-items: baseline;
    column-gap: 8px;
    color: var(--done);
  }

  li :global(svg) {
    align-self: center;
  }

  li.unmet {
    color: var(--danger);
  }

  .label {
    color: var(--text);
  }

  .note {
    min-width: 0;
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .overridden {
    margin: 0;
    color: var(--warning);
    overflow-wrap: anywhere;
  }

  @media (max-width: 640px) {
    li {
      grid-template-columns: 14px 1fr;
    }

    .note {
      grid-column: 2;
    }
  }
</style>
