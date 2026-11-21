<script lang="ts">
  import Archive from "@lucide/svelte/icons/archive";
  import CirclePlay from "@lucide/svelte/icons/circle-play";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import GitPullRequest from "@lucide/svelte/icons/git-pull-request";
  import { t } from "$lib/i18n";
  import type { RunStatus } from "$lib/tickets.svelte";

  let { status }: { status: RunStatus } = $props();

  const statuses = {
    running: { label: "runStatusRunning", icon: CirclePlay },
    delivered: { label: "runStatusDelivered", icon: GitPullRequest },
    closed: { label: "runStatusClosed", icon: Archive },
    failed: { label: "runStatusFailed", icon: CircleX },
  } as const;
  const shown = $derived(statuses[status]);
</script>

<span class="status {status}"><shown.icon size={12} aria-hidden="true" />{t(shown.label)}</span>

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

  .running {
    background: var(--working-soft);
    color: var(--working);
  }

  .delivered {
    background: var(--done-soft);
    color: var(--done);
  }

  .closed {
    background: var(--hover);
    color: var(--muted);
  }

  .failed {
    background: var(--danger-soft);
    color: var(--danger);
  }
</style>
