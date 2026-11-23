<script lang="ts">
  import { approxUsd } from "$lib/usage.svelte";
  import { onMount, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import LibraryBig from "@lucide/svelte/icons/library-big";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import ScanSearch from "@lucide/svelte/icons/scan-search";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Ticket from "@lucide/svelte/icons/ticket";
  import Workflow from "@lucide/svelte/icons/workflow";
  import X from "@lucide/svelte/icons/x";
  import ClaudeTrust from "$lib/ClaudeTrust.svelte";
  import Conventions from "$lib/Conventions.svelte";
  import OnboardingJournal from "$lib/OnboardingJournal.svelte";
  import OnboardingTeam from "$lib/OnboardingTeam.svelte";
  import TicketSources from "$lib/TicketSources.svelte";
  import StackOffers from "$lib/StackOffers.svelte";
  import StackSkills from "$lib/StackSkills.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import QuestionInput from "$lib/ui/QuestionInput.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { lang, t } from "$lib/i18n";
  import { library, showLibrary, type RepoStack } from "$lib/library.svelte";
  import { analyze as runAnalysis, analyzing, closeSetup, reads, readText, running, setup } from "$lib/onboarding.svelte";
  import FileText from "@lucide/svelte/icons/file-text";
  import Search from "@lucide/svelte/icons/search";
  import { spans } from "$lib/markdown-logic";
  import {
    byTheme,
    githubIssues,
    reachable,
    recipeLines,
    steps,
    ticketProposal,
    type Question,
    type RepoRecipe,
    type Step,
    type Theme,
    type Trackers,
  } from "$lib/onboarding-logic";
  import { app, shortPath } from "$lib/state.svelte";

  type Facts = {
    name: string;
    path: string;
    stack: string[];
    workflows: string[];
    instructions: string[];
    claude: string[];
    parts?: { path: string; name: string; role: string }[];
    tests?: string[];
    infra?: string[];
  };
  type Analysis = {
    at: number;
    lang: string;
    repos: Facts[];
    github: string[];
    tickets: string[];
    library: string[];
    recipe: boolean;
    conventions: string[];
    asked: boolean;
    findings: { repo: string; label: string; value: string }[];
    questions: Question[];
    dropped: string[];
    model: string;
    cost: number | null;
  };
  type Decision = "accepted" | "rejected";
  type Onboarding = {
    autoOpen: boolean;
    step: Step;
    analysis: Analysis | null;
    answers: Record<string, string[]>;
    decisions: Record<string, Decision>;
    log: { at: number; what: string; decision?: Decision; detail: string }[];
    edited: boolean;
    analyzing: boolean;
  };
  type Recipe = { agent: boolean; notes: string; repos: Record<string, RepoRecipe> };
  type RecipeView = {
    recipe: Recipe;
    saved: boolean;
    detected: Recipe;
    draft: { recipe: Recipe; cost: number | null; dropped: string[] } | null;
  };
  type Sources = { trackers: Trackers; githubRepos: string[] };
  type Notify = "mine" | "all" | "off";

  // Mounted per project (keyed on `setup.project`).
  const project = app.workspace?.projects.find((p) => p.id === setup.project);

  let ob = $state<Onboarding | null>(null);
  let shown = $state<Step>("analysis");
  /** The call in progress, or "". */
  let busy = $state("");
  let error = $state("");
  let answers = $state<Record<string, string[]>>({});
  let sources = $state<Sources | null>(null);
  let editingSources = $state(false);
  let sourcesEditor = $state<TicketSources>();
  let recipe = $state<RecipeView | null>(null);
  let notify = $state<Notify>("mine");
  let stacks = $state<RepoStack[]>([]);

  const analysis = $derived(ob?.analysis ?? null);
  const inProgress = $derived(!!project && (!!analyzing[project.id] || !!ob?.analyzing));
  const proposedRecipe = $derived(recipe?.draft?.recipe ?? recipe?.detected ?? null);
  const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD" });

  const stepLabel = {
    analysis: "obStepAnalysis",
    questions: "obStepQuestions",
    proposals: "obStepProposals",
    trust: "obStepTrust",
    done: "obStepDone",
  } as const;
  const themeLabel: Record<Theme, Parameters<typeof t>[0]> = {
    quality: "obThemeQuality",
    tests: "obThemeTests",
    workflow: "obThemeWorkflow",
    agents: "obThemeAgents",
    other: "obThemeOther",
  };
  const notifyLabel = { mine: "ciNotifyMine", all: "ciNotifyAll", off: "ciNotifyOff" } as const;
  const notifyOptions: Notify[] = ["mine", "all", "off"];

  onMount(async () => {
    if (!project) return closeSetup();
    try {
      ob = await invoke<Onboarding>("onboarding_state", { project: project.id });
      shown = setup.at ?? ob.step;
      setup.at = null;
      answers = { ...ob.answers };
    } catch (e) {
      error = String(e);
    }
    // Started before the assistant was left: follow it instead of starting another.
    // Moves on to its questions only if the owner still waits on it; one who went on stays on that step.
    running(project.id)?.then(
      () => reload(shown === "analysis"),
      (e) => ((error = String(e)), reload(false)),
    );
  });

  async function reload(done: boolean) {
    ob = await invoke<Onboarding>("onboarding_state", { project: project!.id }).catch(() => ob);
    if (!ob) return;
    answers = { ...ob.answers };
    if (done) shown = ob.step;
  }

  $effect(() => {
    if (shown === "proposals") untrack(loadProposals);
  });

  // Read again when the library changes, the trust given at the next step included.
  $effect(() => {
    void library.version;
    if (shown !== "proposals" || !project) return;
    invoke<RepoStack[]>("library_stack", { project: project.id })
      .then((list) => (stacks = list))
      .catch((e) => (error = String(e)));
  });

  async function call<T>(what: string, command: string, args: Record<string, unknown> = {}): Promise<T | undefined> {
    busy = what;
    error = "";
    try {
      return await invoke<T>(command, { project: project!.id, ...args });
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  async function analyze() {
    error = "";
    try {
      await runAnalysis(project!.id, lang);
      await reload(true);
    } catch (e) {
      error = String(e);
      // A first analysis keeps the survey, a new one the last analysis: the owner may go on.
      await reload(false);
    }
  }

  async function go(step: Step) {
    const next = await call<Onboarding>("step", "onboarding_step", { step });
    if (next) {
      ob = next;
      shown = step;
    }
  }

  async function submitAnswers() {
    const next = await call<Onboarding>("answer", "onboarding_answer", { answers, replace: ob?.edited ?? false });
    if (next) {
      ob = next;
      shown = "proposals";
    }
  }

  async function decide(proposal: string, decision: Decision, detail = "") {
    const next = await call<Onboarding>(proposal, "onboarding_decide", { proposal, decision, detail });
    if (next) ob = next;
  }

  async function loadProposals() {
    const id = project!.id;
    try {
      [sources, recipe, notify] = await Promise.all([
        invoke<Sources>("tickets_sources", { project: id }),
        invoke<RecipeView>("worktree_recipe", { project: id }),
        invoke<Notify>("ci_notify"),
      ]);
    } catch (e) {
      error = String(e);
    }
  }

  async function acceptTickets() {
    if (!sources) return;
    const saved = await call<Sources>("tickets", "tickets_sources_save", { trackers: githubIssues(sources.trackers) });
    if (saved) {
      sources = saved;
      await decide("tickets", "accepted", saved.githubRepos.join(", "));
    }
  }

  async function saveSources() {
    // Unchanged sources write nothing, so nothing is decided.
    const changed = !!sourcesEditor?.dirty();
    busy = "tickets";
    error = "";
    try {
      await sourcesEditor?.save();
      sources = await invoke<Sources>("tickets_sources", { project: project!.id });
      editingSources = false;
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
    if (changed && !error) await decide("tickets", "accepted");
  }

  async function acceptRecipe() {
    const view = await call<RecipeView>("recipe", "worktree_recipe_save", { recipe: proposedRecipe });
    if (view) {
      recipe = view;
      await decide("worktree", "accepted", Object.keys(view.recipe.repos).join(", "));
    }
  }

  async function proposeRecipe() {
    const view = await call<RecipeView>("recipe", "worktree_recipe_propose");
    if (view) recipe = view;
  }

  function editRecipe() {
    Object.assign(library, { global: false, tab: "worktrees", panel: null });
    showLibrary();
  }

  async function acceptCi() {
    await call("ci", "ci_notify_set", { notify });
    if (!error) await decide("ci", "accepted", notify);
  }

  type Key = Parameters<typeof t>[0];
  const teamOptions: Record<string, Key> = {
    one: "obTeamOneSquad",
    split: "obTeamPerPart",
    own: "obTeamQOwn",
    sonnet: "obTeamQThrifty",
    opus: "obTeamQQuality",
  };
  // TermF's own questions come as translation keys and option codes, Claude Code's as text.
  const ours = (q: Question) => q.id.startsWith("team-");
  const qText = (q: Question) => (ours(q) ? t(q.text as Key) : q.text);
  const optionText = (q: Question, option: string) =>
    ours(q) && option in teamOptions ? t(teamOptions[option]) : option;
  const none = (list: string[]) => (list.length ? list.join(", ") : t("obNothing"));
  /** A repository's facts that are there; an empty one says nothing. */
  const rows = (r: Facts) =>
    (
      [
        ["obCi", r.workflows],
        ["obInstructions", r.instructions],
        ["obClaudeConfig", r.claude],
        ["obTeamParts", (r.parts ?? []).map((p) => `${p.path || r.name} (${p.role})`)],
        ["obTeamTests", r.tests ?? []],
        ["obTeamInfra", r.infra ?? []],
      ] as const
    ).filter(([, list]) => list.length);
</script>

{#snippet decided(proposal: string)}
  {@const d = ob?.decisions[proposal]}
  {#if d}<StatusPill status={d} />{/if}
{/snippet}

{#snippet inline(text: string)}
  {#each spans(text) as sp, i (i)}{#if sp.kind === "code"}<code>{sp.text}</code>{:else if sp.kind === "strong"}{@render inline(sp.spans.map((s) => ("text" in s ? s.text : "")).join(""))}{:else}{sp.text}{/if}{/each}
{/snippet}

{#snippet facts(a: Analysis)}
  <div class="facts">
    {#each a.repos as r (r.path)}
      {@const found = a.findings.filter((f) => f.repo === r.name)}
      <article class="fact">
        <header>
          <FolderGit2 size={14} aria-hidden="true" />
          <b>{r.name}</b>
          <span class="mono" title={r.path}>{shortPath(r.path)}</span>
        </header>
        <dl>
          <dt>{t("obStack")}</dt>
          <dd>
            {#each r.stack as s (s)}<span class="badge">{s}</span>{:else}<span class="muted">{t("obNothing")}</span>{/each}
          </dd>
          {#each rows(r) as [label, list] (label)}
            <dt>{t(label)}</dt>
            <dd>{list.join(", ")}</dd>
          {/each}
          {#each found as f, i (i)}
            <dt class="found">{f.label}</dt>
            <dd class="found">{@render inline(f.value)}</dd>
          {/each}
        </dl>
      </article>
    {/each}
    <article class="fact">
      <header><b>{project?.name}</b></header>
      <dl>
        <dt>{t("obGithub")}</dt>
        <dd>{none(a.github)}</dd>
        <dt>{t("obTickets")}</dt>
        <dd>{none(a.tickets)}</dd>
        <dt>{t("obLibrary")}</dt>
        <dd>{a.library.length ? t("obItemsOn", { count: a.library.length }) : t("obNothing")}</dd>
        <dt>{t("obRecipe")}</dt>
        <dd>{a.recipe ? t("obRecipeSaved") : t("obRecipeNone")}</dd>
      </dl>
    </article>
    {#if a.asked}
      <p class="meta">
        <Sparkles size={12} aria-hidden="true" />
        {t("obFound", { model: a.model })}
        {#if a.cost !== null}· {t("convCost", { cost: approxUsd(money.format(a.cost)) })}{/if}
      </p>
    {/if}
    {#if a.dropped.length}
      <details class="meta">
        <summary>{t("convDropped", { count: a.dropped.length })}</summary>
        <ul>
          {#each a.dropped as why, i (i)}<li>{why}</li>{/each}
        </ul>
      </details>
    {/if}
  </div>
{/snippet}

{#snippet answer(q: Question)}
  <QuestionInput
    kind={q.kind}
    options={q.options}
    label={qText(q)}
    optionLabel={(o) => optionText(q, o)}
    value={answers[q.id] ?? q.default}
    onchange={(v) => (answers[q.id] = v)}
  />
{/snippet}

<div class="onboarding">
  <ViewHeader title={t("obConfigure")} context={project?.name ?? ""}>
    {#snippet actions()}
      <button class="btn sm" onclick={closeSetup}>{t("obLater")}</button>
    {/snippet}
  </ViewHeader>

  {#if error}
    <div class="banners">
      <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
    </div>
  {/if}

  <div class="content">
    {#if ob}
      <ol class="steps">
        {#each steps.slice(0, 4) as step, i (step)}
          {@const past = steps.indexOf(step) < steps.indexOf(ob.step)}
          <li>
            <button
              class="step"
              aria-current={shown === step ? "step" : undefined}
              disabled={!reachable(step, ob.step, !!analysis) && step !== shown}
              onclick={() => (shown = step)}
            >
              <span class="num" class:past>
                {#if past}<Check size={12} aria-hidden="true" />{:else}{i + 1}{/if}
              </span>
              {t(stepLabel[step])}
            </button>
          </li>
        {/each}
      </ol>

      {#if shown === "analysis"}
        <section class="panel">
          <h2><ScanSearch size={16} aria-hidden="true" />{t("obAnalysisTitle")}</h2>
          <p class="lead">{t("obAnalysisIntro", { budget: money.format(2) })}</p>
          {#if inProgress}
            {@const live = reads[project?.id ?? ""]}
            <div class="working" role="status">
              <p><LoaderCircle size={14} aria-hidden="true" class="spin" />{t("obAnalyzing")}</p>
              {#if live?.last.length}
                <ol class="feed">
                  {#each live.last as r, i (live.count - live.last.length + i)}
                    <li>
                      {#if r.tool === "Read"}<FileText size={12} aria-hidden="true" />{:else}<Search size={12} aria-hidden="true" />{/if}
                      <span class="mono">{readText(r)}</span>
                    </li>
                  {/each}
                </ol>
              {:else}
                <p class="muted">{t("scoutStarting")}</p>
              {/if}
            </div>
          {/if}
          {#if analysis}
            {@render facts(analysis)}
          {/if}
          <footer class="actions">
            {#if analysis && !analysis.asked && !inProgress}
              <button class="btn" onclick={() => go("questions")}>{t("obContinueWithout")}</button>
            {/if}
            <button class="btn" class:primary={!analysis?.asked} disabled={!!busy || inProgress} onclick={analyze}>
              {analysis ? t("obReanalyze") : t("obAnalyze")}
            </button>
            {#if analysis?.asked}
              <button class="btn primary" disabled={!!busy || inProgress} onclick={() => (shown = "questions")}>
                {t("obContinue")}
              </button>
            {/if}
          </footer>
        </section>
      {:else if shown === "questions" && analysis}
        <div class="two">
          <aside>{@render facts(analysis)}</aside>
          <section class="panel">
            <h2>{t("obQuestionsTitle")}</h2>
            <p class="lead">{analysis.questions.length ? t("obQuestionsHelp") : t("obNoQuestions")}</p>
            {#each byTheme(analysis.questions) as [theme, list] (theme)}
              <fieldset>
                <legend>{t(themeLabel[theme])}</legend>
                {#each list as q (q.id)}
                  <div class="question">
                    <span class="q-text">{qText(q)}</span>
                    {@render answer(q)}
                  </div>
                {/each}
              </fieldset>
            {/each}
            {#if ob.edited}
              <p class="notice warn" role="status">{t("obEdited")}</p>
            {/if}
            <footer class="actions">
              <button class="btn" onclick={() => (shown = "analysis")}>{t("obBack")}</button>
              <button class="btn primary" disabled={!!busy} onclick={submitAnswers}>
                {#if busy === "answer"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
                {ob.edited ? t("obReplace") : t("obContinue")}
              </button>
            </footer>
          </section>
        </div>
      {:else if shown === "proposals" && project}
        <section class="panel">
          <h2>{t("obProposalsTitle")}</h2>
          <p class="lead">{t("obProposalsHelp")}</p>

          <article class="proposal">
            <h3><LibraryBig size={15} aria-hidden="true" />{t("obLibraryTitle")}</h3>
            <p class="hint">{t("obLibraryHint")}</p>
            <Conventions {project} only="onboarding" />
          </article>

          <StackSkills project={project.id} {stacks} ondecide={(decision, detail) => decide("skills", decision, detail)} />
          <StackOffers {stacks} ondecide={(detail) => decide("mcp", "accepted", detail)} />

          <OnboardingTeam project={project.id} decision={ob.decisions.team} ondecided={(s) => (ob = s as Onboarding)} />

          <article class="proposal">
            <h3><Ticket size={15} aria-hidden="true" />{t("obTicketsTitle")} {@render decided("tickets")}</h3>
            {#if sources}
              {@const kind = ticketProposal(sources.trackers, sources.githubRepos)}
              <p>
                {#if kind === "saved"}{t("obTicketsSaved", { sources: none(analysis?.tickets ?? []) })}
                {:else if kind === "github"}{t("obTicketsGithub", { repos: sources.githubRepos.join(", ") })}
                {:else}{t("obTicketsNone")}{/if}
              </p>
              {#if editingSources}
                <TicketSources project={project.id} bind:this={sourcesEditor} />
                <div class="row-actions">
                  <button class="btn sm" onclick={() => (editingSources = false)}>{t("cancel")}</button>
                  <button class="btn sm primary" disabled={!!busy} onclick={saveSources}>{t("save")}</button>
                </div>
              {:else}
                <div class="row-actions">
                  {#if kind === "github"}
                    <button class="btn sm" aria-pressed={ob.decisions.tickets === "accepted"} disabled={!!busy} onclick={acceptTickets}>
                      <Check size={14} aria-hidden="true" />{t("convAccept")}
                    </button>
                    <button class="btn sm" aria-pressed={ob.decisions.tickets === "rejected"} disabled={!!busy} onclick={() => decide("tickets", "rejected")}>
                      <X size={14} aria-hidden="true" />{t("convReject")}
                    </button>
                  {/if}
                  <button class="btn sm quiet" onclick={() => (editingSources = true)}>{t("obEdit")}</button>
                </div>
              {/if}
            {/if}
          </article>

          <article class="proposal">
            <h3><FolderGit2 size={15} aria-hidden="true" />{t("obRecipeTitle")} {@render decided("worktree")}</h3>
            {#if recipe?.saved && !recipe.draft}
              <p>{t("obRecipeAlready")}</p>
              <div class="row-actions">
                <button class="btn sm quiet" onclick={editRecipe}>{t("obEdit")}</button>
              </div>
            {:else if recipe && proposedRecipe}
              <p class="hint">{recipe.draft ? t("obRecipeDraft") : t("obRecipeDetected")}</p>
              <ul class="lines">
                {#each recipeLines(proposedRecipe.repos) as line (line.repo)}
                  <li>
                    <b>{line.repo}</b>
                    {#each line.parts as part, i (i)}<code>{part}</code>{:else}<span class="muted">{t("obRecipeNothing")}</span>{/each}
                  </li>
                {/each}
              </ul>
              {#if busy === "recipe"}
                <p class="notice" role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" />{t("obAnalyzing")}</p>
              {/if}
              <div class="row-actions">
                <button class="btn sm" aria-pressed={ob.decisions.worktree === "accepted"} disabled={!!busy} onclick={acceptRecipe}>
                  <Check size={14} aria-hidden="true" />{t("convAccept")}
                </button>
                <button class="btn sm" aria-pressed={ob.decisions.worktree === "rejected"} disabled={!!busy} onclick={() => decide("worktree", "rejected")}>
                  <X size={14} aria-hidden="true" />{t("convReject")}
                </button>
                {#if !recipe.draft}
                  <button class="btn sm quiet" disabled={!!busy} onclick={proposeRecipe}>
                    <Sparkles size={14} aria-hidden="true" />{t("obRecipePropose")}
                  </button>
                {/if}
                <button class="btn sm quiet" onclick={editRecipe}>{t("obEdit")}</button>
              </div>
            {/if}
          </article>

          <article class="proposal">
            <h3><Workflow size={15} aria-hidden="true" />{t("obCiTitle")} {@render decided("ci")}</h3>
            {#if analysis}
              {@const withCi = analysis.repos.filter((r) => r.workflows.length)}
              <p>
                {#if withCi.length}
                  {t("obCiWorkflows", { list: withCi.map((r) => `${r.name} (${r.workflows.join(", ")})`).join(" · ") })}
                {:else}
                  {t("obCiNone")}
                {/if}
              </p>
            {/if}
            <div class="row-actions">
              <span class="muted">{t("ciNotify")}</span>
              <div class="segmented" role="radiogroup" aria-label={t("ciNotify")}>
                {#each notifyOptions as value (value)}
                  <button role="radio" aria-checked={notify === value} onclick={() => (notify = value)}>{t(notifyLabel[value])}</button>
                {/each}
              </div>
              <span class="grow"></span>
              <button class="btn sm" aria-pressed={ob.decisions.ci === "accepted"} disabled={!!busy} onclick={acceptCi}>
                <Check size={14} aria-hidden="true" />{t("convAccept")}
              </button>
              <button class="btn sm" aria-pressed={ob.decisions.ci === "rejected"} disabled={!!busy} onclick={() => decide("ci", "rejected")}>
                <X size={14} aria-hidden="true" />{t("convReject")}
              </button>
            </div>
          </article>

          <footer class="actions">
            <button class="btn" onclick={() => (shown = "questions")}>{t("obBack")}</button>
            <button class="btn primary" disabled={!!busy} onclick={() => go("trust")}>{t("obContinue")}</button>
          </footer>
        </section>
      {:else if shown === "trust" && project}
        <section class="panel">
          <h2><ShieldCheck size={16} aria-hidden="true" />{t("obTrustTitle")}</h2>
          <ClaudeTrust project={project.id} />
          <footer class="actions">
            <button class="btn" onclick={() => (shown = "proposals")}>{t("obBack")}</button>
            <button class="btn primary" disabled={!!busy} onclick={() => go("done")}>{t("obFinish")}</button>
          </footer>
        </section>
      {:else if shown === "done"}
        <section class="panel">
          <h2><Check size={16} aria-hidden="true" />{t("obDoneTitle")}</h2>
          <p class="lead">{t("obDoneHelp")}</p>
          {#if ob.log.length}
            <h3>{t("obJournal")}</h3>
            <OnboardingJournal log={ob.log} />
          {/if}
          <footer class="actions">
            <button class="btn" onclick={() => (shown = "trust")}>{t("obBack")}</button>
            <button class="btn primary" onclick={closeSetup}>{t("close")}</button>
          </footer>
        </section>
      {/if}
    {:else if !error}
      <Skeleton rows={6} />
    {/if}
  </div>
</div>

<style>
  .onboarding {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }

  .content {
    flex: 1;
    min-height: 0;
    padding: 20px 32px 40px;
    overflow-y: auto;
  }

  .steps {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0 0 20px;
    padding: 0;
    list-style: none;
  }

  .step {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 12px 0 6px;
    border: 0;
    border-radius: var(--radius-pill);
    background: none;
    color: var(--muted);
    font-weight: 500;
  }

  .step:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }

  .step[aria-current="step"] {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .step:disabled {
    opacity: 0.55;
  }

  .num {
    display: inline-grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    font-size: var(--text-xs);
  }

  .num.past {
    border-color: var(--done);
    background: var(--done-soft);
    color: var(--done);
  }

  .step[aria-current="step"] .num {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 920px;
    min-width: 0;
  }

  h2,
  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
  }

  h2 {
    font-size: var(--text-xl);
    font-weight: 650;
  }

  h3 {
    font-size: var(--text-md);
    font-weight: 600;
  }

  .lead,
  .hint {
    margin: 0;
    color: var(--muted);
    line-height: 1.5;
  }

  .two {
    display: grid;
    grid-template-columns: minmax(240px, 320px) minmax(0, 1fr);
    gap: 24px;
    align-items: start;
    max-width: 1180px;
  }

  @media (max-width: 900px) {
    .two {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .facts {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .fact {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .fact header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
    color: var(--muted);
  }

  .fact header b {
    white-space: nowrap;
    color: var(--text);
    font-weight: 600;
  }

  .fact header .mono {
    min-width: 0;
    overflow: hidden;
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 4px 10px;
    margin: 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .found {
    padding-top: 4px;
    border-top: 1px dashed var(--border);
  }


  .found ~ .found {
    padding-top: 0;
    border-top: 0;
  }

  dd.found {
    display: block;
  }

  .working {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 12px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--working-soft);
    color: var(--working);
    font-size: var(--text-sm);
  }

  .working > p {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-weight: 500;
  }

  .feed {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin: 0;
    padding: 0;
    color: var(--text);
    list-style: none;
  }

  .feed li {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    animation: enter var(--dur) var(--ease-out);
  }

  .feed li > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .feed :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  @keyframes enter {
    from {
      opacity: 0;
      transform: translateY(3px);
    }
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  details.meta {
    display: block;
  }

  fieldset {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin: 0;
    padding: 14px 16px 16px;
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

  .question {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }

  .q-text {
    font-weight: 500;
  }

  .segmented {
    flex-wrap: wrap;
  }

  .proposal {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .proposal p {
    margin: 0;
  }

  .row-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .grow {
    flex: 1;
  }

  .lines {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .lines li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .lines b {
    min-width: 80px;
    font-weight: 600;
  }

  code {
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
