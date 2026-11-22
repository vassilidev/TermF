<script lang="ts">
  import { t } from "$lib/i18n";
  import type { Param } from "$lib/ops-logic";

  let { params, values = $bindable() }: { params: Param[]; values: Record<string, string> } = $props();
</script>

{#if params.length}
  <fieldset>
    <legend>{t("opsParams")}</legend>
    {#each params as p (p.name)}
      <label class="labelled">
        <span><span class="mono">{p.name}</span>{#if p.required} *{/if} <span class="muted">{p.description}</span></span>
        {#if p.type === "choice" || p.type === "command"}
          <select class="field mono" bind:value={values[p.name]}>
            {#if !p.required}<option value="">—</option>{/if}
            {#each p.values ?? [] as v (v)}<option value={v}>{v}</option>{/each}
          </select>
        {:else}
          <input
            class="field mono"
            inputmode={p.type === "int" ? "numeric" : undefined}
            placeholder={p.default ? t("opsDefault", { value: p.default }) : ""}
            bind:value={values[p.name]}
          />
        {/if}
      </label>
    {/each}
  </fieldset>
{/if}

<style>
  fieldset {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 10px 16px;
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: 8px;
    font-size: var(--text-sm);
    font-weight: 600;
  }
</style>
