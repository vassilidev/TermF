<script lang="ts">
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { t } from "$lib/i18n";
  import { app, setContextSettings, type ContextSettings } from "$lib/state.svelte";

  let error = $state("");

  async function save(change: Partial<ContextSettings>) {
    if (!app.contextSettings) return;
    const next = { ...app.contextSettings, ...change };
    const valid = [next.resetAt, next.warningAt].every((p) => Number.isInteger(p) && p >= 1 && p <= 100);
    error = valid ? await setContextSettings(next) : t("contextThresholds");
  }

  const value = (e: Event) => (e.currentTarget as HTMLInputElement).valueAsNumber;
</script>

{#if app.contextSettings}
  {@const { on, resetAt, warningAt } = app.contextSettings}
  <section class="context">
    <fieldset>
      <legend>{t("contextSettings")}</legend>
      <label class="option">
        <input type="checkbox" checked={on} onchange={(e) => save({ on: e.currentTarget.checked })} />
        {t("contextAuto")}
      </label>
      <div class="thresholds">
        <label>
          <span>{t("contextResetAt")}</span>
          <span class="value">
            <input
              type="number"
              min="1"
              max="100"
              value={resetAt}
              disabled={!on}
              onchange={(e) => save({ resetAt: value(e) })}
            />{t("contextPercentUnit")}
          </span>
        </label>
        <label>
          <span>{t("contextWarningAt")}</span>
          <span class="value">
            <input
              type="number"
              min="1"
              max="100"
              value={warningAt}
              onchange={(e) => save({ warningAt: value(e) })}
            />{t("contextPercentUnit")}
          </span>
        </label>
      </div>
      <p class="help">{t("contextSettingsHelp")}</p>
      {#if error}
        <p class="notice error" role="alert">{error}</p>
      {/if}
    </fieldset>
  </section>
{:else}
  <Skeleton shape="form" rows={3} label={t("contextSettings")} />
{/if}

<style>
  fieldset {
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

  .thresholds {
    display: grid;
    grid-template-columns: max-content max-content;
    align-items: center;
    gap: 6px 8px;
    margin-top: 4px;
  }

  /* Text and field line up in columns across both thresholds. */
  .thresholds label {
    display: contents;
  }

  .value {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .thresholds input {
    width: 56px;
  }

  .help {
    margin: 8px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .notice {
    margin-top: 8px;
  }
</style>
