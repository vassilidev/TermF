<script lang="ts">
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleDashed from "@lucide/svelte/icons/circle-dashed";
  import CirclePause from "@lucide/svelte/icons/circle-pause";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Maximize2 from "@lucide/svelte/icons/maximize-2";
  import Minimize2 from "@lucide/svelte/icons/minimize-2";
  import Smartphone from "@lucide/svelte/icons/smartphone";
  import X from "@lucide/svelte/icons/x";
  import Terminal from "$lib/Terminal.svelte";
  import { keys, toggleMaximize } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { app, closeSession, ticked, type Session } from "$lib/state.svelte";

  let { session, index }: { session: Session; index: number } = $props();

  const isFocused = $derived(app.focused === session.id);

  const statuses = {
    starting: { label: "statusStarting", icon: CircleDashed },
    working: { label: "statusWorking", icon: LoaderCircle },
    waiting: { label: "statusWaiting", icon: BellRing },
    idle: { label: "statusIdle", icon: CirclePause },
    done: { label: "statusDone", icon: CircleCheck },
  } as const;

  const pill = $derived.by(() => {
    if (!session.running) {
      const failed = session.exitCode !== null && session.exitCode !== 0;
      const code = session.exitCode != null ? ` (${session.exitCode})` : "";
      return {
        kind: failed ? "failed" : "exited",
        label: t("exitedLabel") + code,
        icon: failed ? CircleX : CircleCheck,
      };
    }
    if (!session.status) return null;
    const { label, icon } = statuses[session.status];
    return { kind: session.status, label: t(label), icon };
  });
</script>

<section
  class="tile"
  class:waiting={session.running && session.status === "waiting"}
  class:ticked={ticked.has(session.id)}
  class:maximized={isFocused}
  data-session={session.id}
  hidden={app.focused !== null && !isFocused}
  aria-label={session.title}
>
  <!-- svelte-ignore a11y_no_static_element_interactions (a shortcut for the maximize button) -->
  <header ondblclick={(e) => !(e.target as Element).closest("button, input") && toggleMaximize(session.id)}>
    {#if pill}
      <span class="pill {pill.kind}">
        <pill.icon size={13} aria-hidden="true" />{pill.label}
      </span>
    {/if}
    <span class="title" title={session.cwd}>{session.title}</span>
    {#if session.remoteControl}
      <span class="rc" role="img" aria-label={t("remoteControlOn")} title={t("remoteControlOn")}>
        <Smartphone size={14} aria-hidden="true" />
      </span>
    {/if}
    <!-- Over the end of the title, so a narrow tile keeps its name readable until hovered. -->
    <div class="actions">
      {#if index < 9}<kbd aria-hidden="true">{keys.tile(index + 1)}</kbd>{/if}
      <input
        type="checkbox"
        class="tick"
        checked={ticked.has(session.id)}
        onchange={(e) =>
          e.currentTarget.checked ? ticked.add(session.id) : ticked.delete(session.id)}
        aria-label={t("broadcastTo")}
        title={t("broadcastTo")}
      />
      <button
        class="icon-btn sm"
        onclick={() => toggleMaximize(session.id)}
        aria-label={isFocused ? t("unfocus") : t("focus")}
        title="{isFocused ? t('unfocus') : t('focus')} ({keys.maximize})"
      >
        {#if isFocused}<Minimize2 size={14} aria-hidden="true" />{:else}<Maximize2 size={14} aria-hidden="true" />{/if}
      </button>
      <button
        class="icon-btn sm"
        onclick={() => closeSession(session.id)}
        aria-label={t("close")}
        title={t("close")}
      >
        <X size={14} aria-hidden="true" />
      </button>
    </div>
  </header>
  <Terminal id={session.id} />
</section>

<style>
  .tile {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--term-bg);
    box-shadow: var(--shadow-1);
    transition: box-shadow var(--dur) var(--ease-out);
  }

  .tile[hidden] {
    display: none;
  }

  .tile:focus-within {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }

  .tile.waiting {
    border-color: var(--waiting-ring);
    box-shadow: 0 0 0 1px var(--waiting-ring);
  }

  header {
    --header-bg: var(--panel);
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 8px 0 10px;
    border-bottom: 1px solid var(--border);
    background: var(--header-bg);
    user-select: none;
  }

  .waiting header {
    --header-bg: var(--waiting-soft);
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

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

  .pill.working {
    background: var(--working-soft);
    color: var(--working);
  }

  .pill.working :global(svg) {
    animation: spin 1.2s linear infinite;
  }

  .pill.waiting {
    background: var(--waiting);
    color: var(--waiting-text);
  }

  .pill.done {
    background: var(--done-soft);
    color: var(--done);
  }

  .pill.failed {
    background: var(--danger-soft);
    color: var(--danger);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .pill.working :global(svg) {
      animation: none;
    }
  }

  .rc {
    display: grid;
    place-items: center;
    color: var(--accent-ink);
  }

  .actions {
    position: absolute;
    inset: 0 6px 0 auto;
    display: flex;
    align-items: center;
    gap: 2px;
    padding-left: 20px;
    background: linear-gradient(to right, transparent, var(--header-bg) 16px);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--dur-fast) ease-out;
  }

  /* Shown on hover or keyboard focus; ticked or maximized tiles keep them, as they carry state. */
  .tile:hover .actions,
  .tile:focus-within .actions,
  .tile.ticked .actions,
  :global(.grid.broadcasting) .actions,
  .tile.maximized .actions {
    opacity: 1;
    pointer-events: auto;
  }

  .tick {
    margin: 0 6px;
  }

  kbd {
    margin-right: 2px;
  }
</style>
