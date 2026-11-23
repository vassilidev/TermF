<script lang="ts">
  import { approxUsd, budgetLabel } from "$lib/usage.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import CalendarClock from "@lucide/svelte/icons/calendar-clock";
  import CalendarPlus from "@lucide/svelte/icons/calendar-plus";
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
  import ScheduleForm from "$lib/ScheduleForm.svelte";
  import { clip, detail, kindLabel, toTask } from "$lib/SessionLoops.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
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
  import { app, loopClock, shortPath } from "$lib/state.svelte";
  import { focusSession } from "$lib/commands";
  import { liveLoops, tileName } from "$lib/tile-logic";

  const ROUTINES = "https://claude.ai/code/routines";

  const tasks = $derived(schedule.tasks ?? []);
  const sessionLoops = $derived(
    app.sessions.filter((s) => s.running).flatMap((session) => liveLoops(session.loops, loopClock.now).map((loop) => ({ session, loop }))),
  );
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
    schedule.draft = null;
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
    schedule.draft = null;
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
  <StatusPill {status} label={t(statuses[status])} />
{/snippet}

<div class="scheduler">
  <ViewHeader title={t("schedulerView")}>
    {#snippet actions()}
      {#if schedule.tab === "tasks" && tasks.length && editing === undefined}
        <button class="btn sm primary" onclick={() => ((schedule.draft = null), (schedule.editing = ""))}>
          <Plus size={14} aria-hidden="true" />{t("schedulerNew")}
        </button>
      {/if}
    {/snippet}
  </ViewHeader>

  <Tabs
    id="scheduler"
    label={t("schedulerView")}
    tabs={[
      { id: "tasks", label: t("schedulerTabTasks"), count: schedule.tasks ? tasks.length : null },
      { id: "others", label: t("schedulerTabOthers"), count: sessionLoops.length || null },
    ]}
    selected={schedule.tab}
    onselect={(tab) => (tab === "others" ? showOthers() : (schedule.tab = "tasks"))}
  >
    {#if schedule.error}
      <div class="banners">
        <Banner kind="error" ondismiss={() => (schedule.error = "")}>{schedule.error}</Banner>
      </div>
    {/if}

    {#if schedule.tab === "others"}
      <div class="pane">
        <div class="others">
          <section>
            <h2><Monitor size={16} aria-hidden="true" />{t("schedulerDesktopTitle")}</h2>
            <p class="muted">{t("schedulerDesktopHelp")}</p>
            {#if !schedule.desktop}
              <Skeleton shape="list" />
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
          <section>
            <h2><Repeat size={16} aria-hidden="true" />{t("loopsSchedulerTitle")}</h2>
            {#if !sessionLoops.length}
              <p class="notice">{t("loopsSchedulerEmpty")}</p>
            {:else}
              <ul class="desktop">
                {#each sessionLoops as { session, loop } (`${session.id}:${loop.id}`)}
                  {@const make = toTask(session, loop)}
                  <li>
                    <span class="name">{tileName(session.title, session.agent ?? "shell")} · {kindLabel(loop)}</span>
                    <span class="desc">{detail(loop, loopClock.now)}</span>
                    <pre class="prompt">{clip(loop.prompt) || t("loopsDefaultPrompt")}</pre>
                    <span class="mono muted">{shortPath(session.cwd)}</span>
                    <span class="loop-actions">
                      <button class="btn sm" onclick={() => focusSession(session.id)}>{t("loopsShowSession")}</button>
                      {#if make}
                        <button class="btn sm" onclick={make}><CalendarPlus size={14} aria-hidden="true" />{t("loopsToTask")}</button>
                      {/if}
                    </span>
                  </li>
                {/each}
              </ul>
            {/if}
          </section>
        </div>
      </div>
    {:else if schedule.loadError}
      <div class="pane">
        <EmptyState>
          <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{t("schedulerError", { error: schedule.loadError })}</p>
          <button class="btn" onclick={refresh}><RefreshCw size={14} aria-hidden="true" />{t("schedulerRetry")}</button>
        </EmptyState>
      </div>
    {:else if !schedule.tasks}
      <div class="split">
        <div class="list"><Skeleton shape="list" rows={6} label={t("schedulerLoading")} /></div>
      </div>
    {:else if !tasks.length && editing === undefined}
      <div class="pane">
        <EmptyState icon={CalendarClock} title={t("schedulerEmpty")} text={t("schedulerEmptyHelp")}>
          <button class="btn primary" onclick={() => ((schedule.draft = null), (schedule.editing = ""))}>
            <Plus size={14} aria-hidden="true" />{t("schedulerNew")}
          </button>
        </EmptyState>
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
                <StatusPill status="off" />
              {:else if entry.last}
                {@render status(entry.last.status)}
              {/if}
            </li>
          {/each}
        </ul>
        <section class="detail" aria-live="polite">
          {#if editing !== undefined}
            {#key `${schedule.editing}:${schedule.drafts}`}
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
              <dd>{current.mode === "scout" ? t("watchModeScout") : current.mode === "headless" ? t("schedulerModeHeadless") : t("schedulerModeTile")}</dd>
              <dt>{t("schedulerMetaModel")}</dt>
              <dd>{current.model ?? t("schedulerDefault")}</dd>
              <dt>{t("schedulerMetaEffort")}</dt>
              <dd>{current.effort ? t(efforts[current.effort] ?? "schedulerDefault") : t("schedulerDefault")}</dd>
              <dt>{t("schedulerMetaPermissions")}</dt>
              <dd>
                {current.permissions ? (permissions[current.permissions] ? t(permissions[current.permissions]) : current.permissions) : t("schedulerDefault")}
              </dd>
              {#if current.mode !== "tile"}
                <dt>{budgetLabel(t("schedulerMetaBudget"))}</dt>
                <dd>{current.budget !== null ? cost(current.budget) : "—"}</dd>
              {/if}
              <dt>{t("schedulerMetaCatchup")}</dt>
              <dd>{current.catchup ? t("schedulerCatchupOn") : t("schedulerCatchupOff")}</dd>
            </dl>

            {#if current.mode === "scout"}
              <p class="muted">{t("watchModeScoutHelp")}</p>
            {:else}
              <h3>{t("schedulerPrompt")}</h3>
              <pre class="prompt">{current.prompt}</pre>
            {/if}

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
                        {dots(duration(run), run.cost !== null && approxUsd(cost(run.cost)))}
                      </span>
                    </summary>
                    {#if run.error}
                      <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{run.error}</p>
                    {/if}
                    {#if run.output}
                      <pre class="output">{run.output}</pre>
                    {:else if run.mode !== "tile" && run.status !== "running" && !run.error}
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
  </Tabs>
</div>

<style>
  .scheduler {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .pane {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .row.elsewhere .pick {
    color: var(--muted);
  }

  .elsewhere-note {
    margin-top: 12px;
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
    font-size: var(--text-sm);
    line-height: 1.5;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  header h2 {
    margin: 0;
    font-size: var(--text-xl);
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

  .loop-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .link-row {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
