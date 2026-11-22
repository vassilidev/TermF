<script module lang="ts">
  import { t } from "$lib/i18n";

  type Key = Parameters<typeof t>[0];
  const kinds: Record<string, Key> = {
    "urgent-bug": "ctoEntryKindUrgentBug",
    bug: "ctoEntryKindBug",
    feature: "ctoEntryKindFeature",
    epic: "ctoEntryKindEpic",
    refactor: "ctoEntryKindRefactor",
    infra: "ctoEntryKindInfra",
    seo: "ctoEntryKindSeo",
    docs: "ctoEntryKindDocs",
    other: "ctoEntryKindOther",
  };

  /** A request's nature as the CTO named it; one TermF does not know as it came. */
  export const kindLabel = (kind: string) => (kinds[kind] ? t(kinds[kind]) : kind);
</script>

<script lang="ts">
  import { usd } from "$lib/agents.svelte";
  import type { Routing } from "$lib/team-run-logic";

  /** What the CTO decided for a run, and why. */
  let { routing }: { routing: Routing } = $props();
</script>

<div class="routing">
  <div class="tags">
    <span class="badge kind">{kindLabel(routing.kind)}</span>
    <span class="badge">{t("ctoEntrySize", { size: routing.size })}</span>
    {#if routing.changed}<span class="badge">{t("ctoEntryChanged")}</span>{/if}
  </div>
  <p class="reason">{routing.reason}</p>
  <dl class="pairs">
    <dt>{t("ctoEntryWorkflow")}</dt>
    <dd class="mono">{routing.workflow}</dd>
    {#if routing.add.length}
      <dt>{t("ctoEntryAdded")}</dt>
      <dd class="mono">{routing.add.join(", ")}</dd>
    {/if}
    {#if routing.skip.length}
      <dt>{t("ctoEntrySkipped")}</dt>
      <dd class="mono">{routing.skip.join(", ")}</dd>
    {/if}
    <dt>{t("ctoEntrySquads")}</dt>
    <dd>{routing.squads.length ? routing.squads.join(", ") : t("ctoEntrySquadsAll")}</dd>
    {#if routing.efforts.length}
      <dt>{t("ctoEntryEfforts")}</dt>
      <dd class="mono">{routing.efforts.map((e) => `${e.step} ${e.effort}`).join(", ")}</dd>
    {/if}
    <dt>{t("ctoEntryBudget")}</dt>
    <dd>{routing.budget_usd > 0 ? usd(routing.budget_usd) : t("ctoEntryNoAdvice")}</dd>
  </dl>
</div>

<style>
  .routing {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: var(--text-sm);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .kind {
    font-weight: 600;
  }

  .reason {
    margin: 0;
    line-height: 1.5;
  }

  .pairs {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 14px;
    margin: 0;
  }

  .pairs dt {
    color: var(--muted);
  }

  .pairs dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
