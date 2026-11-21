<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "$lib/i18n";

  type Mode = "auto" | "always" | "off";
  type KeepAwakeState = { mode: Mode; displayOn: boolean; active: boolean; error: string | null };

  let current = $state<KeepAwakeState | null>(null);
  let error = $state("");

  onMount(() => {
    invoke<KeepAwakeState>("keep_awake_state").then((s) => {
      current = s;
    });
    const unlisten = listen<KeepAwakeState>("keep-awake", ({ payload }) => {
      current = payload;
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  });

  async function set(mode: Mode, displayOn: boolean) {
    try {
      current = await invoke<KeepAwakeState>("keep_awake_set", { mode, displayOn });
      error = "";
    } catch (e) {
      error = String(e);
    }
  }
</script>

{#if current}
  {@const { mode, displayOn } = current}
  <div class="keep-awake" title={t("keepAwakeHelp")}>
    <label class="row">
      <span>{t("keepAwake")}</span>
      <select value={mode} onchange={(e) => set(e.currentTarget.value as Mode, displayOn)}>
        <option value="auto">{t("keepAwakeAuto")}</option>
        <option value="always">{t("keepAwakeAlways")}</option>
        <option value="off">{t("keepAwakeOff")}</option>
      </select>
    </label>
    {#if mode !== "off"}
      <label class="row">
        <input
          type="checkbox"
          checked={displayOn}
          onchange={(e) => set(mode, e.currentTarget.checked)}
        />
        <span>{t("keepDisplayOn")}</span>
      </label>
    {/if}
    {#if current.active}
      <span class="active" role="status"
        ><span class="dot" aria-hidden="true"></span>{t("keepAwakeActive")}</span
      >
    {/if}
    {#if error || current.error}
      <span class="error" role="alert">{error || current.error}</span>
    {/if}
  </div>
{/if}

<style>
  .keep-awake {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--muted);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  select {
    flex: 1;
    min-width: 0;
    padding: 2px 4px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }

  input {
    margin: 0;
  }

  .active {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--waiting);
  }

  .error {
    color: var(--danger);
    overflow-wrap: anywhere;
  }
</style>
