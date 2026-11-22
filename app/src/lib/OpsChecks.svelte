<script lang="ts">
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { t } from "$lib/i18n";
  import type { Check } from "$lib/ops-logic";

  /** A connection's test: what works, and what is missing or worth knowing, with what the client said. */
  let { checks }: { checks: Check[] } = $props();

  const labels = {
    tool: "opsCheckTool",
    secret: "opsCheckSecret",
    tunnel: "opsCheckTunnel",
    connect: "opsCheckConnect",
    readonly: "opsCheckReadonly",
    journal: "opsCheckJournal",
  } as const;

  function text(c: Check): { label: string; detail: string } {
    if (c.warn && c.code === "readonly") return { label: t("opsCheckReadonlyWarn", { subject: c.subject }), detail: "" };
    if (c.warn && c.code === "journal") return { label: t("opsCheckJournalWarn"), detail: "" };
    const detail = c.code === "tool" && !c.ok ? t("opsCheckMissing", { detail: c.detail }) : c.detail;
    return { label: t(labels[c.code], { subject: c.subject }), detail };
  }
</script>

<ul class="checks" aria-label={t("opsTest")}>
  {#each checks as c, i (i)}
    {@const line = text(c)}
    <li class:fail={!c.ok && !c.warn} class:warn={c.warn}>
      {#if c.warn}<TriangleAlert size={14} aria-hidden="true" />{:else if c.ok}<CircleCheck size={14} aria-hidden="true" />{:else}<CircleX
          size={14}
          aria-hidden="true"
        />{/if}
      <span class="t">
        <span>{line.label}</span>
        {#if line.detail}<span class="detail mono">{line.detail}</span>{/if}
      </span>
    </li>
  {/each}
</ul>

<style>
  .checks {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    list-style: none;
    font-size: var(--text-sm);
  }

  li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    color: var(--done);
  }

  li :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  li.fail {
    color: var(--danger);
  }

  li.warn {
    color: var(--warning);
  }

  .t {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    color: var(--text);
  }

  .detail {
    color: var(--muted);
    font-size: var(--text-xs);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
