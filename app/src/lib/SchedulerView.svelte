<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CalendarClock from "@lucide/svelte/icons/calendar-clock";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Cloud from "@lucide/svelte/icons/cloud";
  import Copy from "@lucide/svelte/icons/copy";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Info from "@lucide/svelte/icons/info";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Monitor from "@lucide/svelte/icons/monitor";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Play from "@lucide/svelte/icons/play";
  import Plus from "@lucide/svelte/icons/plus";
  import Repeat from "@lucide/svelte/icons/repeat";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import X from "@lucide/svelte/icons/x";
  import ScheduleForm from "$lib/ScheduleForm.svelte";
  import { t } from "$lib/i18n";
  import {
    cost,
    describe,
    loadHistory,
    refresh,
    runsHere,
    schedule,
    when,
    type DesktopTask,
    type Execution,
    type ExecutionStatus,
    type TaskEntry,
  } from "$lib/schedule.svelte";
  import { shortPath } from "$lib/state.svelte";

  const ROUTINES = "https://claude.ai/code/routines";

  const tasks = $derived(schedule.tasks ?? []);
  const current = $derived(tasks.find((e) => e.id === schedule.selected) ?? tasks[0]);
  const editing = $derived(
    schedule.editing === null ? undefined : schedule.editing === "" ? null : tasks.find((e) => e.id === schedule.editing),
  );
  let busy = $state<"" | "run" | "delete" | "install" | "enable">("");
  let confirming = $state(false);
  let copied = $state(false);

  const statuses: Record<ExecutionStatus, Parameters<typeof t>[0]> = {
    running: "schedulerStatusRunning",
    succeeded: "schedulerStatusSucceeded",
    failed: "schedulerStatusFailed",
    opened: "schedulerStatusOpened",
  };
  const triggers = {
    schedule: "schedulerTriggerSchedule",
    catchup: "schedulerTriggerCatchup",
    manual: "schedulerTriggerManual",
  } as const;
  const efforts: Record<string, Parameters<typeof t>[0]> = {
    low: "schedulerEffortLow",
    medium: "schedulerEffortMedium",
    high: "schedulerEffortHigh",
    xhigh: "schedulerEffortXhigh",
    max: "schedulerEffortMax",
  };
  const permissions: Record<string, Parameters<typeof t>[0]> = {
    manual: "schedulerPermManual",
    acceptEdits: "schedulerPermAcceptEdits",
    plan: "schedulerPermPlan",
    auto: "schedulerPermAuto",
    dontAsk: "schedulerPermDontAsk",
    bypassPermissions: "schedulerPermBypass",
  };

  function select(id: string) {
    schedule.selected = id;
    schedule.editing = null;
    confirming = false;
    loadHistory();
  }

  const dots = (...parts: (string | false)[]) => parts.filter(Boolean).join(" · ");

  function duration(run: Execution): string {
    if (run.finished === null || run.status === "opened") return "";
    const s = run.finished - run.started;
    return s < 120 ? t("schedulerSeconds", { n: s }) : t("schedulerMinutes", { n: Math.round(s / 60) });
  }

  async function act(kind: typeof busy, call: () => Promise<unknown>) {
    busy = kind;
    schedule.error = "";
    try {
      await call();
    } catch (e) {
      schedule.error = String(e);
    } finally {
      busy = "";
    }
  }

  const runNow = (entry: TaskEntry) =>
    act("run", async () => {
      await invoke<Execution>("schedule_run", { id: entry.id });
      await loadHistory();
    });

  const remove = (entry: TaskEntry) =>
    act("delete", async () => {
      await invoke("schedule_delete", { id: entry.id });
      confirming = false;
      schedule.selected = null;
      await refresh();
    });

  // The switch shows what is true afterwards, refused or not.
  const install = async (entry: TaskEntry, box: HTMLInputElement) => {
    await act("install", async () => {
      entry.installed = await invoke<boolean>("schedule_install", { id: entry.id, on: box.checked });
    });
    box.checked = entry.installed;
  };

  const enable = async (entry: TaskEntry, box: HTMLInputElement) => {
    await act("enable", async () => {
      const { id, installed: _i, next: _n, last: _l, ...task } = entry;
      await invoke("schedule_save", { id, task: { ...task, enabled: box.checked } });
      await refresh();
    });
    box.checked = current?.enabled ?? false;
  };

  async function closeForm(id?: string) {
    schedule.editing = null;
    if (id) {
      schedule.selected = id;
      await refresh();
    }
  }

  function showOthers() {
    schedule.tab = "others";
    if (!schedule.desktop) {
      invoke<DesktopTask[]>("schedule_desktop")
        .then((list) => (schedule.desktop = list))
        .catch((e) => (schedule.error = String(e)));
    }
  }

  async function copyLink() {
    await navigator.clipboard.writeText(ROUTINES).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

{#snippet status(status: ExecutionStatus)}
  <span class="status {status}">{t(statuses[status])}</span>
{/snippet}

<div class="scheduler">
  <div class="tabs" role="tablist" aria-label={t("schedulerView")}>
    <button role="tab" aria-selected={schedule.tab === "tasks"} onclick={() => (schedule.tab = "tasks")}>
      {t("schedulerTabTasks")}{#if schedule.tasks}<span class="count">{tasks.length}</span>{/if}
    </button>
    <button role="tab" aria-selected={schedule.tab === "others"} onclick={showOthers}>{t("schedulerTabOthers")}</button>
    {#if schedule.tab === "tasks" && tasks.length && editing === undefined}
      <button class="btn sm primary new" onclick={() => (schedule.editing = "")}>
        <Plus size={14} aria-hidden="true" />{t("schedulerNew")}
      </button>
    {/if}
  </div>

  {#if schedule.error}
    <div class="notices">
      <div class="notice error banner" role="alert">
        <CircleAlert size={14} aria-hidden="true" />
        <span>{schedule.error}</span>
        <button class="icon-btn sm" onclick={() => (schedule.error = "")} aria-label={t("dismiss")} title={t("dismiss")}>
          <X size={14} aria-hidden="true" />
        </button>
      </div>
    </div>
  {/if}

  {#if schedule.tab === "others"}
    <div class="pane">
      <div class="others">
        <section>
          <h2><Monitor size={16} aria-hidden="true" />{t("schedulerDesktopTitle")}</h2>
          <p class="muted">{t("schedulerDesktopHelp")}</p>
          {#if !schedule.desktop}
            <p class="loading muted" role="status"><LoaderCircle size={16} aria-hidden="true" class="spin" /></p>
          {:else if !schedule.desktop.length}
            <p class="notice">{t("schedulerDesktopEmpty")}</p>
          {:else}
            <ul class="desktop">
              {#each schedule.desktop as task (task.path)}
                <li>
                  <span class="name">{task.name}</span>
                  {#if task.description}<span class="desc">{task.description}</span>{/if}
                  <pre class="prompt">{task.prompt}</pre>
                  <span class="mono muted">{shortPath(task.path)}</span>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
        <section>
          <h2><Cloud size={16} aria-hidden="true" />{t("schedulerCloudTitle")}</h2>
          <p class="muted">{t("schedulerCloudHelp")}</p>
          <span class="link-row">
            <span class="mono">{ROUTINES}</span>
            <button class="icon-btn sm" onclick={copyLink} aria-label={t("schedulerCopyLink")} title={t("schedulerCopyLink")}>
              <Copy size={13} aria-hidden="true" />
            </button>
            <button
              class="icon-btn sm"
              onclick={() => invoke("open_url", { url: ROUTINES }).catch((e) => (schedule.error = String(e)))}
              aria-label={t("schedulerOpenLink")}
              title={t("schedulerOpenLink")}
            >
              <ExternalLink size={13} aria-hidden="true" />
            </button>
            {#if copied}<span class="muted small" role="status">{t("schedulerCopied")}</span>{/if}
          </span>
        </section>
        <section>
          <h2><Repeat size={16} aria-hidden="true" />{t("schedulerLoopsTitle")}</h2>
          <p class="muted">{t("schedulerLoopsHelp")}</p>
        </section>
      </div>
    </div>
  {:else if schedule.loadError}
    <div class="pane">
      <div class="empty">
        <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{t("schedulerError", { error: schedule.loadError })}</p>
        <button class="btn" onclick={refresh}><RefreshCw size={14} aria-hidden="true" />{t("schedulerRetry")}</button>
      </div>
    </div>
  {:else if !schedule.tasks}
    <div class="pane">
      <p class="loading muted" role="status">
        <LoaderCircle size={16} aria-hidden="true" class="spin" />{t("schedulerLoading")}
      </p>
    </div>
  {:else if !tasks.length && editing === undefined}
    <div class="pane">
      <div class="empty">
        <CalendarClock size={20} aria-hidden="true" />
        <h2>{t("schedulerEmpty")}</h2>
        <p>{t("schedulerEmptyHelp")}</p>
        <button class="btn primary" onclick={() => (schedule.editing = "")}>
          <Plus size={14} aria-hidden="true" />{t("schedulerNew")}
        </button>
      </div>
    </div>
  {:else}
    <div class="split">
      <ul class="list" aria-label={t("schedulerTabTasks")}>
        {#each tasks as entry (entry.id)}
          <li class="row" class:current={entry === current && editing === undefined} class:elsewhere={!runsHere(entry)}>
            <button class="pick" aria-current={entry === current} onclick={() => select(entry.id)}>
              <span class="name">{entry.name}</span>
              <span class="desc">
                {#if runsHere(entry)}
                  {dots(describe(entry.cron) ?? entry.cron, entry.next !== null && t("schedulerNext", { when: when(entry.next) }))}
                {:else}
                  {dots(describe(entry.cron) ?? entry.cron, t("schedulerRunsOn", { machine: entry.machine ?? "" }))}
                {/if}
              </span>
            </button>
            {#if !runsHere(entry)}{:else if !entry.enabled}
              <span class="status off">{t("schedulerOff")}</span>
            {:else if entry.last}
              {@render status(entry.last.status)}
            {/if}
          </li>
        {/each}
      </ul>
      <section class="detail" aria-live="polite">
        {#if editing !== undefined}
          {#key schedule.editing}
            <ScheduleForm entry={editing} onclose={closeForm} />
          {/key}
        {:else if current}
          <header>
            <h2>{current.name}</h2>
            <p class="muted">
              {#if describe(current.cron)}{describe(current.cron)}{" · "}{/if}<span class="mono">{current.cron}</span
              >{#if current.next !== null}{" · "}{t("schedulerNext", { when: when(current.next) })}{/if}
            </p>
          </header>

          {#if !runsHere(current)}
            <p class="notice elsewhere-note">
              <Info size={14} aria-hidden="true" />{t("schedulerElsewhere", { machine: current.machine ?? "" })}
            </p>
          {/if}
          <div class="actions">
            <button class="btn sm primary" onclick={() => runNow(current)} disabled={!!busy || !runsHere(current)}>
              {#if busy === "run"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Play
                  size={14}
                  aria-hidden="true"
                />{/if}
              {t("schedulerRunNow")}
            </button>
            <button class="btn sm" onclick={() => (schedule.editing = current.id)}>
              <Pencil size={14} aria-hidden="true" />{t("schedulerEdit")}
            </button>
            <button class="btn sm" aria-expanded={confirming} onclick={() => (confirming = true)}>
              <Trash2 size={14} aria-hidden="true" />{t("schedulerDelete")}
            </button>
          </div>
          {#if confirming}
            <div class="ask" role="alert">
              <span>{t("schedulerDeleteConfirm", { name: current.name })}</span>
              <div class="buttons">
                <button class="btn sm" onclick={() => (confirming = false)}>{t("cancel")}</button>
                <button class="btn sm danger" onclick={() => remove(current)} disabled={!!busy}>
                  {#if busy === "delete"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
                  {t("schedulerDelete")}
                </button>
              </div>
            </div>
          {/if}

          <div class="switches">
            <label class="check">
              <input
                type="checkbox"
                class="switch"
                role="switch"
                checked={current.enabled}
                disabled={!!busy}
                onchange={(e) => enable(current, e.currentTarget)}
              />
              <span class="label">{t("schedulerEnabled")}</span>
            </label>
            <label class="check">
              <input
                type="checkbox"
                class="switch"
                role="switch"
                checked={current.installed}
                disabled={!!busy || ((current.mode === "tile" || !runsHere(current)) && !current.installed)}
                onchange={(e) => install(current, e.currentTarget)}
              />
              <span>
                <span class="label">{t("schedulerClosed")}</span>
                <span class="help">
                  {!runsHere(current)
                    ? t("schedulerRunsOn", { machine: current.machine ?? "" })
                    : current.mode === "tile"
                      ? t("schedulerClosedTile")
                      : t("schedulerClosedHelp")}
                </span>
              </span>
            </label>
          </div>

          <dl class="meta">
            <dt>{t("schedulerMachine")}</dt>
            <dd>{current.machine ?? t("schedulerMachineAny")}</dd>
            <dt>{t("schedulerMetaDir")}</dt>
            <dd class="mono">{shortPath(current.dir)}</dd>
            <dt>{t("schedulerMetaMode")}</dt>
            <dd>{current.mode === "headless" ? t("schedulerModeHeadless") : t("schedulerModeTile")}</dd>
            <dt>{t("schedulerMetaModel")}</dt>
            <dd>{current.model ?? t("schedulerDefault")}</dd>
            <dt>{t("schedulerMetaEffort")}</dt>
            <dd>{current.effort ? t(efforts[current.effort] ?? "schedulerDefault") : t("schedulerDefault")}</dd>
            <dt>{t("schedulerMetaPermissions")}</dt>
            <dd>
              {current.permissions ? (permissions[current.permissions] ? t(permissions[current.permissions]) : current.permissions) : t("schedulerDefault")}
            </dd>
            {#if current.mode === "headless"}
              <dt>{t("schedulerMetaBudget")}</dt>
              <dd>{current.budget !== null ? cost(current.budget) : "—"}</dd>
            {/if}
            <dt>{t("schedulerMetaCatchup")}</dt>
            <dd>{current.catchup ? t("schedulerCatchupOn") : t("schedulerCatchupOff")}</dd>
          </dl>

          <h3>{t("schedulerPrompt")}</h3>
          <pre class="prompt">{current.prompt}</pre>

          <h3>{t("schedulerHistory")}</h3>
          <ol class="history">
            {#each schedule.history as run, i (run.id)}
              <li>
                <details open={i === 0 && run.status !== "opened"}>
                  <summary>
                    {@render status(run.status)}
                    <time datetime={new Date(run.started * 1000).toISOString()}>{when(run.started)}</time>
                    <span class="muted">{t(triggers[run.trigger])}</span>
                    <span class="facts muted">
                      {dots(duration(run), run.cost !== null && cost(run.cost))}
                    </span>
                  </summary>
                  {#if run.error}
                    <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{run.error}</p>
                  {/if}
                  {#if run.output}
                    <pre class="output">{run.output}</pre>
                  {:else if run.mode === "headless" && run.status !== "running" && !run.error}
                    <p class="muted small">{t("schedulerNoOutput")}</p>
                  {/if}
                </details>
              </li>
            {:else}
              <li class="muted none">{t("schedulerHistoryEmpty")}</li>
            {/each}
          </ol>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .scheduler {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  .tabs [role="tab"] {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 38px;
    padding: 0 10px;
    border: 0;
    background: none;
    color: var(--muted);
    font-weight: 500;
    box-shadow: inset 0 -2px 0 transparent;
    transition:
      color var(--dur-fast) ease-out,
      box-shadow var(--dur-fast) ease-out;
  }

  .tabs [role="tab"]:hover {
    color: var(--text);
  }

  .tabs [role="tab"][aria-selected="true"] {
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .new {
    margin-left: auto;
  }

  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    line-height: 16px;
    text-align: center;
  }

  .notices {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .banner {
    align-items: center;
  }

  .banner > span {
    flex: 1;
  }

  .banner .icon-btn {
    color: inherit;
  }

  .pane {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin: 40px auto;
  }

  :global(.spin) {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.spin) {
      animation: none;
    }
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    max-width: 480px;
    margin: 48px auto 0;
    color: var(--muted);
    text-align: center;
  }

  .empty > :global(svg) {
    color: var(--accent-ink);
  }

  .empty h2 {
    margin: 8px 0 6px;
    color: var(--text);
    font-size: var(--text-md);
  }

  .empty p {
    margin: 0;
    line-height: 1.5;
  }

  .empty .btn {
    margin-top: 14px;
  }

  .split {
    display: grid;
    flex: 1;
    grid-template-columns: 340px minmax(0, 1fr);
    min-height: 0;
  }

  .list {
    min-height: 0;
    margin: 0;
    padding: 8px 6px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--chrome);
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-right: 8px;
    border-radius: var(--radius);
  }

  .row:hover {
    background: var(--hover);
  }

  .row.current {
    background: var(--selected);
  }

  .row.elsewhere .pick {
    opacity: 0.6;
  }

  .elsewhere-note {
    margin-top: 12px;
  }

  .pick {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    text-align: left;
  }

  .name {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .desc {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-xs);
    line-height: 1.5;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    height: 20px;
    padding: 0 7px;
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
    white-space: nowrap;
  }

  .running {
    background: var(--working-soft);
    color: var(--working);
  }

  .succeeded,
  .opened {
    background: var(--done-soft);
    color: var(--done);
  }

  .failed {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .off {
    background: var(--hover);
    color: var(--muted);
  }

  .detail {
    min-width: 0;
    min-height: 0;
    padding: 20px 28px 32px;
    overflow-y: auto;
  }

  .detail > :global(*) {
    max-width: 820px;
  }

  header h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  header p {
    margin: 4px 0 0;
    font-size: var(--text-sm);
  }

  h3 {
    margin: 20px 0 6px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 16px;
  }

  .ask {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 12px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    font-size: var(--text-sm);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .switches {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 18px;
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

  .check > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .label {
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .help {
    max-width: 620px;
    color: var(--muted);
    font-size: var(--text-xs);
    line-height: 1.5;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    align-items: baseline;
    gap: 6px 16px;
    margin: 20px 0 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .mono {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }

  .small {
    margin: 0;
    font-size: var(--text-sm);
  }

  pre {
    margin: 0;
    padding: 10px 12px;
    overflow-x: auto;
    border-radius: var(--radius);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .output {
    max-height: 360px;
    overflow-y: auto;
  }

  .history {
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .history li {
    border-bottom: 1px solid var(--border);
  }

  .history .none {
    padding: 6px 0;
    border: 0;
  }

  details > :not(summary) {
    margin-bottom: 10px;
  }

  summary {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 0;
    cursor: pointer;
    list-style: none;
  }

  summary::-webkit-details-marker {
    display: none;
  }

  summary time {
    font-variant-numeric: tabular-nums;
  }

  .facts {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
  }

  .others {
    display: flex;
    flex-direction: column;
    gap: 28px;
    max-width: 820px;
    padding: 20px 28px 32px;
  }

  .others h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 6px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .others h2 :global(svg) {
    color: var(--accent-ink);
  }

  .others p {
    margin: 0 0 10px;
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  .others .loading {
    margin: 12px 0;
    justify-content: flex-start;
  }

  .desktop {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .desktop li {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .desktop .desc {
    white-space: normal;
  }

  .link-row {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
