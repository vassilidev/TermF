<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { app, ticked } from "$lib/state.svelte";

  let text = $state("");
  const targets = $derived(app.sessions.filter((s) => s.running && ticked.has(s.id)));
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
  <div class="segmented">
    <button type="button" onclick={() => app.sessions.forEach((s) => ticked.add(s.id))}>
      {t("tickAll")}
    </button>
    <button type="button" onclick={() => ticked.clear()}>{t("tickNone")}</button>
  </div>
  <input
    bind:value={text}
    placeholder={t("broadcastPlaceholder")}
    aria-label={t("broadcastPlaceholder")}
  />
  <span class="count" aria-live="polite">{t("recipients", { count: targets.length })}</span>
  <button
    type="submit"
    class="primary"
    disabled={!text || !targets.length}
    title={keys}
    aria-keyshortcuts="Meta+Shift+Enter Control+Shift+Enter"
  >
    {t("broadcast")}
  </button>
</form>

<style>
  .broadcast {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 6px 6px;
  }

  .segmented {
    display: flex;
    flex-shrink: 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }

  .segmented button {
    padding: 6px 8px;
    border: 0;
    background: var(--panel);
    color: var(--muted);
    font-size: 12px;
  }

  .segmented button + button {
    border-left: 1px solid var(--border);
  }

  .segmented button:hover {
    background: var(--hover);
    color: var(--text);
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 7px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    color: var(--text);
    font: inherit;
  }

  .count {
    flex-shrink: 0;
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }

  .primary {
    flex-shrink: 0;
    padding: 7px 12px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }
</style>
