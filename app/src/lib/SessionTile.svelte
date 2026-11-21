<script lang="ts">
  import Maximize2 from "@lucide/svelte/icons/maximize-2";
  import Minimize2 from "@lucide/svelte/icons/minimize-2";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import Smartphone from "@lucide/svelte/icons/smartphone";
  import X from "@lucide/svelte/icons/x";
  import ContextGauge from "$lib/ContextGauge.svelte";
  import Terminal from "$lib/Terminal.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { keys, toggleMaximize } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { app, closeSession, resetContext, ticked, type Session } from "$lib/state.svelte";

  /** `index`: among the tiles shown, -1 for a session of another project. */
  let { session, index }: { session: Session; index: number } = $props();

  const isFocused = $derived(app.focused === session.id);

  const failed = $derived(session.exitCode !== null && session.exitCode !== 0);
</script>

<section
  class="tile"
  class:waiting={session.running && session.status === "waiting"}
  class:ticked={ticked.has(session.id)}
  class:maximized={isFocused}
  data-session={session.id}
  hidden={index < 0 || (app.focused !== null && !isFocused)}
  aria-label={session.title}
>
  <!-- svelte-ignore a11y_no_static_element_interactions (a shortcut for the maximize button) -->
  <header ondblclick={(e) => !(e.target as Element).closest("button, input") && toggleMaximize(session.id)}>
    {#if !session.running}
      <StatusPill
        status={failed ? "failed" : "exited"}
        label={t("exitedLabel") + (session.exitCode != null ? ` (${session.exitCode})` : "")}
      />
    {:else if session.status}
      <StatusPill status={session.status} />
    {/if}
    {#if session.context && session.running}<ContextGauge gauge={session.context} />{/if}
    <span class="title" title={session.cwd}>{session.title}</span>
    {#if session.remoteControl}
      <span class="rc" role="img" aria-label={t("remoteControlOn")} title={t("remoteControlOn")}>
        <Smartphone size={14} aria-hidden="true" />
      </span>
    {/if}
    <!-- Over the end of the title, so a narrow tile keeps its name readable until hovered. -->
    <div class="actions">
      {#if index >= 0 && index < 9}<kbd aria-hidden="true">{keys.tile(index + 1)}</kbd>{/if}
      <input
        type="checkbox"
        class="tick"
        checked={ticked.has(session.id)}
        onchange={(e) =>
          e.currentTarget.checked ? ticked.add(session.id) : ticked.delete(session.id)}
        aria-label={t("broadcastTo")}
        title={t("broadcastTo")}
      />
      {#if session.context && session.running}
        <button
          class="icon-btn sm"
          onclick={() => resetContext(session.id)}
          disabled={session.context.phase !== "idle"}
          aria-label={t("contextReset")}
          title={t("contextReset")}
        >
          <RotateCcw size={14} aria-hidden="true" />
        </button>
      {/if}
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
