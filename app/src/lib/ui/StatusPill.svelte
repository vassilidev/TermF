<script lang="ts" module>
  import Archive from "@lucide/svelte/icons/archive";
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import Check from "@lucide/svelte/icons/check";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleDashed from "@lucide/svelte/icons/circle-dashed";
  import CircleOff from "@lucide/svelte/icons/circle-off";
  import CirclePause from "@lucide/svelte/icons/circle-pause";
  import CirclePlay from "@lucide/svelte/icons/circle-play";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import GitPullRequest from "@lucide/svelte/icons/git-pull-request";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import X from "@lucide/svelte/icons/x";
  import type { Component } from "svelte";
  import { t } from "$lib/i18n";

  type Tone = "" | "working" | "waiting" | "done" | "failed";

  const statuses = {
    starting: ["statusStarting", CircleDashed, ""],
    working: ["statusWorking", LoaderCircle, "working"],
    waiting: ["statusWaiting", BellRing, "waiting"],
    idle: ["statusIdle", CirclePause, ""],
    done: ["statusDone", CircleCheck, "done"],
    exited: ["exitedLabel", CircleCheck, ""],
    running: ["runStatusRunning", CirclePlay, "working"],
    delivered: ["runStatusDelivered", GitPullRequest, "done"],
    closed: ["runStatusClosed", Archive, ""],
    failed: ["runStatusFailed", CircleX, "failed"],
    succeeded: ["schedulerStatusSucceeded", CircleCheck, "done"],
    opened: ["schedulerStatusOpened", SquareTerminal, "done"],
    off: ["schedulerOff", CircleOff, ""],
    accepted: ["convAccepted", Check, "done"],
    rejected: ["convRejected", X, ""],
  } satisfies Record<string, [Parameters<typeof t>[0], Component, Tone]>;

  export type Status = keyof typeof statuses;
</script>

<script lang="ts">
  let { status, label }: { status: Status; label?: string } = $props();

  const [key, Icon, tone] = $derived(statuses[status]);
</script>

<span class="pill {tone}" class:spinning={status === "working"}>
  <Icon size={13} aria-hidden="true" />{label ?? t(key)}
</span>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
    height: 22px;
    padding: 0 8px 0 6px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    white-space: nowrap;
  }

  .working {
    background: var(--working-soft);
    color: var(--working);
  }

  .waiting {
    background: var(--waiting);
    color: var(--waiting-text);
  }

  .done {
    background: var(--done-soft);
    color: var(--done);
  }

  .failed {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .spinning :global(svg) {
    animation: spin 1.2s linear infinite;
  }

  @media (prefers-reduced-motion: reduce) {
    .spinning :global(svg) {
      animation: none;
    }
  }
</style>
