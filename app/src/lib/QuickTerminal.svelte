<script module lang="ts">
  let toggle = () => {};
  /** Shows or hides the quick terminal, from the top bar button or the palette. */
  export const toggleQuickTerminal = () => toggle();
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import X from "@lucide/svelte/icons/x";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Terminal, { fontReady } from "$lib/Terminal.svelte";
  import { t } from "$lib/i18n";
  import { keys } from "$lib/commands";
  import { app, onScreen, ui } from "$lib/state.svelte";
  import { zoomed } from "$lib/tabs.svelte";

  let open = $state(false);
  let id = $state<number | null>(null);
  let panel: HTMLElement;
  let previousFocus: HTMLElement | null = null;

  /** The zoomed split, else the one holding the keyboard focus. */
  function contextCwd(): string | null {
    const tile = document.activeElement?.closest<HTMLElement>("[data-session]");
    const sessionId = zoomed()?.id ?? (tile ? Number(tile.dataset.session) : null);
    return app.sessions.find((s) => s.id === sessionId)?.cwd ?? null;
  }

  async function show() {
    if (!panel.contains(document.activeElement)) {
      previousFocus = document.activeElement as HTMLElement | null;
    }
    // The panel lives in the terminals pane.
    ui.view = "terminals";
    try {
      id = await invoke<number>("quick_terminal", { cwd: contextCwd(), cols: 120, rows: 16 });
    } catch (e) {
      app.error = String(e);
      return;
    }
    open = true;
    await tick();
    await fontReady;
    panel.querySelector("textarea")?.focus();
  }

  function hide() {
    open = false;
    previousFocus?.focus();
  }

  const visible = () => open && onScreen("terminals");
  toggle = () => (visible() ? hide() : show());

  onMount(() => {
    const toggled = listen<boolean>("quick-terminal", ({ payload: wasFocused }) =>
      wasFocused && visible() ? hide() : show(),
    );
    const exited = listen<{ id: number }>("pty-exit", ({ payload }) => {
      if (payload.id !== id) return;
      if (visible()) hide();
      else open = false;
    });
    return () => {
      toggled.then((stop) => stop());
      exited.then((stop) => stop());
    };
  });
</script>

<section class="quick" class:open bind:this={panel} aria-label={t("quickTerminal")}>
  <header>
    <SquareTerminal size={14} aria-hidden="true" />
    <span class="title">{t("quickTerminal")}</span>
    {#if keys.quick}<kbd>{keys.quick}</kbd>{/if}
    <button class="icon-btn sm" onclick={hide} aria-label={t("close")} title={t("close")}>
      <X size={14} aria-hidden="true" />
    </button>
  </header>
  {#if id !== null}
    {#key id}
      <Terminal {id} />
    {/key}
  {/if}
</section>

<style>
  .quick {
    position: absolute;
    inset: 0 8px auto;
    z-index: 10;
    display: flex;
    flex-direction: column;
    height: 45%;
    min-height: 200px;
    border: 1px solid var(--field-border);
    border-top: 0;
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
    overflow: hidden;
    background: var(--term-bg);
    box-shadow: var(--shadow-3);
    transform: translateY(-100%);
    visibility: hidden;
    transition:
      transform var(--dur) var(--ease-out),
      visibility 0s var(--dur);
  }

  /* Visible at once when opening, so the terminal can take the focus right away. */
  .quick.open {
    transform: none;
    visibility: visible;
    transition: transform var(--dur) var(--ease-out);
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 6px 0 12px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .title {
    flex: 1;
    color: var(--text);
    font-weight: 600;
  }
</style>
