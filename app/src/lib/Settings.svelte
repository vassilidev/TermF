<script lang="ts">
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import KeepAwake from "$lib/KeepAwake.svelte";
  import { t } from "$lib/i18n";
  import { app, load, ui } from "$lib/state.svelte";

  let trigger: HTMLButtonElement;
  let panel = $state<HTMLElement>();

  function close(refocus: boolean) {
    ui.settings = false;
    if (refocus) trigger.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    if (ui.settings && e.key === "Escape") {
      e.preventDefault();
      close(true);
    }
  }

  function onpointerdown(e: PointerEvent) {
    const target = e.target as Node;
    if (ui.settings && !panel?.contains(target) && !trigger.contains(target)) close(false);
  }
</script>

<svelte:window {onkeydown} {onpointerdown} />

<button
  bind:this={trigger}
  class="icon-btn"
  aria-label={t("settings")}
  title={t("settings")}
  aria-expanded={ui.settings}
  aria-controls="settings-panel"
  onclick={() => (ui.settings = !ui.settings)}
>
  <SettingsIcon size={16} aria-hidden="true" />
</button>

{#if ui.settings}
  <div
    id="settings-panel"
    class="panel"
    role="dialog"
    aria-label={t("settings")}
    bind:this={panel}
    {@attach (el) =>
      (el.querySelector<HTMLElement>("input:checked") ?? el.querySelector<HTMLElement>("input, button"))?.focus()}
  >
    <h2>{t("settings")}</h2>
    <KeepAwake />
    {#if app.workspace}
      <section class="config">
        <h3>{t("configFolder")}</h3>
        <code title={app.workspace.configDir}>{app.workspace.configDir}</code>
        <button class="btn sm" onclick={load}>
          <RefreshCw size={14} aria-hidden="true" />{t("reload")}
        </button>
      </section>
    {/if}
  </div>
{/if}

<style>
  .panel {
    position: fixed;
    top: 48px;
    right: 8px;
    z-index: 30;
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 340px;
    max-height: calc(100vh - 64px);
    padding: 16px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--raised);
    box-shadow: var(--shadow-3);
    animation: drop var(--dur) var(--ease-out);
  }

  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }

  h2 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  h3 {
    margin: 0 0 6px;
    font-size: var(--text-base);
    font-weight: 600;
  }

  .config {
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  code {
    display: block;
    margin-bottom: 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font: var(--text-sm) var(--font-mono);
  }
</style>
