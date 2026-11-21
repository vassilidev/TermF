<script lang="ts">
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import Coffee from "@lucide/svelte/icons/coffee";
  import Search from "@lucide/svelte/icons/search";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import Settings from "$lib/Settings.svelte";
  import { toggleQuickTerminal } from "$lib/QuickTerminal.svelte";
  import { keys, nextWaiting } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { files, isMac, showFiles } from "$lib/files.svelte";
  import { showLibrary } from "$lib/library.svelte";
  import { app, ui, waitingSessions } from "$lib/state.svelte";

  const waiting = $derived(waitingSessions().length);
</script>

<header class="topbar">
  <span class="brand">TermF</span>
  <nav class="segmented" aria-label={t("views")}>
    <button aria-pressed={files.view === "terminals"} onclick={() => (files.view = "terminals")}>
      {t("viewTerminals")}
    </button>
    <button aria-pressed={files.view === "files"} onclick={showFiles}>{t("viewFiles")}</button>
    <button aria-pressed={files.view === "library"} onclick={showLibrary}>{t("viewLibrary")}</button>
  </nav>

  <button
    class="omnibox"
    aria-label={t("palette")}
    aria-haspopup="dialog"
    aria-keyshortcuts={isMac ? "Meta+K" : "Control+K"}
    onclick={() => (ui.palette = true)}
  >
    <Search size={14} aria-hidden="true" />
    <span>{t("palettePlaceholder")}</span>
    <kbd>{keys.palette}</kbd>
  </button>

  <div class="tools">
    {#if waiting}
      <button
        class="waiting"
        onclick={nextWaiting}
        title="{t('nextWaiting')} ({keys.next})"
        aria-keyshortcuts={isMac ? "Meta+J" : "Control+J"}
      >
        <BellRing size={14} aria-hidden="true" />{t("waitingCount", { count: waiting })}
      </button>
    {/if}
    {#if app.keepAwake?.active}
      <button
        class="icon-btn awake"
        onclick={() => (ui.settings = true)}
        aria-label={t("keepAwakeActive")}
        title={t("keepAwakeActive")}
      >
        <Coffee size={16} aria-hidden="true" />
      </button>
    {/if}
    <button
      class="icon-btn"
      onclick={toggleQuickTerminal}
      aria-label={t("quickTerminal")}
      aria-keyshortcuts="Control+`"
      title={t("quickTerminalTitle")}
    >
      <SquareTerminal size={16} aria-hidden="true" />
    </button>
    <Settings />
  </div>
</header>

<style>
  .topbar {
    grid-area: top;
    display: flex;
    align-items: center;
    gap: 12px;
    height: 44px;
    padding: 0 8px 0 16px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  .brand {
    width: 244px;
    flex-shrink: 0;
    font-size: var(--text-md);
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .segmented {
    flex-shrink: 0;
  }

  .segmented button {
    height: 24px;
    padding: 0 10px;
    font-size: var(--text-base);
  }

  .omnibox {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 0 1 420px;
    min-width: 0;
    height: 30px;
    margin: 0 auto;
    padding: 0 6px 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--panel);
    color: var(--muted);
    text-align: left;
    transition: border-color var(--dur-fast) ease-out;
  }

  .omnibox:hover {
    border-color: var(--border-strong);
  }

  .omnibox span {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tools {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .waiting {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    margin-right: 4px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--waiting);
    color: var(--waiting-text);
    font-size: var(--text-sm);
    font-weight: 600;
    white-space: nowrap;
  }

  .waiting:hover {
    filter: brightness(1.05);
  }

  .awake {
    color: var(--warning);
  }
</style>
