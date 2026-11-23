<script lang="ts" module>
  import Archive from "@lucide/svelte/icons/archive";
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import Check from "@lucide/svelte/icons/check";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleDollarSign from "@lucide/svelte/icons/circle-dollar-sign";
  import CircleDashed from "@lucide/svelte/icons/circle-dashed";
  import CircleOff from "@lucide/svelte/icons/circle-off";
  import CirclePause from "@lucide/svelte/icons/circle-pause";
  import CirclePlay from "@lucide/svelte/icons/circle-play";
  import CircleStop from "@lucide/svelte/icons/circle-stop";
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
    interactive: ["chatStateInteractive", SquareTerminal, ""],
    stopped: ["chatStateStopped", CircleStop, ""],
    error: ["chatStateError", CircleX, "failed"],
    budget: ["chatStateBudget", CircleDollarSign, "failed"],
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

  /** The state's family of color: "working", "waiting", "done", "failed", or "" for a neutral one. */
  export const toneOf = (status: Status): Tone => statuses[status][2];
</script>

<script lang="ts">
  /** `hint` explains a state whose name says too little, as a tooltip. */
  let { status, label, hint }: { status: Status; label?: string; hint?: string } = $props();

  const [key, Icon, tone] = $derived(statuses[status]);
</script>

<span class="pill {tone}" class:spinning={status === "working"} title={hint} data-status={status}>
  <Icon size={13} aria-hidden="true" /><span class="text">{label ?? t(key)}</span>
</span>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
    height: 22px;
    padding: 0 8px 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--well);
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
    white-space: nowrap;
  }

  .working,
  .waiting,
  .done,
  .failed {
    border-color: transparent;
  }

  /* One case for every state, whatever its text ("travaille", "En cours"); ::first-letter needs a block. */
  .text {
    display: inline-block;
  }

  .text::first-letter {
    text-transform: uppercase;
  }

  .working {
    background: var(--working-soft);
    color: var(--working);
  }

  /* The one filled state: someone waits for the user. */
  .waiting {
    background: var(--waiting);
    color: var(--waiting-text);
    font-weight: 600;
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
