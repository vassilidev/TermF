<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Ban from "@lucide/svelte/icons/ban";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import Copy from "@lucide/svelte/icons/copy";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import FileText from "@lucide/svelte/icons/file-text";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Rocket from "@lucide/svelte/icons/rocket";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import WebhookIcon from "@lucide/svelte/icons/webhook";
  import Workflow from "@lucide/svelte/icons/workflow";
  import CiDeploys from "$lib/CiDeploys.svelte";
  import CiGraph from "$lib/CiGraph.svelte";
  import CiHooks from "$lib/CiHooks.svelte";
  import CiLog from "$lib/CiLog.svelte";
  import CiStatus from "$lib/CiStatus.svelte";
  import CiTools from "$lib/CiTools.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Card from "$lib/ui/Card.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import List from "$lib/ui/List.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { ago, t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import { currentProject, scopeRepos } from "$lib/files.svelte";
  import { newProject } from "$lib/newproject.svelte";
  import { editProject } from "$lib/project.svelte";
  import { showSettings } from "$lib/settings.svelte";
  import { onScreen } from "$lib/state.svelte";
  import { openUrl } from "$lib/tickets.svelte";
  import {
    ci,
    deployLabel,
    deployMark,
    deploys,
    duration,
    loadDeploys,
    loadHooks,
    NO_ADMIN,
    outcome,
    type Input,
    type Job,
    type JobLog,
    type Node,
    type RunDetail,
    type RunPage,
    type Workflow as Flow,
    type WorkflowRun,
  } from "$lib/ci.svelte";
  import { branches, columns, excerpt, firstFailure, history } from "$lib/ci-logic";
  import { askAI, provide, type Chip } from "$lib/ask.svelte";
  import { ciRunChip } from "$lib/ask-logic";

  const repos = $derived(scopeRepos());
  /** The project the repository is in: where its deploy targets are. */
  const project = $derived(app.project ?? app.workspace?.projects.find((p) => p.repos.some((r) => r.path === ci.repo))?.id ?? null);

  /** Every branch's latest runs (null while the first page loads), and the selected branch's own. */
  let overview = $state<WorkflowRun[] | null>(null);
  let total = $state(0);
  let pages = $state(1);
  let slug = $state("");
  let listError = $state("");
  let loading = $state(false);
  let branchRuns = $state<WorkflowRun[] | null>(null);
  let selected = $state<number | null>(null);
  let detail = $state<RunDetail | null>(null);
  let detailError = $state("");
  /** By run: the jobs of its workflow file, or null when the file could not be read (the jobs then stand alone). */
  let graphs = $state<Record<number, Node[] | null>>({});
  /** The job whose steps show under the graph, and the job whose whole log does. */
  let job = $state<number | null>(null);
  let logOf = $state<number | null>(null);
  /** By job: its log, read once it failed. */
  let logs = $state<Record<number, JobLog | { error: string }>>({});
  /** Null until known: whether the repository has a workflow at all, asked when it has no run. */
  let workflowsKnown = $state<Flow[] | null>(null);
  let filter = $state("");
  let busy = $state<"" | "rerun" | "failed" | "cancel" | "dispatch">("");
  let confirmCancel = $state(false);
  let form = $state<{
    repo: string;
    workflows: Flow[] | null;
    workflow: string;
    ref: string;
    inputs: Input[] | null;
    values: Record<string, string>;
    error: string;
  } | null>(null);

  const branch = $derived(ci.branch ?? "");
  const groups = $derived(overview ? branches(overview) : []);
  const shownGroups = $derived.by(() => {
    const q = filter.trim().toLowerCase().replace(/^#/, "");
    const list = q ? groups.filter((g) => g.branch.toLowerCase().includes(q) || g.prs.some((n) => String(n) === q)) : groups;
    // The branch shown stays in the list, even past the first page.
    return list.some((g) => g.branch === branch) || ci.branch === null || !branchRuns?.length || q ? list : [{ branch, prs: [...new Set(branchRuns.flatMap((r) => r.pullRequests))], runs: branchRuns }, ...list];
  });
  const current = $derived(branchRuns?.find((r) => r.id === selected) ?? branchRuns?.[0]);
  const run = $derived(current && detail?.run.id === current.id ? detail.run : current);
  const strip = $derived(history(branchRuns ?? [], 20));
  const cols = $derived(detail && current && detail.run.id === current.id ? columns(graphs[current.id] ?? [], detail.jobs) : null);
  const failure = $derived(cols ? firstFailure(cols) : null);
  const failedLog = $derived(failure ? logs[failure.job.id] : undefined);
  const lines = $derived(failedLog && "lines" in failedLog ? excerpt(failedLog) : null);
  const picked = $derived(detail?.jobs.find((j) => j.id === job));

  const lineNo = (i: number) => (failedLog && "lines" in failedLog ? failedLog.skipped : 0) + (lines?.start ?? 0) + i + 1;

  /** The failing line and what led to it, as the agent reads it. */
  const errorChip = $derived.by((): Chip | null => {
    if (!failure || !lines) return null;
    return {
      id: `ci-error:${failure.job.id}`,
      kind: "object",
      label: t("cvErrorChip", { job: failure.job.name, line: lineNo(lines.at) }),
      text: [`${failure.job.name} › ${failure.step ?? ""}, line ${lineNo(lines.at)}:`, ...lines.lines].join("\n"),
    };
  });

  // The ⌘I panel: devops answers, about the run shown and, once read, its failing line.
  $effect(() => provide("ci", { repo: ci.repo, chips: run ? [ciRunChip(run), ...(errorChip ? [errorChip] : [])] : [] }));

  function why() {
    if (!run) return;
    void askAI({
      role: "devops",
      chips: [ciRunChip(run), ...(errorChip ? [errorChip] : [])],
      draft: t("aigNextCiWhy", { name: run.workflow, branch: run.branch ?? "" }),
    });
  }

  const setupCi = () => askAI({ role: "devops", draft: t("cvSetupDraft") });

  // A repository out of the current project falls back to the project's first one.
  $effect(() => {
    if (!repos.some((r) => r.path === ci.repo)) ci.repo = repos[0]?.path ?? "";
  });

  $effect(() => {
    void ci.repo;
    untrack(() => {
      if (form && form.repo !== ci.repo) form = null;
      ci.branch = null;
      load();
      if (ci.repo) void loadHooks(ci.repo);
    });
  });

  $effect(() => {
    const p = project;
    if (p && deploys.targetsOf !== p) untrack(() => loadDeploys(p));
  });

  $effect(() => {
    const p = project;
    if (p && ci.tab === "deploys" && !deploys.tokensRead) untrack(() => loadDeploys(p, true));
  });

  // A project that deploys with a tool opens on its deployments, once per project.
  let landed = "";
  $effect(() => {
    const p = project;
    const targets = deploys.targets;
    if (!p || deploys.targetsOf !== p || targets === null || landed === p) return;
    landed = p;
    if (targets.length && ci.tab === "runs") ci.tab = "deploys";
  });

  $effect(() => {
    void ci.branch;
    untrack(() => loadBranch());
  });

  // Read again when the run shown changes, not each time its object is replaced by the same run.
  const currentKey = $derived(current ? `${current.id}:${current.updated}:${current.status}` : "");
  $effect(() => {
    void currentKey;
    untrack(() => loadDetail(current));
  });

  // The first failed job's log, once per job: the line at fault.
  $effect(() => {
    const f = failure?.job;
    if (f && f.status === "completed" && !(f.id in logs)) untrack(() => readLog(f));
  });

  // While the view is on screen: a look every 30 s, cheap since GitHub answers "unchanged" (ETag); every 5 s while
  // the run shown is at work, so its graph moves live.
  $effect(() => {
    if (!onScreen("ci")) return;
    const timer = setInterval(() => document.visibilityState === "visible" && refresh(), 30_000);
    return () => clearInterval(timer);
  });

  const working = $derived(!!run && run.status !== "completed");
  $effect(() => {
    if (!onScreen("ci") || ci.tab !== "runs" || !working) return;
    const timer = setInterval(() => document.visibilityState === "visible" && untrack(() => loadDetail(current, true)), 5_000);
    return () => clearInterval(timer);
  });

  $effect(() => {
    if (ci.dispatch) untrack(openForm);
  });

  let request = 0;

  /** Pages 1 to `count` of every branch's runs; only the latest request fills the list. */
  async function fetchPages(count: number, mode: "load" | "refresh" | "more") {
    if (!ci.repo) return;
    const r = ++request;
    loading = true;
    if (mode === "load") listError = "";
    try {
      const found: WorkflowRun[] = [];
      let page: RunPage | null = null;
      for (let n = 1; n <= count; n++) {
        page = await invoke<RunPage>("ci_runs", { repo: ci.repo, page: n });
        found.push(...page.runs.filter((run) => !found.some((f) => f.id === run.id)));
      }
      if (r !== request) return;
      [overview, total, pages, listError, slug] = [found, page?.total ?? 0, count, "", page?.repo ?? slug];
      // Picked once: a refresh never moves the view to the branch that ran last.
      if (ci.branch === null && found.length) ci.branch = found[0].branch ?? "";
      if (!found.length && workflowsKnown === null) {
        invoke<Flow[]>("ci_workflows", { repo: ci.repo }).then(
          (w) => r === request && (workflowsKnown = w),
          () => {},
        );
      }
    } catch (e) {
      if (r === request && mode === "load") listError = String(e);
      if (r === request && mode === "more") toast(String(e), { kind: "error" });
    } finally {
      if (r === request) loading = false;
    }
  }

  function load() {
    [overview, branchRuns, selected, workflowsKnown, graphs, logs, slug] = [null, null, null, null, {}, {}, ""];
    return fetchPages(1, "load");
  }

  const refresh = () => {
    void loadBranch(true);
    if (project && ci.tab === "deploys") void loadDeploys(project, true);
    if (ci.tab === "hooks") void loadHooks(ci.repo);
    return fetchPages(pages, overview === null ? "load" : "refresh");
  };
  const more = () => fetchPages(pages + 1, "more");

  let branchRequest = 0;

  async function loadBranch(quiet = false) {
    const r = ++branchRequest;
    if (!quiet) [branchRuns, selected, job, logOf] = [null, null, null, null];
    if (!ci.repo || ci.branch === null) return;
    // Runs without a branch (a tag, a deleted fork's) are those of the overview: GitHub filters by name only.
    if (ci.branch === "") {
      if (r === branchRequest) branchRuns = (overview ?? []).filter((x) => !x.branch);
      return;
    }
    try {
      const page = await invoke<RunPage>("ci_runs", { repo: ci.repo, branch });
      if (r === branchRequest) branchRuns = page.runs;
    } catch (e) {
      if (r === branchRequest && !quiet) [branchRuns, listError] = [[], String(e)];
    }
  }

  let detailRequest = 0;

  async function loadDetail(r: WorkflowRun | undefined, quiet = false) {
    const n = ++detailRequest;
    if (detail?.run.id !== r?.id) [detail, detailError, job, logOf, confirmCancel] = [null, "", null, null, false];
    if (!r) return;
    try {
      const [found] = await Promise.all([invoke<RunDetail>("ci_run", { repo: ci.repo, id: r.id }), readGraph(r)]);
      if (n !== detailRequest) return;
      detail = found;
      // The squares follow the run as it moves.
      const moved = (b: WorkflowRun) => b.id === found.run.id && (b.status !== found.run.status || b.updated !== found.run.updated);
      if (branchRuns?.some(moved)) branchRuns = branchRuns.map((b) => (moved(b) ? found.run : b));
    } catch (e) {
      if (n === detailRequest && !quiet) detailError = String(e);
    }
  }

  async function readGraph(r: WorkflowRun) {
    if (r.id in graphs) return;
    if (!r.path.startsWith(".github/workflows/")) {
      graphs[r.id] = null;
      return;
    }
    try {
      graphs[r.id] = await invoke<Node[]>("ci_graph", { repo: ci.repo, path: r.path, sha: r.sha });
    } catch {
      // Without the file the jobs still show, side by side.
      graphs[r.id] = null;
    }
  }

  async function readLog(j: Job) {
    try {
      logs[j.id] = await invoke<JobLog>("ci_log", { repo: ci.repo, job: j.id });
    } catch (e) {
      logs[j.id] = { error: String(e) };
    }
  }

  async function act(kind: typeof busy, run: () => Promise<unknown>, message: string) {
    busy = kind;
    try {
      await run();
      toast(message);
      await refresh();
    } catch (e) {
      toast(String(e), { kind: "error" });
    } finally {
      busy = "";
    }
  }

  const rerun = (r: WorkflowRun, failed: boolean) =>
    act(failed ? "failed" : "rerun", () => invoke("ci_rerun", { repo: ci.repo, id: r.id, failed }), t("ciRerunDone"));

  const cancel = (r: WorkflowRun) => act("cancel", () => invoke("ci_cancel", { repo: ci.repo, id: r.id }), t("ciCancelDone"));

  async function copy(url: string) {
    try {
      await navigator.clipboard.writeText(url);
      toast(t("runLinkCopied"));
    } catch (e) {
      toast(String(e), { kind: "error" });
    }
  }

  async function openForm() {
    ci.dispatch = false;
    if (!ci.repo) return;
    ci.tab = "runs";
    form = { repo: ci.repo, workflows: null, workflow: "", ref: branch, inputs: null, values: {}, error: "" };
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
    const still = () => form === f && f.workflow === asked.workflow && f.ref.trim() === asked.ref;
    [f.inputs, f.error] = [null, ""];
    try {
      const path = f.workflows?.find((w) => String(w.id) === asked.workflow)?.path ?? "";
      const inputs = await invoke<Input[]>("ci_inputs", { repo: f.repo, path, gitRef: asked.ref || null });
      if (!still()) return;
      f.values = Object.fromEntries(inputs.map((i) => [i.name, initial(i)]));
      f.inputs = inputs;
    } catch (e) {
      if (still()) f.error = String(e);
    }
  }

  async function dispatch() {
    const f = form;
    if (!f?.inputs) return;
    [busy, f.error] = ["dispatch", ""];
    try {
      await invoke("ci_dispatch", { repo: f.repo, workflow: f.workflow, gitRef: f.ref.trim() || null, inputs: f.values });
      form = null;
      toast(t("ciDispatched"));
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
  const outcomeLabel = { running: "ciRunning", queued: "ciQueued", passed: "ciPassed", failed: "ciFailed", stopped: "ciStopped" } as const;
  const squareLabel = (r: WorkflowRun) => `${r.workflow} #${r.number} · ${t(outcomeLabel[outcome(r)])} · ${ago(Date.parse(r.created) / 1000)}`;

  const deployItems = $derived(
    (deploys.targets ?? [])
      .flatMap((target) => {
        const found = deploys.found[target.name];
        return Array.isArray(found) ? found.slice(0, 2).map((d) => ({ target, d })) : [];
      })
      .sort((a, b) => Date.parse(b.d.created ?? "") - Date.parse(a.d.created ?? "") || 0)
      .slice(0, 3),
  );
  const deployCount = $derived(
    (deploys.targets ?? []).reduce((n, target) => {
      const found = deploys.found[target.name];
      return n + (Array.isArray(found) ? found.length : 0);
    }, 0),
  );
  const tabs = $derived([
    { id: "runs", label: t("ciRuns") },
    { id: "deploys", label: t("cvDeploys"), count: deploys.targets?.length && (deploys.tokensRead || !deploys.targets.some((x) => x.token)) ? deployCount : null },
    { id: "hooks", label: t("cvHooks"), count: deploys.hooksOf === ci.repo && !deploys.hooksError ? (deploys.hooks?.length ?? null) : null },
    { id: "tools", label: t("cvTools"), count: deploys.targets?.length || null },
  ]);
  const hookHost = (url: string) => url.replace(/^https?:\/\//, "").split("?")[0];
  const deployTone = (state: string) => (state === "passed" ? "done" : state === "failed" ? "failed" : state === "running" || state === "queued" ? "working" : "");
</script>

{#snippet dispatchForm()}
  {#if form}
    {@const f = form}
    <section class="dispatch" aria-labelledby="dispatch-title">
      <h3 id="dispatch-title">{t("ciDispatchTitle")}</h3>
      {#if f.workflows && !f.workflows.length}
        <p class="muted">{t("cvNoDispatchable")}</p>
        <div class="buttons">
          <Button size="sm" variant="quiet" onclick={() => (form = null)}>{t("cancel")}</Button>
          <Button size="sm" variant="primary" icon={Sparkles} onclick={setupCi}>{t("cvSetupCi")}</Button>
        </div>
      {:else}
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
        {#if f.error.includes("workflow_dispatch")}
          <p class="notice warn"><CircleAlert size={14} aria-hidden="true" />{t("cvNotByHand")}</p>
        {:else if f.error}
          <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{f.error}</p>
        {/if}
        <div class="buttons">
          <button class="btn sm" type="button" onclick={() => (form = null)}>{t("cancel")}</button>
          <button class="btn sm primary" onclick={dispatch} disabled={!f.inputs || missing || !!busy}>
            {#if busy === "dispatch"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("ciDispatchRun")}
          </button>
        </div>
      {/if}
    </section>
  {/if}
{/snippet}

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

  {#if !repos.length}
    {@const p = currentProject()}
    <EmptyState icon={Workflow} title={t("ciNoRepo")} text={t("ciNoRepoHelp")}>
      <button class="btn" onclick={() => (p ? editProject(p.id) : newProject())}>
        <Pencil size={14} aria-hidden="true" />{t(p ? "ticketsEditProject" : "newProject")}
      </button>
    </EmptyState>
  {:else}
    <Tabs id="ci" label={t("viewCi")} {tabs} selected={ci.tab} onselect={(id) => (ci.tab = id as typeof ci.tab)}>
      {#if ci.tab === "deploys"}
        {#if project}<CiDeploys {project} repo={ci.repo} {slug} runs={overview ?? []} onsetup={() => (ci.tab = "tools")} />{/if}
      {:else if ci.tab === "hooks"}
        <CiHooks repo={ci.repo} {slug} />
      {:else if ci.tab === "tools"}
        {#if project}<CiTools {project} repos={app.workspace?.projects.find((p) => p.id === project)?.repos ?? []} />{/if}
      {:else if listError}
        <EmptyState>
          <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{listError}</p>
          <button class="btn" onclick={load}><RefreshCw size={14} aria-hidden="true" />{t("ciRetry")}</button>
        </EmptyState>
      {:else if !overview}
        <div class="split">
          <div class="list"><Skeleton shape="list" rows={8} label={t("ciLoading")} /></div>
          <div class="detail"><Skeleton shape="lines" rows={3} label={t("ciLoading")} /><Skeleton shape="block" height="180px" /></div>
        </div>
      {:else if !overview.length}
        {#if form}<div class="pad">{@render dispatchForm()}</div>{/if}
        {#if workflowsKnown && !workflowsKnown.length}
          <EmptyState icon={Workflow} title={t("cvNoWorkflow")} text={t("cvNoWorkflowText")} action={{ label: t("cvSetupCi"), icon: Sparkles, run: setupCi }}>
            <p class="deploy-hint">{t("actCiDeployHint")} <button class="lnk" onclick={() => (ci.tab = "tools")}>{t("actCiConnectTool")}</button></p>
          </EmptyState>
        {:else if workflowsKnown}
          <EmptyState icon={Workflow} title={t("ciEmpty")} text={t("cvNoRunText")} action={{ label: t("ciDispatchLong"), icon: Rocket, run: openForm }} onask={setupCi} askLabel={t("cvAskDevops")} />
        {:else}
          <div class="pad"><Skeleton shape="lines" rows={2} label={t("ciLoading")} /></div>
        {/if}
      {:else}
        <div class="split">
          <div class="list">
            <input class="field filter" placeholder={t("cvFilter")} aria-label={t("cvFilter")} spellcheck="false" bind:value={filter} />
            <List label={t("cvBranches")} items={shownGroups} key={(g) => g.branch} selected={branch} onselect={(g) => { ci.branch = g.branch; }}>
              {#snippet row(g)}
                {@const last = g.runs[0]}
                <div class="branch">
                  <div class="b-top">
                    <span class="b-name">{g.branch || "—"}</span>
                    {#each g.prs.slice(0, 2) as n (n)}<span class="pr">#{n}</span>{/each}
                  </div>
                  <div class="b-sub">
                    <span class="mini" aria-hidden="true">
                      {#each history(g.runs, 10).squares as r (r.id)}<i class={outcome(r)}></i>{/each}
                    </span>
                    <span class="muted">{last ? ago(Date.parse(last.created) / 1000) : ""}</span>
                  </div>
                </div>
              {/snippet}
            </List>
            {#if overview.length < total}
              <button class="btn sm more" onclick={more} disabled={loading}>{t("ciMore")}</button>
            {/if}
          </div>

          <section class="detail" aria-live="polite">
            {@render dispatchForm()}
            {#if !branchRuns}
              <Skeleton shape="lines" rows={2} label={t("ciLoading")} />
              <Skeleton shape="block" height="180px" />
            {:else if run}
              <div class="hist">
                <span>{t("cvLastRuns", { branch })}</span>
                <span class="sq">
                  {#each strip.squares as r (r.id)}
                    <button class="s {outcome(r)}" class:on={r.id === run.id} aria-label={squareLabel(r)} title={squareLabel(r)} aria-pressed={r.id === run.id} onclick={() => (selected = r.id)}></button>
                  {/each}
                </span>
                {#if strip.failing >= 2}<span class="streak">{t("cvFailingStreak", { count: strip.failing })}</span>{/if}
              </div>

              <header class="runhd">
                <div class="grow">
                  <h2>{run.title}</h2>
                  <p class="muted">
                    {t("cvRunMeta", { workflow: run.workflow, number: run.number })}{#if run.pullRequests.length}{" · "}{t("cvPrBy", { n: run.pullRequests[0], actor: run.actor ?? "—" })}{:else}{" · "}{t("ciTriggeredBy", { event: run.event, actor: run.triggeredBy ?? run.actor ?? "—" })}{/if}
                    · <span class="mono">{short(run.sha)}</span> · {ago(Date.parse(run.started ?? run.created) / 1000)}{#if run.attempt > 1}{" · "}{t("cvAttempt", { n: run.attempt })}{/if}
                  </p>
                </div>
                <span class="state"><CiStatus item={run} />{#if run.started}<span class="muted num">{duration(run.started, run.status === "completed" ? run.updated : null)}</span>{/if}</span>
                {#if run.status !== "completed"}
                  <Button size="sm" icon={Ban} loading={busy === "cancel"} disabled={!!busy} onclick={() => (confirmCancel = true)}>{t("ciCancel")}</Button>
                {:else if outcome(run) === "failed"}
                  <Button size="sm" icon={RotateCw} loading={busy === "failed"} disabled={!!busy} onclick={() => rerun(run, true)}>{t("cvRerunFailed")}</Button>
                  <Button size="sm" variant="quiet" loading={busy === "rerun"} disabled={!!busy} onclick={() => rerun(run, false)}>{t("cvRerunAll")}</Button>
                {:else}
                  <Button size="sm" icon={RotateCw} loading={busy === "rerun"} disabled={!!busy} onclick={() => rerun(run, false)}>{t("ciRerun")}</Button>
                {/if}
                <button class="icon-btn" aria-label={t("runCopyLink")} title={t("runCopyLink")} onclick={() => copy(run.url)}><Copy size={14} aria-hidden="true" /></button>
                <button class="icon-btn" aria-label={t("cvOnGithub")} title={t("cvOnGithub")} onclick={() => openUrl(run.url)}><ExternalLink size={14} aria-hidden="true" /></button>
              </header>

              {#if detailError}
                <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{detailError}</p>
              {:else if !cols}
                <Skeleton shape="block" height="150px" label={t("ciLoading")} />
              {:else if !cols.length}
                <p class="muted small">{t("ciNoJobs")}</p>
              {:else}
                <div class="graph-hd">
                  <h3>{t("ciJobs")}</h3>
                  {#if run.status !== "completed"}<span class="live"><span class="pulse" aria-hidden="true"></span>{t("cvLive")}</span>{/if}
                  {#if graphs[run.id] === null}<span class="muted small">{t("cvNoGraph")}</span>{/if}
                </div>
                <CiGraph {cols} selected={job} onselect={(j) => (job = job === j.id ? null : j.id)} />
              {/if}

              {#if picked}
                <div class="steps-box">
                  <div class="steps-hd">
                    <CiStatus item={picked} compact />
                    <b>{picked.name}</b>
                    <span class="muted num">{duration(picked.started, picked.completed)}</span>
                    <span class="grow"></span>
                    <Button size="sm" variant="quiet" icon={FileText} disabled={picked.status !== "completed"} title={picked.status !== "completed" ? t("ciLogLater") : undefined} onclick={() => (logOf = logOf === picked.id ? null : picked.id)}>{t("ciLog")}</Button>
                  </div>
                  <ol class="steps">
                    {#each picked.steps as step (step.number)}
                      <li class:failed={outcome(step) === "failed"}>
                        <CiStatus item={step} compact />
                        <span class="step-name">{step.name}</span>
                        <span class="muted num">{duration(step.started, step.completed)}</span>
                      </li>
                    {:else}
                      <li class="muted">{t("cvNoSteps")}</li>
                    {/each}
                  </ol>
                  {#if logOf === picked.id}{#key picked.id}<CiLog repo={ci.repo} job={picked} />{/key}{/if}
                </div>
              {/if}

              {#if failure}
                <section class="first" aria-labelledby="first-title">
                  <div class="first-hd">
                    <CircleX size={15} aria-hidden="true" class="bad" />
                    <b id="first-title">{t("cvFirstError")}</b>
                    <span class="muted where">{failure.job.name}{failure.step ? ` › ${failure.step}` : ""}{lines ? ` · ${t("cvLine", { n: lineNo(lines.at) })}` : ""}</span>
                    <span class="grow"></span>
                    <Button size="sm" variant="quiet" icon={FileText} onclick={() => (logOf = logOf === failure.job.id ? null : failure.job.id)}>{t("cvFullLog")}</Button>
                    <Button size="sm" variant="primary" icon={Sparkles} onclick={why}>{t("aigAskWhy")}</Button>
                  </div>
                  {#if failedLog && "error" in failedLog}
                    <div class="log-error">
                      <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{failedLog.error}</p>
                      <Button size="sm" icon={RefreshCw} onclick={() => delete logs[failure.job.id]}>{t("ciRetry")}</Button>
                    </div>
                  {:else if !failedLog}
                    <div class="pad-s"><Skeleton shape="lines" rows={4} label={t("ciLogLoading")} /></div>
                  {:else if lines}
                    <ol class="excerpt" aria-label={t("cvFirstError")}>
                      {#each lines.lines as line, i (i)}
                        <li class:hl={i === lines.at}><span class="n num" aria-hidden="true">{lineNo(i)}</span><span class="l">{line}</span></li>
                      {/each}
                    </ol>
                  {:else}
                    <p class="muted pad-s">{t("cvNoErrorLine")}</p>
                  {/if}
                  {#if logOf === failure.job.id && job !== failure.job.id}{#key failure.job.id}<CiLog repo={ci.repo} job={failure.job} />{/key}{/if}
                </section>
              {/if}

              <div class="two">
                <Card title={t("cvDeploys")}>
                  {#snippet actions()}<button class="lnk" onclick={() => (ci.tab = "deploys")}>{t("cvAll")}</button>{/snippet}
                  {#if deploys.targets === null}
                    <Skeleton shape="list" rows={2} />
                  {:else if !deploys.targets.length}
                    <p class="muted small">{t("cvNoTargetsShort")} <button class="lnk" onclick={() => (ci.tab = "tools")}>{t("cvConnectTool")}</button></p>
                  {:else if !deployItems.length}
                    <p class="muted small">
                      {#if deploys.targets.some((x) => !(x.name in deploys.found))}<button class="lnk" onclick={() => (ci.tab = "deploys")}>{t("cvSeeDeploys")}</button>{:else}{t("cvNoDeploys")}{/if}
                    </p>
                  {:else}
                    {#each deployItems as { target, d } (`${target.name}:${d.id}`)}
                      <button class="mini-row" onclick={() => (ci.tab = "deploys")}>
                        <span class="logo" aria-hidden="true">{deployMark[target.tool]}</span>
                        <span class="mt"><span class="t">{target.name} · {deployLabel[target.tool]}</span><span class="muted">{[d.description ?? d.branch, d.created ? ago(Date.parse(d.created) / 1000) : null].filter(Boolean).join(" · ")}</span></span>
                        <span class="dot {deployTone(d.state)}" aria-hidden="true"></span>
                      </button>
                    {/each}
                  {/if}
                </Card>
                <Card title={t("cvHooks")}>
                  {#snippet actions()}<button class="lnk" onclick={() => (ci.tab = "hooks")}>{t("cvManage")}</button>{/snippet}
                  {#if deploys.hooksOf !== ci.repo || deploys.hooks === null}
                    <Skeleton shape="list" rows={2} />
                  {:else if deploys.hooksError.startsWith(NO_ADMIN)}
                    <p class="muted small">{t("cvNoAdminShort")}</p>
                  {:else if deploys.hooksError}
                    <p class="muted small">{deploys.hooksError}</p>
                  {:else if !deploys.hooks.length}
                    <p class="muted small">{t("cvNoHooks")}</p>
                  {:else}
                    {#each deploys.hooks.slice(0, 3) as h (h.id)}
                      <button class="mini-row" onclick={() => (ci.tab = "hooks")}>
                        <span class="logo" aria-hidden="true"><WebhookIcon size={12} /></span>
                        <span class="mt"><span class="t">{hookHost(h.url)}</span><span class="muted">{h.events.join(", ")}{h.lastCode !== null ? ` · ${h.lastCode}` : ""}</span></span>
                        <span class="dot {h.lastCode === null ? '' : h.lastCode < 300 ? 'done' : 'failed'}" aria-hidden="true"></span>
                      </button>
                    {/each}
                  {/if}
                </Card>
              </div>

              <ConfirmDialog bind:open={confirmCancel} title={t("cvCancelTitle")} confirm={t("ciCancel")} cancel={t("ciCancelKeep")} danger onconfirm={() => run && cancel(run)}>
                <p>{t("ciCancelConfirm")}</p>
              </ConfirmDialog>
            {:else}
              <p class="muted">{t("cvNoBranchRuns")}</p>
            {/if}
          </section>
        </div>
      {/if}
    </Tabs>
  {/if}
</div>

<style>
  .ci {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .pad {
    padding: 16px 24px 0;
  }

  .deploy-hint {
    margin: 4px 0 0;
  }

  .pad-s {
    margin: 0;
    padding: 12px 14px;
  }

  .log-error {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    padding: 12px 14px;
  }

  .log-error p {
    margin: 0;
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

  .split {
    flex: 1;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .filter {
    flex-shrink: 0;
    height: var(--h-sm);
    margin: 0 4px 4px;
  }

  .branch {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .b-top {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .b-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pr {
    flex-shrink: 0;
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .b-sub {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: var(--text-xs);
  }

  .mini {
    display: flex;
    gap: 2px;
  }

  .mini i,
  .s {
    display: block;
    border-radius: 2px;
    background: var(--border-strong);
  }

  .mini i {
    width: 5px;
    height: 10px;
  }

  .passed {
    --sq: var(--done);
  }

  .failed {
    --sq: var(--danger);
  }

  .running,
  .queued {
    --sq: var(--working);
  }

  .mini i.passed,
  .mini i.failed,
  .mini i.running,
  .mini i.queued,
  .s.passed,
  .s.failed,
  .s.running,
  .s.queued {
    background: var(--sq);
  }

  .more {
    align-self: center;
    margin: 8px auto;
  }

  .detail {
    padding: 18px 24px 80px;
    overflow-y: auto;
  }

  .hist {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .sq {
    display: flex;
    gap: 3px;
  }

  .s {
    width: 9px;
    height: 18px;
    padding: 0;
    border: 0;
    cursor: pointer;
    opacity: 0.85;
    transition:
      opacity var(--dur-fast) var(--ease-out),
      transform var(--dur-press) var(--ease-out);
  }

  .s:hover {
    opacity: 1;
  }

  .s:active {
    transform: scale(0.92);
  }

  .s.on {
    opacity: 1;
    outline: 2px solid var(--text);
    outline-offset: 1px;
  }

  .s.running {
    animation: breathe 1.6s var(--ease-in-out) infinite;
  }

  @keyframes breathe {
    50% {
      opacity: 0.45;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .s.running,
    .pulse {
      animation: none;
    }
  }

  .streak {
    color: var(--danger);
    font-weight: 600;
  }

  .runhd {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 8px 10px;
    margin: 16px 0 18px;
  }

  .grow {
    flex: 1;
    min-width: 0;
  }

  .runhd .grow {
    min-width: 280px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  .runhd p {
    margin: 4px 0 0;
    font-size: var(--text-sm);
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: var(--h-sm);
    font-size: var(--text-sm);
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .graph-hd {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }

  .live {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--working);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .pulse {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--working);
    animation: breathe 1.6s var(--ease-in-out) infinite;
  }

  .small {
    margin: 0;
    font-size: var(--text-sm);
  }

  .steps-box,
  .first {
    margin-top: 16px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
  }

  .first {
    border-color: var(--danger-line);
  }

  .steps-hd,
  .first-hd {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 8px 10px 8px 14px;
    border-bottom: 1px solid var(--border);
  }

  .first-hd :global(.bad) {
    color: var(--danger);
  }

  .where {
    min-width: 0;
    overflow: hidden;
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .steps {
    margin: 0;
    padding: 6px 14px 10px;
    font-size: var(--text-sm);
    list-style: none;
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

  .excerpt {
    margin: 0;
    padding: 8px 0;
    overflow-x: auto;
    background: var(--well);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    line-height: 1.6;
    list-style: none;
  }

  .excerpt li {
    display: flex;
    gap: 14px;
    padding: 0 14px;
    white-space: pre;
  }

  .excerpt li.hl {
    background: var(--danger-soft);
    color: var(--danger);
    font-weight: 600;
  }

  .n {
    flex-shrink: 0;
    width: 4ch;
    color: var(--muted);
    text-align: right;
    user-select: none;
  }

  .two {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 12px;
    margin-top: 16px;
  }

  .two :global(.body) {
    padding: 4px 8px 8px;
  }

  .two p {
    padding: 8px;
  }

  .mini-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .mini-row:hover {
    background: var(--hover);
  }

  .mt {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    font-size: var(--text-sm);
  }

  .mt .t {
    overflow: hidden;
    color: var(--text);
    font-size: var(--text-base);
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .logo {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--well);
    color: var(--soft);
    font-size: 10px;
    font-weight: 700;
  }

  .dot {
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--border-strong);
  }

  .dot.done {
    background: var(--done);
  }

  .dot.failed {
    background: var(--danger);
  }

  .dot.working {
    background: var(--working);
  }

  .lnk {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .lnk:hover {
    text-decoration: underline;
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
    margin-bottom: 18px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-1);
  }

  .dispatch h3 {
    margin: 0;
  }

  .dispatch p {
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

  .row .grow {
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
</style>
