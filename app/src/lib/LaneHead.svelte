<script lang="ts">
  import type { Snippet } from "svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { Status } from "$lib/ui/StatusPill.svelte";

  /** The head of an agent's lane on the Run page: who, on what, since when; `children`, its state and actions on the right. */
  let { role, title, meta, status, children }: { role: string | null; title: string; meta: string; status: Status; children?: Snippet } =
    $props();
</script>

<header class="lane-head">
  <Avatar role={role ?? "termf"} {status} size="lg" />
  <div class="who">
    <b>{role ?? "TermF"} · {title}</b>
    {#if meta}<span>{meta}</span>{/if}
  </div>
  <div class="end">{@render children?.()}</div>
</header>

<style>
  .lane-head {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }

  .who {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  b {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  span {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .end {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 8px;
  }
</style>
