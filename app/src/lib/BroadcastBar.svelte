<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Megaphone from "@lucide/svelte/icons/megaphone";
  import { t } from "$lib/i18n";
  import { shown } from "$lib/project.svelte";
  import { app, ticked } from "$lib/state.svelte";

  /** `onexit`: Escape in the message leaves the broadcast mode. */
  let { onexit }: { onexit: () => void } = $props();

  let text = $state("");
  let input = $state<HTMLInputElement>();
  // Turning the mode on is asking to type the message.
  $effect(() => input?.focus());
  // Never to a tile of another project, out of sight.
  const targets = $derived(app.sessions.filter((s) => s.running && ticked.has(s.id) && shown(s)));
  const keys = t(navigator.userAgent.includes("Mac") ? "broadcastKeysMac" : "broadcastKeys");

  const write = (id: number, data: string) => invoke("pty_write", { id, data }).catch(() => {});

  async function send() {
    if (!text || !targets.length) return;
    const ids = targets.map((s) => s.id);
    const data = text;
    text = "";
    await Promise.all(ids.map((id) => write(id, data)));
    // Agent TUIs treat text and Enter arriving in one chunk as a paste, so Enter goes on its own.
    await new Promise((resolve) => setTimeout(resolve, 50));
    await Promise.all(ids.map((id) => write(id, "\r")));
  }

  // Capture phase, so a focused terminal does not also receive the keystroke; with nothing to send it gets it.
  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && e.shiftKey && (e.metaKey || e.ctrlKey) && text && targets.length) {
      e.preventDefault();
      e.stopPropagation();
      send();
    }
  }
</script>

<svelte:window onkeydowncapture={onkeydown} />

<form
  class="broadcast"
  onsubmit={(e) => {
    e.preventDefault();
    send();
  }}
>
  <div class="ticks">
    <button
      type="button"
      class="btn sm quiet"
      onclick={() => app.sessions.filter((s) => s.running && shown(s)).forEach((s) => ticked.add(s.id))}
    >
      {t("tickAll")}
    </button>
    <button type="button" class="btn sm quiet" onclick={() => ticked.clear()}>{t("tickNone")}</button>
  </div>
  <div class="input">
    <Megaphone size={14} aria-hidden="true" />
    <input
      class="field"
      bind:this={input}
      bind:value={text}
      onkeydown={(e) => e.key === "Escape" && (e.preventDefault(), onexit())}
      placeholder={t("broadcastPlaceholder")}
      aria-label={t("broadcastPlaceholder")}
    />
  </div>
  <span class="count" aria-live="polite">{t("recipients", { count: targets.length })}</span>
  <button
    type="submit"
    class="btn"
    class:primary={!!text && targets.length > 0}
    disabled={!text || !targets.length}
    title={keys}
    aria-keyshortcuts="Meta+Shift+Enter Control+Shift+Enter"
  >
    {t("broadcast")}<kbd>{keys}</kbd>
  </button>
</form>

<style>
  .broadcast {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 8px 8px;
  }

  .ticks {
    display: flex;
    flex-shrink: 0;
  }

  .ticks .btn {
    color: var(--muted);
  }

  .ticks .btn:hover {
    color: var(--text);
  }

  .input {
    position: relative;
    flex: 1;
    min-width: 0;
    color: var(--muted);
  }

  .input :global(svg) {
    position: absolute;
    top: 8px;
    left: 10px;
    pointer-events: none;
  }

  .input .field {
    padding-left: 30px;
  }

  .count {
    flex-shrink: 0;
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .primary kbd {
    border: 0;
    background: rgb(255 255 255 / 0.18);
    color: inherit;
  }
</style>
