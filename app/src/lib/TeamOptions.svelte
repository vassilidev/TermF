<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import type { Item } from "$lib/library.svelte";
  import { budgetError } from "$lib/team-run-logic";
  import { budgetUsd } from "$lib/agents.svelte";
  import { budgetLabel } from "$lib/usage.svelte";

  /** The workflow and budget a team starts with; a team going on keeps its workflow (`resume`). */
  let {
    project,
    workflow = $bindable(""),
    budget = $bindable(),
    spent = 0,
    resume = false,
    required = false,
  }: { project: string; workflow?: string; budget: number | null; spent?: number; resume?: boolean; required?: boolean } =
    $props();

  let names = $state<string[]>([]);

  // A late answer for the project before never replaces this one's; a workflow this project lacks is dropped.
  $effect(() => {
    if (resume) return;
    let cancelled = false;
    // The shipped `squads` workflow only for a project that declares squads.
    Promise.all([
      invoke<Item[]>("library_list", { kind: "workflow", project }),
      invoke<{ declared: boolean }>("team_squads", { project }).catch(() => ({ declared: false })),
    ])
      .then(([items, squads]) => [...new Set(items.map((i) => i.name))].filter((n) => n !== "squads" || squads.declared))
      .catch(() => [] as string[])
      .then((list) => {
        if (cancelled) return;
        names = list;
        if (!list.includes(workflow)) workflow = "";
      });
    return () => (cancelled = true);
  });

  const error = $derived(budgetError(budget, spent, required));
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
    <span>{budgetLabel(t("teamRunsBudget"))}</span>
    <input
      class="field"
      type="number"
      min="0"
      step="0.5"
      placeholder={required ? "" : t(resume ? "teamRunsBudgetKeep" : "teamRunsBudgetHint")}
      aria-invalid={error === "invalid" || error === "spent"}
      aria-describedby={error ? id : undefined}
      bind:value={budget}
    />
  </label>
  {#if error}
    <p class="notice" class:error={error !== "missing"} {id}>
      {error === "invalid" ? t("teamRunsBudgetInvalid") : t("teamRunsBudgetSpent", { spent: budgetUsd(spent) })}
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
