<script lang="ts">
  import Coffee from "@lucide/svelte/icons/coffee";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { t } from "$lib/i18n";
  import { app, setKeepAwake, type KeepAwakeMode } from "$lib/state.svelte";

  let error = $state("");

  const modes: [KeepAwakeMode, "keepAwakeAuto" | "keepAwakeAlways" | "keepAwakeOff"][] = [
    ["auto", "keepAwakeAuto"],
    ["always", "keepAwakeAlways"],
    ["off", "keepAwakeOff"],
  ];

  async function set(mode: KeepAwakeMode, displayOn: boolean) {
    error = await setKeepAwake(mode, displayOn);
  }
</script>

{#if app.keepAwake}
  {@const { mode, displayOn, active } = app.keepAwake}
  <fieldset class="keep-awake">
    <legend>{t("keepAwake")}</legend>
    {#each modes as [value, label] (value)}
      <label class="option">
        <input
          type="radio"
          name="keep-awake"
          {value}
          checked={mode === value}
          onchange={() => set(value, displayOn)}
        />
        {t(label)}
      </label>
    {/each}
    {#if mode !== "off"}
      <label class="option display">
        <input type="checkbox" checked={displayOn} onchange={(e) => set(mode, e.currentTarget.checked)} />
        {t("keepDisplayOn")}
      </label>
    {/if}
    <p class="help">{t("keepAwakeHelp")}</p>
    {#if active}
      <p class="active" role="status"><Coffee size={14} aria-hidden="true" />{t("keepAwakeActive")}</p>
    {/if}
    {#if error || app.keepAwake.error}
      <p class="notice error" role="alert">{error || app.keepAwake.error}</p>
    {/if}
  </fieldset>
{:else}
  <Skeleton shape="form" rows={3} label={t("keepAwake")} />
{/if}

<style>
  .keep-awake {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    border: 0;
  }

  /* The page's title in Settings. */
  legend {
    margin-bottom: 12px;
    padding: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .option {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    cursor: pointer;
  }

  .option input {
    margin: 0;
  }

  .display {
    margin-top: 4px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }

  .help {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .active {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 0 0;
    color: var(--warning);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .notice {
    margin-top: 8px;
  }
</style>
