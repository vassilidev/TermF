<script lang="ts">
  import Rocket from "@lucide/svelte/icons/rocket";
  import X from "@lucide/svelte/icons/x";
  import StreamSteps from "$lib/ui/StreamSteps.svelte";
  import { t } from "$lib/i18n";
  import { library, showLibrary } from "$lib/library.svelte";
  import { jobs, type Job } from "$lib/newproject.svelte";

  /** What the setup left running for the project (its analysis, its rules, the expert's memory), live on the Home. */
  let { project }: { project: string } = $props();

  const list = $derived(jobs[project] ?? []);
  const running = $derived(list.some((j) => j.state === "running"));
  const label: Record<Job["id"], Parameters<typeof t>[0]> = {
    analysis: "cpJobAnalysis",
    conventions: "cpJobConventions",
    memory: "cpJobMemory",
  };
  const state = { running: "now", done: "ok", failed: "fail" } as const;

  function seeRules() {
    Object.assign(library, { global: false, tab: "conventions", panel: null });
    showLibrary();
  }
</script>

{#if list.length}
  <article class="jobs" aria-labelledby="setup-jobs">
    <header>
      <Rocket size={16} aria-hidden="true" />
      <b id="setup-jobs">{t("cpJobsTitle")}</b>
      {#if !running}
        <button class="icon-btn sm" aria-label={t("close")} title={t("close")} onclick={() => delete jobs[project]}><X size={14} aria-hidden="true" /></button>
      {/if}
    </header>
    <StreamSteps live steps={list.map((j) => ({ verb: t(label[j.id]), object: j.state === "failed" ? j.detail : undefined, state: state[j.state] }))} />
    {#if list.some((j) => j.id === "conventions" && j.state === "done")}
      <button class="btn sm" onclick={seeRules}>{t("cpJobsSeeLibrary")}</button>
    {/if}
  </article>
{/if}

<style>
  .jobs {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    align-self: stretch;
  }

  header :global(svg) {
    color: var(--accent-ink);
  }

  header .icon-btn {
    margin-left: auto;
  }
</style>
