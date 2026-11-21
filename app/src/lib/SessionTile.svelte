<script lang="ts">
  import Terminal from "$lib/Terminal.svelte";
  import { t } from "$lib/i18n";
  import { app, closeSession, ticked, type Session } from "$lib/state.svelte";

  let { session }: { session: Session } = $props();

  const isFocused = $derived(app.focused === session.id);
</script>

<section
  class="tile"
  data-session={session.id}
  hidden={app.focused !== null && !isFocused}
  aria-label={session.title}
>
  <header>
    <input
      type="checkbox"
      class="tick"
      checked={ticked.has(session.id)}
      onchange={(e) =>
        e.currentTarget.checked ? ticked.add(session.id) : ticked.delete(session.id)}
      aria-label={t("broadcastTo")}
      title={t("broadcastTo")}
    />
    <span
      class="dot"
      class:running={session.running}
      title={session.running ? t("running") : t("exited")}
    ></span>
    <span class="title">{session.title}</span>
    {#if !session.running}
      <span class="muted"
        >{t("exitedLabel")}{session.exitCode != null ? ` (${session.exitCode})` : ""}</span
      >
    {/if}
    <button
      class="icon"
      onclick={() => (app.focused = isFocused ? null : session.id)}
      aria-label={isFocused ? t("unfocus") : t("focus")}
      title={isFocused ? t("unfocus") : t("focus")}
    >
      {isFocused ? "⤡" : "⤢"}
    </button>
    <button
      class="icon"
      onclick={() => closeSession(session.id)}
      aria-label={t("close")}
      title={t("close")}
    >
      ×
    </button>
  </header>
  <Terminal id={session.id} />
</section>

<style>
  .tile {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    background: var(--term-bg);
  }

  .tile[hidden] {
    display: none;
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px 4px 10px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
  }

  .title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .tick {
    margin: 0;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--stopped);
  }

  .dot.running {
    background: var(--running);
  }

  .icon {
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--muted);
  }

  .icon:hover {
    background: var(--hover);
    color: var(--text);
  }
</style>
