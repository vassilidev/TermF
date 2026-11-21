<script lang="ts">
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
          <input
            type="number"
            min="1"
            max="100"
            value={resetAt}
            disabled={!on}
            onchange={(e) => save({ resetAt: value(e) })}
          />
          <span>%</span>
        </label>
        <label>
          <span>{t("contextWarningAt")}</span>
          <input type="number" min="1" max="100" value={warningAt} onchange={(e) => save({ warningAt: value(e) })} />
          <span>%</span>
        </label>
      </div>
      <p class="help">{t("contextSettingsHelp")}</p>
      {#if error}
        <p class="notice error" role="alert">{error}</p>
      {/if}
    </fieldset>
  </section>
{/if}

<style>
  .context {
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  fieldset {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: 6px;
    padding: 0;
    font-weight: 600;
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
    grid-template-columns: max-content max-content max-content;
    align-items: center;
    gap: 6px 8px;
    margin-top: 4px;
  }

  /* Text, field and unit line up in columns across both thresholds. */
  .thresholds label {
    display: contents;
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
