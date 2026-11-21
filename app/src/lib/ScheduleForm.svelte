<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { allRepos } from "$lib/files.svelte";
  import { t } from "$lib/i18n";
  import { describe, schedule, when, type Task, type TaskEntry } from "$lib/schedule.svelte";
  import { shortPath } from "$lib/state.svelte";

  let { entry, onclose }: { entry: TaskEntry | null; onclose: (id?: string) => void } = $props();

  const blank: Task = {
    name: "",
    prompt: "",
    dir: "",
    cron: "0 9 * * 1-5",
    mode: "headless",
    model: null,
    effort: null,
    permissions: null,
    budget: null,
    machine: schedule.machine || null,
    enabled: true,
    catchup: true,
  };
  // svelte-ignore state_referenced_locally
  const start = entry;
  let task = $state<Task>(
    start
      ? {
          name: start.name,
          prompt: start.prompt,
          dir: start.dir,
          cron: start.cron,
          mode: start.mode,
          model: start.model,
          effort: start.effort,
          permissions: start.permissions,
          budget: start.budget,
          machine: start.machine,
          enabled: start.enabled,
          catchup: start.catchup,
        }
      : { ...blank },
  );
  let runs = $state<number[]>([]);
  let cronError = $state("");
  let saving = $state(false);
  let error = $state("");

  const repos = $derived([...allRepos()].sort((a, b) => a.name.localeCompare(b.name)));
  const words = $derived(describe(task.cron));
  const presets = [
    ["schedulerPresetHourly", "0 * * * *"],
    ["schedulerPresetDaily", "0 9 * * *"],
    ["schedulerPresetWeekdays", "0 9 * * 1-5"],
    ["schedulerPresetWeekly", "0 9 * * 1"],
  ] as const;
  const efforts = [
    ["low", "schedulerEffortLow"],
    ["medium", "schedulerEffortMedium"],
    ["high", "schedulerEffortHigh"],
    ["xhigh", "schedulerEffortXhigh"],
    ["max", "schedulerEffortMax"],
  ] as const;
  const permissions = [
    ["manual", "schedulerPermManual"],
    ["acceptEdits", "schedulerPermAcceptEdits"],
    ["plan", "schedulerPermPlan"],
    ["auto", "schedulerPermAuto"],
    ["dontAsk", "schedulerPermDontAsk"],
    ["bypassPermissions", "schedulerPermBypass"],
  ] as const;

  let previewRequest = 0;
  $effect(() => {
    const cron = task.cron;
    const request = ++previewRequest;
    const timer = setTimeout(async () => {
      try {
        const next = await invoke<number[]>("schedule_preview", { cron, count: 5 });
        if (request === previewRequest) [runs, cronError] = [next, ""];
      } catch (e) {
        if (request === previewRequest) [runs, cronError] = [[], String(e)];
      }
    }, 200);
    return () => clearTimeout(timer);
  });

  async function pickDir(e: Event) {
    const select = e.currentTarget as HTMLSelectElement;
    if (select.value !== "\0other") return void (task.dir = select.value);
    // Back to the current folder while the dialog is open, and for good if it is cancelled.
    select.value = task.dir;
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir === "string") task.dir = dir;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    error = "";
    try {
      const budget = task.mode === "headless" && task.budget ? Number(task.budget) : null;
      const id = await invoke<string>("schedule_save", { id: start?.id ?? null, task: { ...task, budget } });
      onclose(id);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<form class="form" onsubmit={save}>
  <h2>{start ? t("schedulerFormEdit") : t("schedulerFormNew")}</h2>

  <label class="labelled">
    <span>{t("schedulerName")}</span>
    <input class="field" required placeholder={t("schedulerNamePlaceholder")} bind:value={task.name} />
  </label>

  <label class="labelled">
    <span>{t("schedulerDir")}</span>
    <select class="field" required value={task.dir} onchange={pickDir}>
      <option value="" disabled>{t("schedulerDirPick")}</option>
      {#if task.dir && !repos.some((r) => r.path === task.dir)}
        <option value={task.dir}>{shortPath(task.dir)}</option>
      {/if}
      {#each repos as repo (repo.path)}
        <option value={repo.path}>{repo.name} · {shortPath(repo.path)}</option>
      {/each}
      <option value={"\0other"}>{t("schedulerDirOther")}</option>
    </select>
  </label>

  <label class="labelled">
    <span>{t("schedulerPrompt")}</span>
    <textarea class="field prompt" required rows="5" placeholder={t("schedulerPromptPlaceholder")} bind:value={task.prompt}
    ></textarea>
  </label>

  <fieldset>
    <legend>{t("schedulerCron")}</legend>
    <div class="cron-row">
      <input
        class="field mono cron"
        required
        spellcheck="false"
        autocomplete="off"
        aria-label={t("schedulerCron")}
        aria-describedby="cron-help"
        bind:value={task.cron}
      />
      <div class="presets">
        {#each presets as [label, cron] (cron)}
          <button type="button" class="chip" aria-pressed={task.cron === cron} onclick={() => (task.cron = cron)}>
            {t(label)}
          </button>
        {/each}
      </div>
    </div>
    <p class="help" id="cron-help">{t("schedulerCronHelp")}</p>
    <div aria-live="polite">
      {#if cronError}
        <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{t("schedulerCronInvalid", { error: cronError })}</p>
      {:else if words || runs.length}
        <div class="preview">
          {#if words}<p class="words">{words}</p>{/if}
          {#if runs.length}
            <p class="help">{t("schedulerNextRuns")}</p>
            <ul class="runs">
              {#each runs as run (run)}<li>{when(run)}</li>{/each}
            </ul>
          {/if}
        </div>
      {/if}
    </div>
  </fieldset>

  <fieldset>
    <legend>{t("schedulerMode")}</legend>
    <div class="modes" role="radiogroup" aria-label={t("schedulerMode")}>
      {#each [["headless", "schedulerModeHeadless", "schedulerModeHeadlessHelp"], ["tile", "schedulerModeTile", "schedulerModeTileHelp"]] as const as [mode, label, help] (mode)}
        <label class="mode" class:checked={task.mode === mode}>
          <input type="radio" name="mode" value={mode} bind:group={task.mode} />
          <span class="mode-text">
            <span class="mode-name">{t(label)}</span>
            <span class="help">{t(help)}</span>
          </span>
        </label>
      {/each}
    </div>
  </fieldset>

  <label class="labelled">
    <span>{t("schedulerMachine")}</span>
    <select class="field" bind:value={() => task.machine ?? "", (v) => (task.machine = v || null)}>
      <option value={schedule.machine}>{t("schedulerMachineHere", { name: schedule.machine })}</option>
      <option value="">{t("schedulerMachineAny")}</option>
      {#if task.machine && task.machine !== schedule.machine}
        <option value={task.machine}>{t("schedulerMachineThere", { name: task.machine })}</option>
      {/if}
    </select>
    <span class="help">{t("schedulerMachineHelp")}</span>
  </label>

  <div class="grid">
    <label class="labelled">
      <span>{t("schedulerMetaModel")}</span>
      <input
        class="field"
        list="schedule-models"
        placeholder={t("schedulerDefault")}
        spellcheck="false"
        autocomplete="off"
        bind:value={() => task.model ?? "", (v) => (task.model = v.trim() || null)}
      />
      <datalist id="schedule-models">
        <option value="opus"></option>
        <option value="sonnet"></option>
        <option value="haiku"></option>
      </datalist>
    </label>
    <label class="labelled">
      <span>{t("schedulerMetaEffort")}</span>
      <select class="field" bind:value={() => task.effort ?? "", (v) => (task.effort = v || null)}>
        <option value="">{t("schedulerDefault")}</option>
        {#each efforts as [value, label] (value)}<option {value}>{t(label)}</option>{/each}
      </select>
    </label>
    <label class="labelled">
      <span>{t("schedulerMetaPermissions")}</span>
      <select class="field" bind:value={() => task.permissions ?? "", (v) => (task.permissions = v || null)}>
        <option value="">{t("schedulerDefault")}</option>
        {#each permissions as [value, label] (value)}<option {value}>{t(label)}</option>{/each}
      </select>
    </label>
    {#if task.mode === "headless"}
      <label class="labelled">
        <span>{t("schedulerBudget")}</span>
        <input
          class="field"
          type="number"
          min="0.01"
          step="0.01"
          inputmode="decimal"
          placeholder="—"
          bind:value={() => task.budget ?? "", (v) => (task.budget = v === "" || v === null ? null : Number(v))}
        />
      </label>
    {/if}
  </div>
  <p class="help">
    {task.mode === "headless" ? `${t("schedulerPermissionsHelp")} ${t("schedulerBudgetHelp")}` : ""}
  </p>

  <label class="check">
    <input type="checkbox" class="switch" role="switch" bind:checked={task.catchup} />
    <span>
      <span class="mode-name">{t("schedulerCatchup")}</span>
      <span class="help">{t("schedulerCatchupHelp")}</span>
    </span>
  </label>
  <label class="check">
    <input type="checkbox" class="switch" role="switch" bind:checked={task.enabled} />
    <span class="mode-name">{t("schedulerEnabled")}</span>
  </label>

  {#if error}
    <p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>
  {/if}
  <div class="buttons">
    <button type="button" class="btn sm" onclick={() => onclose()}>{t("cancel")}</button>
    <button type="submit" class="btn sm primary" disabled={saving || !!cronError}>
      {#if saving}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
      {t("schedulerSave")}
    </button>
  </div>
</form>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .labelled {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .prompt {
    height: auto;
    min-height: 96px;
    padding: 6px 8px;
    line-height: 1.5;
    resize: vertical;
  }

  fieldset {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }

  legend {
    margin-bottom: 4px;
    padding: 0;
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .cron-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .cron {
    width: 180px;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    height: 24px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--panel);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
  }

  .chip:hover {
    color: var(--text);
    background: var(--hover);
  }

  .chip[aria-pressed="true"] {
    border-color: var(--accent);
    color: var(--accent-ink);
  }

  .help {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 400;
    line-height: 1.5;
  }

  .help:empty {
    display: none;
  }

  .preview {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-radius: var(--radius-lg);
    background: var(--hover);
  }

  .words {
    margin: 0;
    font-weight: 600;
  }

  .runs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .modes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .mode {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    cursor: pointer;
  }

  .mode.checked {
    border-color: var(--accent);
    background: var(--selected);
  }

  .mode input {
    margin: 2px 0 0;
  }

  .mode-text,
  .check > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .mode-name {
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 10px 12px;
  }

  .check {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    cursor: pointer;
  }

  .check input {
    margin-top: 2px;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
