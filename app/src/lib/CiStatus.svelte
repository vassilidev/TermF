<script lang="ts">
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleDashed from "@lucide/svelte/icons/circle-dashed";
  import CircleSlash from "@lucide/svelte/icons/circle-slash";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { t } from "$lib/i18n";
  import { outcome } from "$lib/ci.svelte";

  /** `compact`: the icon alone, its label for screen readers and on hover. */
  let { item, compact = false }: { item: { status: string; conclusion: string | null }; compact?: boolean } =
    $props();

  const shown = {
    running: { label: "ciRunning", icon: LoaderCircle },
    queued: { label: "ciQueued", icon: CircleDashed },
    passed: { label: "ciPassed", icon: CircleCheck },
    failed: { label: "ciFailed", icon: CircleX },
    stopped: { label: "ciStopped", icon: CircleSlash },
  } as const;
  const kind = $derived(outcome(item));
  const state = $derived(shown[kind]);
  const label = $derived(
    t(item.conclusion === "cancelled" ? "ciCancelled" : item.conclusion === "skipped" ? "ciSkipped" : state.label),
  );
</script>

{#if compact}
  <span class="icon {kind}" title={label}>
    <state.icon size={15} aria-hidden="true" class={kind === "running" ? "spin" : ""} />
    <span class="sr-only">{label}</span>
  </span>
{:else}
  <span class="status {kind}"><state.icon size={12} aria-hidden="true" />{label}</span>
{/if}

<style>
  .status {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    height: 20px;
    padding: 0 7px;
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
    white-space: nowrap;
  }

  .icon {
    display: inline-flex;
    flex-shrink: 0;
  }

  .status.running,
  .status.queued {
    background: var(--working-soft);
  }

  .running,
  .queued {
    color: var(--working);
  }

  .status.passed {
    background: var(--done-soft);
  }

  .passed {
    color: var(--done);
  }

  .status.failed {
    background: var(--danger-soft);
  }

  .failed {
    color: var(--danger);
  }

  .status.stopped {
    background: var(--hover);
  }

  .stopped {
    color: var(--muted);
  }
</style>
