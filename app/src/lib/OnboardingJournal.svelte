<script lang="ts">
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { ago, t } from "$lib/i18n";

  /** The decisions of the onboarding assistant, as `onboarding_state` logs them. */
  let { log }: { log: { at: number; what: string; decision?: "accepted" | "rejected"; detail: string }[] } = $props();

  const logLabel = {
    tickets: "obTicketsTitle",
    worktree: "obRecipeTitle",
    ci: "obCiTitle",
    trust: "obTrustTitle",
    team: "obTeamTitle",
    skills: "stackSkillsTitle",
    mcp: "stackMcpTitle",
  } as const;
  // The CI decision's detail is the notification choice's code.
  const notifyLabel = { mine: "ciNotifyMine", all: "ciNotifyAll", off: "ciNotifyOff" } as const;
</script>

<ul class="journal">
  {#each log as entry, i (i)}
    <li>
      <span class="muted">{ago(entry.at)}</span>
      {#if entry.what === "answers"}
        {t("obLogAnswers", { count: entry.detail })}
      {:else}
        <b>{t(logLabel[entry.what as keyof typeof logLabel] ?? "obConfigure")}</b>
        {#if entry.decision}<StatusPill status={entry.decision} />{/if}
        {#if entry.what === "ci" && entry.detail in notifyLabel}<span class="detail">{t(notifyLabel[entry.detail as keyof typeof notifyLabel])}</span>
        {:else if entry.detail}<span class="mono detail">{entry.detail}</span>{/if}
      {/if}
    </li>
  {/each}
</ul>

<style>
  .journal {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  b {
    font-weight: 500;
  }

  .detail {
    color: var(--muted);
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }
</style>
