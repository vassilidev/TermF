<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import type { Item } from "$lib/library.svelte";
  import { budgetError } from "$lib/team-run-logic";
  import { usd } from "$lib/agents.svelte";

  /** The workflow and budget a team starts with; a team going on keeps its workflow (`resume`). */
  let {
    project,
    workflow = $bindable(""),
    budget = $bindable(),
    spent = 0,
    resume = false,
  }: { project: string; workflow?: string; budget: number | null; spent?: number; resume?: boolean } = $props();

  let names = $state<string[]>([]);

  $effect(() => {
    if (resume) return;
    // The shipped `squads` workflow only for a project that declares squads.
    Promise.all([
      invoke<Item[]>("library_list", { kind: "workflow", project }),
      invoke<{ declared: boolean }>("team_squads", { project }).catch(() => ({ declared: false })),
    ]).then(
      ([items, squads]) => (names = [...new Set(items.map((i) => i.name))].filter((n) => n !== "squads" || squads.declared)),
      () => (names = []),
    );
  });

  const error = $derived(budgetError(budget, spent));
  const id = `team-budget-${Math.random().toString(36).slice(2)}`;
</script>

<div class="options">
  {#if !resume}
    <label class="labelled">
      <span>{t("teamRunsWorkflow")}</span>
      <select class="field" bind:value={workflow}>
        <option value="">{t("teamRunsWorkflowDefault")}</option>
        {#each names as name (name)}<option value={name}>{name}</option>{/each}
      </select>
    </label>
  {/if}
  <label class="labelled budget">
    <span>{t("teamRunsBudget")}</span>
    <input
      class="field"
      type="number"
      min="0"
      step="0.5"
      placeholder={t(resume ? "teamRunsBudgetKeep" : "teamRunsBudgetHint")}
      aria-invalid={!!error}
      aria-describedby={error ? id : undefined}
      bind:value={budget}
    />
  </label>
  {#if error}
    <p class="notice error" {id}>
      {error === "spent" ? t("teamRunsBudgetSpent", { spent: usd(spent) }) : t("teamRunsBudgetInvalid")}
    </p>
  {/if}
</div>

<style>
  .options {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 8px 12px;
  }

  .options .labelled {
    width: 200px;
  }

  .options .budget {
    width: 150px;
  }

  .notice {
    flex-basis: 100%;
  }
</style>
