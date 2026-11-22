<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { t } from "$lib/i18n";
  import { budgetError, editRouting, routeBudget, runs, type Routing, type TeamRun, type WorkflowView } from "$lib/team-run-logic";
  import { stepTitle, upsertRun } from "$lib/team.svelte";

  /** Changes the CTO's routing before it applies: the workflow, its optional steps, the squads, the budget. */
  let { run, routing, onclose }: { run: TeamRun; routing: Routing; onclose: () => void } = $props();

  let workflows = $state<WorkflowView[]>([]);
  let squads = $state<string[]>([]);
  // The form starts from the CTO's routing; the run keeps its own until this one is applied.
  // svelte-ignore state_referenced_locally
  let name = $state(routing.workflow);
  // svelte-ignore state_referenced_locally
  let picked = $state<string[]>([...routing.squads]);
  // svelte-ignore state_referenced_locally
  let budget = $state<number | null>(routeBudget(run.budget, routing.budget_usd));
  let running = $state<string[]>([]);
  let busy = $state(false);
  let error = $state("");

  const workflow = $derived(workflows.find((w) => w.name === name));
  const optional = $derived(workflow?.steps.filter((s) => s.optional) ?? []);

  $effect(() => {
    let cancelled = false;
    Promise.all([
      invoke<WorkflowView[]>("team_workflows", { run: run.run }),
      invoke<{ declared: boolean; squads: { id: string }[] }>("team_squads", {
        project: run.target.project,
      }),
    ]).then(
      ([list, team]) => {
        if (cancelled) return;
        workflows = list;
        squads = team.declared ? team.squads.map((s) => s.id) : [];
        const own = list.find((w) => w.name === routing.workflow);
        running = (own?.steps ?? []).filter((s) => s.optional && runs(s, routing)).map((s) => s.id);
      },
      (e) => (error = String(e)),
    );
    return () => (cancelled = true);
  });

  /** Another workflow starts from its own optional steps. */
  function pickWorkflow(next: string) {
    name = next;
    const chosen = workflows.find((w) => w.name === next);
    running = (chosen?.steps ?? []).filter((s) => s.optional && s.on).map((s) => s.id);
  }

  const toggle = (list: string[], id: string) => (list.includes(id) ? list.filter((x) => x !== id) : [...list, id]);

  async function apply() {
    if (!workflow) return;
    busy = true;
    error = "";
    try {
      const next = editRouting(routing, workflow, running, picked);
      upsertRun(await invoke<TeamRun>("team_route", { run: run.run, routing: next, budget: budget ?? undefined }));
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="edit">
  <div class="row">
    <label class="labelled">
      <span>{t("ctoEntryWorkflow")}</span>
      <select class="field" value={name} onchange={(e) => pickWorkflow(e.currentTarget.value)}>
        {#each workflows as w (w.name)}<option value={w.name}>{w.name}</option>{/each}
      </select>
    </label>
    <label class="labelled budget">
      <span>{t("ctoEntryRunBudget")}</span>
      <input class="field" type="number" min="0" step="0.5" bind:value={budget} />
    </label>
  </div>
  {#if workflow?.description}<p class="help">{workflow.description}</p>{/if}

  <fieldset>
    <legend>{t("ctoEntryOptional")}</legend>
    {#if optional.length}
      <div class="chips">
        {#each optional as step (step.id)}
          <label class="chip" class:on={running.includes(step.id)}>
            <input type="checkbox" checked={running.includes(step.id)} onchange={() => (running = toggle(running, step.id))} />
            <span>{stepTitle(step.id)}</span>{#if step.role && step.role !== step.id}<span class="muted">{step.role}</span>{/if}
          </label>
        {/each}
      </div>
    {:else}
      <p class="help">{t("ctoEntryNoOptional")}</p>
    {/if}
  </fieldset>

  {#if squads.length}
    <fieldset>
      <legend>{t("ctoEntrySquads")}</legend>
      <div class="chips">
        {#each squads as id (id)}
          <label class="chip" class:on={picked.includes(id)}>
            <input type="checkbox" checked={picked.includes(id)} onchange={() => (picked = toggle(picked, id))} />
            {id}
          </label>
        {/each}
      </div>
      <p class="help">{t("ctoEntrySquadsHelp")}</p>
    </fieldset>
  {/if}

  {#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
  <div class="buttons">
    <button class="btn sm" onclick={onclose}>{t("cancel")}</button>
    <button class="btn sm primary" disabled={busy || !workflow || !!budgetError(budget)} onclick={apply}>
      {#if busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("ctoEntryApply")}
    </button>
  </div>
</div>

<style>
  .edit {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 8px 12px;
  }

  .row .labelled {
    width: 200px;
  }

  .row .budget {
    width: 150px;
  }

  .field {
    background: var(--panel);
  }

  .help {
    margin: 0;
    color: var(--muted);
    line-height: 1.5;
  }

  fieldset {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 10px 12px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  legend {
    padding: 0 6px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-pill);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .chip input {
    margin: 0;
  }

  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .muted {
    color: var(--muted);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
