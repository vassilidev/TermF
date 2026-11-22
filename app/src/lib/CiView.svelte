<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Ban from "@lucide/svelte/icons/ban";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Copy from "@lucide/svelte/icons/copy";
  import FileText from "@lucide/svelte/icons/file-text";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Rocket from "@lucide/svelte/icons/rocket";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import Workflow from "@lucide/svelte/icons/workflow";
  import CiLog from "$lib/CiLog.svelte";
  import CiStatus from "$lib/CiStatus.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { ago, t } from "$lib/i18n";
  import { currentProject, scopeRepos } from "$lib/files.svelte";
  import { newProject } from "$lib/newproject.svelte";
  import { editProject } from "$lib/project.svelte";
  import { showSettings } from "$lib/settings.svelte";
  import { onScreen } from "$lib/state.svelte";
  import {
    ci,
    duration,
    outcome,
    type Input,
    type RunDetail,
    type RunPage,
    type Workflow as Flow,
    type WorkflowRun,
  } from "$lib/ci.svelte";

  const statuses = [
    ["", "ciStatusAll"],
    ["in_progress", "ciRunning"],
    ["queued", "ciQueued"],
    ["success", "ciPassed"],
    ["failure", "ciFailed"],
    ["cancelled", "ciCancelled"],
  ] as const;

  /** null while the first page of the repository loads. */
  let runs = $state<WorkflowRun[] | null>(null);
  let total = $state(0);
  let pages = $state(1);
  let loading = $state(false);
  let listError = $state("");
  let selected = $state<number | null>(null);
  let detail = $state<RunDetail | null>(null);
  let detailError = $state("");
  /** Jobs whose steps are shown, and the job whose log is. */
  let expanded = $state<number[]>([]);
  let logOf = $state<number | null>(null);
  let busy = $state<"" | "rerun" | "failed" | "cancel" | "dispatch">("");
  let error = $state("");
  let done = $state("");
  /** Asked before cancelling the run shown, which cannot be undone. */
  let confirmCancel = $state(false);
  let form = $state<{
    /** The repository it was opened for. */
    repo: string;
    workflows: Flow[] | null;
    workflow: string;
    ref: string;
    inputs: Input[] | null;
    values: Record<string, string>;
    error: string;
  } | null>(null);

  const repos = $derived(scopeRepos());
  const current = $derived(runs?.find((r) => r.id === selected) ?? runs?.[0]);

  // A repository out of the current project falls back to the project's first one.
  $effect(() => {
    if (!repos.some((r) => r.path === ci.repo)) ci.repo = repos[0]?.path ?? "";
  });

  $effect(() => {
    void ci.repo;
    untrack(() => {
      if (form && form.repo !== ci.repo) form = null;
      load();
    });
  });

  $effect(() => {
    const run = current;
    void run?.updated;
    void run?.status;
    untrack(() => loadDetail(run));
  });

  // While the view is on screen: a look every 30 s, cheap since GitHub answers "unchanged" (ETag).
  $effect(() => {
    if (!onScreen("ci")) return;
    const timer = setInterval(() => document.visibilityState === "visible" && refresh(), 30_000);
    return () => clearInterval(timer);
  });

  $effect(() => {
    if (ci.dispatch) untrack(openForm);
  });

  const filters = () => ({
    repo: ci.repo,
    branch: ci.branch.trim() || null,
    status: ci.status || null,
    pr: /^\d+$/.test(ci.pr.trim()) ? Number(ci.pr.trim()) : null,
  });

  let request = 0;

  /**
   * Pages 1 to `count`, newest first; only the latest request fills the list. A failed `load` replaces the list
   * with its error, a failed `more` shows it above, a failed `refresh` keeps quiet.
   */
  async function fetchPages(count: number, mode: "load" | "refresh" | "more") {
    if (!ci.repo) return;
    const r = ++request;
    loading = true;
    if (mode === "load") listError = "";
    try {
      const found: WorkflowRun[] = [];
      let page: RunPage | null = null;
      for (let n = 1; n <= count; n++) {
        page = await invoke<RunPage>("ci_runs", { ...filters(), page: n });
        found.push(...page.runs.filter((run) => !found.some((f) => f.id === run.id)));
      }
      if (r !== request) return;
      [runs, total, pages, listError] = [found, page?.total ?? 0, count, ""];
    } catch (e) {
      if (r === request && mode === "load") listError = String(e);
      if (r === request && mode === "more") error = String(e);
    } finally {
      if (r === request) loading = false;
    }
  }

  function load() {
    runs = null;
    selected = null;
    return fetchPages(1, "load");
  }

  const refresh = () => fetchPages(pages, runs === null ? "load" : "refresh");
  const more = () => fetchPages(pages + 1, "more");

  let detailRequest = 0;

  async function loadDetail(run: WorkflowRun | undefined) {
    const r = ++detailRequest;
    if (detail?.run.id !== run?.id) {
      [detail, detailError, expanded, logOf, confirmCancel] = [null, "", [], null, false];
    }
    if (!run) return;
    try {
      const found = await invoke<RunDetail>("ci_run", { repo: ci.repo, id: run.id });
      if (r !== detailRequest) return;
      // A failed job opens by itself, the first time only.
      if (!detail) expanded = found.jobs.filter((j) => outcome(j) === "failed").map((j) => j.id);
      detail = found;
    } catch (e) {
      if (r === detailRequest) detailError = String(e);
    }
  }

  async function act(kind: typeof busy, run: () => Promise<unknown>, message: string) {
    [busy, error, done] = [kind, "", ""];
    try {
      await run();
      done = message;
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  const rerun = (run: WorkflowRun, failed: boolean) =>
    act(failed ? "failed" : "rerun", () => invoke("ci_rerun", { repo: ci.repo, id: run.id, failed }), t("ciRerunDone"));

  const cancel = (run: WorkflowRun) => {
    confirmCancel = false;
    return act("cancel", () => invoke("ci_cancel", { repo: ci.repo, id: run.id }), t("ciCancelDone"));
  };

  async function copy(url: string) {
    try {
      await navigator.clipboard.writeText(url);
      done = t("runLinkCopied");
    } catch (e) {
      error = String(e);
    }
  }

  function toggle(job: number) {
    expanded = expanded.includes(job) ? expanded.filter((j) => j !== job) : [...expanded, job];
  }

  async function openForm() {
    ci.dispatch = false;
    if (!ci.repo) return;
    form = { repo: ci.repo, workflows: null, workflow: "", ref: ci.branch.trim(), inputs: null, values: {}, error: "" };
    const f = form;
    try {
      const workflows = await invoke<Flow[]>("ci_workflows", { repo: f.repo });
      if (form === f) f.workflows = workflows.filter((w) => w.state === "active" && w.path.startsWith(".github/"));
    } catch (e) {
      if (form === f) f.error = String(e);
    }
  }

  const initial = (i: Input) =>
    i.default ?? (i.kind === "boolean" ? "false" : i.kind === "choice" ? (i.options[0] ?? "") : "");

  async function readInputs() {
    const f = form;
    if (!f?.workflow) return;
    const asked = { workflow: f.workflow, ref: f.ref.trim() };
    // Only the answer for what the form still shows counts: an older one may come last.
    const current = () => form === f && f.workflow === asked.workflow && f.ref.trim() === asked.ref;
    [f.inputs, f.error] = [null, ""];
    try {
      const path = f.workflows?.find((w) => String(w.id) === asked.workflow)?.path ?? "";
      const inputs = await invoke<Input[]>("ci_inputs", { repo: f.repo, path, gitRef: asked.ref || null });
      if (!current()) return;
      f.values = Object.fromEntries(inputs.map((i) => [i.name, initial(i)]));
      f.inputs = inputs;
    } catch (e) {
      if (current()) f.error = String(e);
    }
  }

  async function dispatch() {
    const f = form;
    if (!f?.inputs) return;
    [busy, f.error] = ["dispatch", ""];
    try {
      await invoke("ci_dispatch", { repo: f.repo, workflow: f.workflow, gitRef: f.ref.trim() || null, inputs: f.values });
      form = null;
      done = t("ciDispatched");
      // GitHub creates the run a few seconds later.
      setTimeout(refresh, 4000);
    } catch (e) {
      f.error = String(e);
    } finally {
      busy = "";
    }
  }

  const short = (sha: string) => sha.slice(0, 7);
  const missing = $derived(form?.inputs?.some((i) => i.required && !form?.values[i.name]?.trim()) ?? true);
</script>

<div class="ci">
  <ViewHeader title={t("viewCi")} context={currentProject()?.name ?? t("ticketsAll")}>
    <label class="labelled inline">
      <span>{t("ciRepo")}</span>
      <select class="field" bind:value={ci.repo} disabled={!repos.length}>
        {#each repos as r (r.path)}<option value={r.path}>{r.name}</option>{/each}
      </select>
    </label>
    {#snippet actions()}
      <button class="btn sm quiet" onclick={() => showSettings("ci")} title={t("ciNotify")}>
        <SettingsIcon size={14} aria-hidden="true" />{t("settings")}
      </button>
      <button class="btn sm" onclick={openForm} disabled={!ci.repo || !!form} title={t("uxfT3DispatchHint")}>
        <Rocket size={14} aria-hidden="true" />{t("ciDispatch")}
      </button>
      <button class="btn sm" onclick={refresh} disabled={!ci.repo || loading}>
        <RefreshCw size={14} aria-hidden="true" class={loading ? "spin" : ""} />{t("ciRefresh")}
      </button>
    {/snippet}
  </ViewHeader>

  <form class="filters" onsubmit={(e) => (e.preventDefault(), load())}>
    <input class="field" placeholder={t("ciBranch")} aria-label={t("ciBranch")} spellcheck="false" bind:value={ci.branch} onchange={load} disabled={!!ci.pr.trim()} />
    <select class="field" aria-label={t("ciStatus")} bind:value={ci.status} onchange={load}>
      {#each statuses as [value, label] (value)}<option {value}>{t(label)}</option>{/each}
    </select>
    <input class="field pr" placeholder={t("ciPr")} aria-label={t("ciPr")} inputmode="numeric" bind:value={ci.pr} onchange={load} />
    {#if runs}<span class="total muted">{t("ciTotal", { count: total })}</span>{/if}
  </form>

  {#if error || done || form}
    <div class="banners">
      {#if error}
        <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
      {/if}
      {#if done}
        <Banner kind="ok" ondismiss={() => (done = "")}>{done}</Banner>
      {/if}
      {#if form}
        {@const f = form}
        <section class="dispatch" aria-labelledby="dispatch-title">
          <h3 id="dispatch-title">{t("ciDispatchTitle")}</h3>
          <div class="row">
            <label class="labelled grow">
              <span>{t("ciWorkflow")}</span>
              <select class="field" bind:value={f.workflow} onchange={readInputs} disabled={!f.workflows}>
                <option value="" disabled>{f.workflows ? t("ciWorkflowPick") : t("ciLoading")}</option>
                {#each f.workflows ?? [] as w (w.id)}<option value={String(w.id)}>{w.name} · {w.path.split("/").pop()}</option>{/each}
              </select>
            </label>
            <label class="labelled">
              <span>{t("ciRef")}</span>
              <input class="field" placeholder={t("ciRefDefault")} spellcheck="false" bind:value={f.ref} onchange={readInputs} />
            </label>
          </div>
          {#if f.inputs?.length}
            <div class="inputs">
              {#each f.inputs as input (input.name)}
                <label class="labelled" class:check={input.kind === "boolean"}>
                  {#if input.kind === "boolean"}
                    <input type="checkbox" checked={f.values[input.name] === "true"} onchange={(e) => (f.values[input.name] = String(e.currentTarget.checked))} />
                    <span class="mono">{input.name}</span>
                  {:else}
                    <span><span class="mono">{input.name}</span>{input.required ? " *" : ""}</span>
                    {#if input.kind === "choice"}
                      <select class="field" bind:value={f.values[input.name]}>
                        {#each input.options as option (option)}<option value={option}>{option}</option>{/each}
                      </select>
                    {:else}
                      <!-- Text even for a number: the value stays a string, as workflow inputs are. -->
                      <input class="field" inputmode={input.kind === "number" ? "decimal" : undefined} bind:value={f.values[input.name]} required={input.required} />
                    {/if}
                  {/if}
                  {#if input.description}<span class="hint">{input.description}</span>{/if}
                </label>
              {/each}
            </div>
          {:else if f.inputs}
            <p class="muted small">{t("ciNoInputs")}</p>
          {/if}
          {#if f.error}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{f.error}</p>{/if}
          <div class="buttons">
            <button class="btn sm" type="button" onclick={() => (form = null)}>{t("cancel")}</button>
            <button class="btn sm primary" onclick={dispatch} disabled={!f.inputs || missing || !!busy}>
              {#if busy === "dispatch"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("ciDispatchRun")}
            </button>
          </div>
        </section>
      {/if}
    </div>
  {/if}

  {#if !repos.length}
    {@const project = currentProject()}
    <EmptyState icon={Workflow} title={t("ciNoRepo")} text={t("ciNoRepoHelp")}>
      <button class="btn" onclick={() => (project ? editProject(project.id) : newProject())}>
        <Pencil size={14} aria-hidden="true" />{t(project ? "ticketsEditProject" : "newProject")}
      </button>
    </EmptyState>
  {:else if listError}
    <EmptyState>
      <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{listError}</p>
      <button class="btn" onclick={load}><RefreshCw size={14} aria-hidden="true" />{t("ciRetry")}</button>
    </EmptyState>
  {:else if !runs}
    <p class="loading muted" role="status"><LoaderCircle size={16} aria-hidden="true" class="spin" />{t("ciLoading")}</p>
  {:else if !runs.length}
    <EmptyState icon={Workflow} title={t("ciEmpty")} text={t("ciEmptyHelp")}>
      <button class="btn" onclick={openForm}><Rocket size={14} aria-hidden="true" />{t("ciDispatchLong")}</button>
    </EmptyState>
  {:else}
    <div class="split">
      <div class="list">
        <ul aria-label={t("ciRuns")}>
          {#each runs as run (run.id)}
            <li class="row" class:current={run === current}>
              <button class="pick" aria-current={run === current} onclick={() => (selected = run.id)}>
                <CiStatus item={run} compact />
                <span class="what">
                  <span class="name">{run.title}</span>
                  <span class="desc">{run.workflow} #{run.number} · {run.branch ?? "—"} · {ago(Date.parse(run.created) / 1000)}</span>
                </span>
              </button>
            </li>
          {/each}
        </ul>
        {#if runs.length < total}
          <button class="btn sm more" onclick={more} disabled={loading}>{t("ciMore")}</button>
        {/if}
      </div>
      <section class="detail" aria-live="polite">
        {#if current}
          {@const run = detail?.run.id === current.id ? detail.run : current}
          <header>
            <div class="title"><CiStatus item={run} /><span class="mono muted">{run.workflow} #{run.number}</span></div>
            <h2>{run.title}</h2>
          </header>
          <div class="actions">
            {#if run.status === "completed"}
              <button class="btn sm" onclick={() => rerun(run, false)} disabled={!!busy}>
                {#if busy === "rerun"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<RotateCw size={14} aria-hidden="true" />{/if}{t("ciRerun")}
              </button>
              {#if outcome(run) === "failed"}
                <button class="btn sm" onclick={() => rerun(run, true)} disabled={!!busy}>
                  {#if busy === "failed"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<RotateCw size={14} aria-hidden="true" />{/if}{t("ciRerunFailed")}
                </button>
              {/if}
            {:else}
              <button class="btn sm" aria-expanded={confirmCancel} onclick={() => (confirmCancel = true)} disabled={!!busy}>
                {#if busy === "cancel"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Ban size={14} aria-hidden="true" />{/if}{t("ciCancel")}
              </button>
            {/if}
            <button class="btn sm" onclick={() => copy(run.url)}><Copy size={14} aria-hidden="true" />{t("runCopyLink")}</button>
          </div>
          {#if confirmCancel && run.status !== "completed"}
            <div class="confirm" role="alert">
              <span>{t("ciCancelConfirm")}</span>
              <div class="buttons">
                <button class="btn sm" onclick={() => (confirmCancel = false)}>{t("ciCancelKeep")}</button>
                <button class="btn sm danger" onclick={() => cancel(run)} disabled={!!busy}>{t("ciCancel")}</button>
              </div>
            </div>
          {/if}
          <dl class="meta">
            <dt>{t("ciMetaBranch")}</dt>
            <dd><span class="mono">{run.branch ?? "—"}</span>{#if run.pullRequests.length}<span class="muted">{` · ${run.pullRequests.map((n) => `#${n}`).join(", ")}`}</span>{/if}</dd>
            <dt>{t("ciMetaCommit")}</dt>
            <dd class="mono">{short(run.sha)}</dd>
            <dt>{t("ciMetaTrigger")}</dt>
            <dd>{t("ciTriggeredBy", { event: run.event, actor: run.triggeredBy ?? run.actor ?? "—" })}</dd>
            {#if run.attempt > 1}
              <dt>{t("ciMetaAttempt")}</dt>
              <dd>{run.attempt}</dd>
            {/if}
            <dt>{t("ciMetaStarted")}</dt>
            <dd>{ago(Date.parse(run.started ?? run.created) / 1000)}</dd>
            <dt>{t("ciMetaDuration")}</dt>
            <dd>{duration(run.started, run.status === "completed" ? run.updated : null)}</dd>
          </dl>
          <h3>{t("ciJobs")}</h3>
          {#if detailError}
            <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{detailError}</p>
          {:else if !detail}
            <p class="muted small" role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" />{t("ciLoading")}</p>
          {:else}
            <ul class="jobs">
              {#each detail.jobs as job (job.id)}
                {@const open = expanded.includes(job.id)}
                <li>
                  <div class="job">
                    <button class="icon-btn sm" aria-expanded={open} onclick={() => toggle(job.id)} aria-label={t("ciSteps", { job: job.name })} title={t("ciSteps", { job: job.name })}>
                      {#if open}<ChevronDown size={14} aria-hidden="true" />{:else}<ChevronRight size={14} aria-hidden="true" />{/if}
                    </button>
                    <CiStatus item={job} compact />
                    <span class="job-name">{job.name}</span>
                    <span class="muted small-time">{duration(job.started, job.completed)}</span>
                    <button class="btn sm" aria-pressed={logOf === job.id} onclick={() => (logOf = logOf === job.id ? null : job.id)} disabled={job.status !== "completed"} title={job.status !== "completed" ? t("ciLogLater") : undefined}>
                      <FileText size={13} aria-hidden="true" />{t("ciLog")}
                    </button>
                  </div>
                  {#if open}
                    <ol class="steps">
                      {#each job.steps as step (step.number)}
                        <li class:failed={outcome(step) === "failed"}>
                          <CiStatus item={step} compact />
                          <span class="step-name">{step.name}</span>
                          <span class="muted small-time">{duration(step.started, step.completed)}</span>
                        </li>
                      {/each}
                    </ol>
                  {/if}
                  {#if logOf === job.id}
                    {#key job.id}<CiLog repo={ci.repo} {job} />{/key}
                  {/if}
                </li>
              {:else}
                <li class="muted small">{t("ciNoJobs")}</li>
              {/each}
            </ul>
          {/if}
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .ci {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  .inline {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }

  .inline select {
    width: 200px;
  }

  .filters .field {
    width: 200px;
    height: 26px;
  }

  .filters select.field {
    width: 170px;
  }

  .filters .pr {
    width: 90px;
  }

  .total {
    margin-left: auto;
    font-size: var(--text-sm);
  }

  .labelled.check {
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
  }

  .check .hint {
    flex-basis: 100%;
  }

  .hint {
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 400;
  }

  .dispatch {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 820px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .dispatch h3 {
    margin: 0;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .row .labelled:not(.grow) {
    width: 220px;
  }

  .grow {
    flex: 1;
    min-width: 260px;
  }

  .inputs {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 10px 12px;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .small {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-sm);
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin: 40px auto;
  }

  .list ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .pick {
    flex-direction: row;
    align-items: flex-start;
    gap: 8px;
  }

  .pick > :global(.icon) {
    margin-top: 2px;
  }

  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .more {
    display: flex;
    margin: 8px auto;
  }

  .detail > :global(*) {
    max-width: 960px;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  h2 {
    margin: 6px 0 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  h3 {
    margin: 0 0 6px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 16px 0 0;
  }

  .confirm {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin-top: 12px;
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    font-size: var(--text-sm);
  }

  .confirm .buttons {
    margin-left: auto;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    align-items: baseline;
    gap: 6px 16px;
    margin: 20px 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .jobs,
  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .jobs > li {
    border-bottom: 1px solid var(--border);
  }

  .job {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
  }

  .job-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .small-time {
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .steps {
    padding: 0 0 8px 36px;
    font-size: var(--text-sm);
  }

  .steps li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }

  .steps li.failed .step-name {
    color: var(--danger);
    font-weight: 600;
  }

  .step-name {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
