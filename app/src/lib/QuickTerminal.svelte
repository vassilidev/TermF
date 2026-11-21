<script module lang="ts">
  let toggle = () => {};
  /** Shows or hides the quick terminal, from the sidebar button. */
  export const toggleQuickTerminal = () => toggle();
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Terminal from "$lib/Terminal.svelte";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";

  let open = $state(false);
  let id = $state<number | null>(null);
  let panel: HTMLElement;
  let previousFocus: HTMLElement | null = null;

  /** The maximized session, else the tile holding the keyboard focus. */
  function contextCwd(): string | null {
    const tile = document.activeElement?.closest<HTMLElement>("[data-session]");
    const sessionId = app.focused ?? (tile ? Number(tile.dataset.session) : null);
    return app.sessions.find((s) => s.id === sessionId)?.cwd ?? null;
  }

  async function show() {
    if (!panel.contains(document.activeElement)) {
      previousFocus = document.activeElement as HTMLElement | null;
    }
    try {
      id = await invoke<number>("quick_terminal", { cwd: contextCwd(), cols: 120, rows: 16 });
    } catch (e) {
      app.error = String(e);
      return;
    }
    open = true;
    await tick();
    panel.querySelector("textarea")?.focus();
  }

  function hide() {
    open = false;
    previousFocus?.focus();
  }

  toggle = () => (open ? hide() : show());

  // In-app only, so Ctrl+` stays free for other apps (VS Code…) while TermF is in the background.
  // Not Cmd+` on macOS: the system uses it to cycle windows. Capture phase, so a focused terminal does not see it.
  function onkeydown(e: KeyboardEvent) {
    if (e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && e.code === "Backquote") {
      e.preventDefault();
      e.stopPropagation();
      toggle();
    }
  }

  onMount(() => {
    const toggled = listen<boolean>("quick-terminal", ({ payload: wasFocused }) =>
      open && wasFocused ? hide() : show(),
    );
    const exited = listen<{ id: number }>("pty-exit", ({ payload }) => {
      if (payload.id === id && open) hide();
    });
    return () => {
      toggled.then((stop) => stop());
      exited.then((stop) => stop());
    };
  });
</script>

<svelte:window onkeydowncapture={onkeydown} />

<section class="quick" class:open bind:this={panel} aria-label={t("quickTerminal")}>
  {#if id !== null}
    {#key id}
      <Terminal {id} />
    {/key}
  {/if}
</section>

<style>
  .quick {
    position: absolute;
    inset: 0 0 auto;
    z-index: 10;
    display: flex;
    flex-direction: column;
    height: 45%;
    min-height: 180px;
    border-bottom: 1px solid var(--border);
    background: var(--term-bg);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.3);
    transform: translateY(-100%);
    visibility: hidden;
    transition:
      transform 160ms ease-out,
      visibility 0s 160ms;
  }

  /* Visible at once when opening, so the terminal can take the focus right away. */
  .quick.open {
    transform: none;
    visibility: visible;
    transition: transform 160ms ease-out;
  }

  @media (prefers-reduced-motion: reduce) {
    .quick {
      transition: none;
    }
  }
</style>
