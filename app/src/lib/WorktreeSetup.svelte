<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Bot from "@lucide/svelte/icons/bot";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleMinus from "@lucide/svelte/icons/circle-minus";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import Container from "@lucide/svelte/icons/container";
  import Database from "@lucide/svelte/icons/database";
  import FileLock from "@lucide/svelte/icons/file-lock";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Package from "@lucide/svelte/icons/package";
  import Plus from "@lucide/svelte/icons/plus";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Terminal from "@lucide/svelte/icons/terminal";
  import X from "@lucide/svelte/icons/x";
  import { ago, lang, t } from "$lib/i18n";
  import { library } from "$lib/library.svelte";
  import { shortPath, type Project } from "$lib/state.svelte";

  type Dep = { dir: string; clone: boolean; install: string[] };
  type RepoRecipe = {
    stack: string[];
    include: string[];
    deps: Dep[];
    database?: string | null;
    compose: boolean;
    ports: string[];
    commands: string[];
  };
  type Recipe = { agent: boolean; notes: string; repos: Record<string, RepoRecipe> };
  type Engine = "postgres" | "mysql" | "sqlite";
  type DbView = { engine: Engine | null; host: string; name: string; service: string | null; refused: string | null };
  type View = {
    recipe: Recipe;
    saved: boolean;
    detected: Recipe;
    draft: { recipe: Recipe; cost: number | null; dropped: string[] } | null;
    repos: { name: string; path: string; databases: Record<string, DbView> }[];
    model: string;
    effort: string;
  };
  type Kind = "recipe" | "include" | "deps" | "database" | "env" | "command" | "agent";
  type Outcome =
    | "copied"
    | "cloned"
    | "installed"
    | "present"
    | "created"
    | "ran"
    | "set"
    | "dropped"
    | "released"
    | "skipped"
    | "refused"
    | "failed";
  type Step = { repo: string; kind: Kind; target: string; outcome: Outcome; detail: string; ms: number };
  type Report = { steps: Step[]; env: Record<string, string>; saved: boolean };
  type RunSetup = {
    run: { id: number; ticket: { key: string }; title: string; repo: string; worktree: string | null };
    at: number | null;
    steps: Step[];
    saved: boolean;
  };

  let { project }: { project: Project } = $props();

  let view = $state<View | null>(null);
  let edit = $state<Recipe | null>(null);
  let runs = $state<RunSetup[]>([]);
  /** The slow call in progress: "propose", "save", "rerun:<run>", or "" when idle. */
  let running = $state("");
  let error = $state("");

  const pristine = $derived(view ? JSON.stringify(editable(view.draft?.recipe ?? view.recipe)) : "");
  const dirty = $derived(!!edit && JSON.stringify(edit) !== pristine);
  const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD" });
  const seconds = new Intl.NumberFormat(lang, { maximumFractionDigits: 1, minimumFractionDigits: 1 });

  const kindLabel = {
    recipe: "wtKindRecipe",
    include: "wtKindInclude",
    deps: "wtKindDeps",
    database: "wtKindDatabase",
    env: "wtKindEnv",
    command: "wtKindCommand",
    agent: "wtKindAgent",
  } as const;
  const outcomeLabel = {
    copied: "wtCopied",
    cloned: "wtCloned",
    installed: "wtInstalled",
    present: "wtPresent",
    created: "wtCreated",
    ran: "wtRan",
    set: "wtSet",
    dropped: "wtDropped",
    released: "wtReleased",
    skipped: "wtSkipped",
    refused: "wtRefused",
    failed: "wtFailed",
  } as const;
  const engineName: Record<Engine, string> = { postgres: "PostgreSQL", mysql: "MySQL", sqlite: "SQLite" };

  const emptyRepo = (): RepoRecipe => ({
    stack: [],
    include: [],
    deps: [],
    database: null,
    compose: false,
    ports: [],
    commands: [],
  });

  /** A copy to edit, with a section for every repository of the project. */
  function editable(recipe: Recipe): Recipe {
    const copy: Recipe = JSON.parse(JSON.stringify(recipe));
    for (const r of view?.repos ?? []) copy.repos[r.name] ??= emptyRepo();
    for (const repo of Object.values(copy.repos)) repo.database ??= null;
    return copy;
  }

  function show(next: View) {
    view = next;
    edit = editable(next.draft?.recipe ?? next.recipe);
  }

  async function call<T>(command: string, args: Record<string, unknown>, what: string): Promise<T | undefined> {
    running = what;
    error = "";
    try {
      return await invoke<T>(command, { project: project.id, ...args });
    } catch (e) {
      error = String(e);
    } finally {
      running = "";
    }
  }

  async function refresh(command = "worktree_recipe", args: Record<string, unknown> = {}, what = "load") {
    const next = await call<View>(command, args, what);
    if (next) show(next);
    if (command !== "worktree_recipe") library.version++;
  }

  onMount(async () => {
    await refresh();
    runs = (await call<RunSetup[]>("worktree_runs", {}, "")) ?? [];
  });

  async function rerun(item: RunSetup) {
    if (!item.run.worktree) return;
    const report = await call<Report>("worktree_setup", { path: item.run.worktree }, `rerun:${item.run.id}`);
    if (!report) return;
    item.steps = report.steps;
    item.saved = report.saved;
    item.at = Math.floor(Date.now() / 1000);
  }

  // Lines and commas map one to one to the list, so typing never moves the cursor; the backend trims.
  const lines = (value: string) => (value === "" ? [] : value.split("\n"));
  const commas = (value: string) => (value === "" ? [] : value.split(","));

  function known(repo: RepoRecipe): boolean {
    return !!(repo.stack.length || repo.deps.length || repo.include.length || repo.database || repo.commands.length);
  }

  function database(info: View["repos"][number], repo: RepoRecipe): { text: string; refused: boolean } | null {
    if (!repo.database) return null;
    const db = info.databases[repo.database];
    if (!db) return { text: t("wtDatabaseUnchecked"), refused: false };
    if (db.refused) return { text: t("wtDatabaseRefused", { why: db.refused }), refused: true };
    const engine = db.engine ? engineName[db.engine] : "";
    if (db.engine === "sqlite") return { text: t("wtDatabaseFile", { name: db.name }), refused: false };
    const copy = `${db.name}_termf_<run>`;
    if (db.service) return { text: t("wtDatabaseService", { engine, name: db.name, service: db.service, copy }), refused: false };
    const host = db.host || t("wtLocalSocket");
    return { text: t("wtDatabaseLocal", { engine, name: db.name, host, copy }), refused: false };
  }

  const failed = (s: Step) => s.outcome === "failed" || s.outcome === "refused";
  const repoName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
</script>

{#snippet stepList(steps: Step[])}
  <ol class="steps">
    {#each steps as s, i (i)}
      <li class:bad={failed(s)} class:skip={s.outcome === "skipped"}>
        {#if failed(s)}
          <CircleX size={14} aria-hidden="true" />
        {:else if s.outcome === "skipped"}
          <CircleMinus size={14} aria-hidden="true" />
        {:else}
          <CircleCheck size={14} aria-hidden="true" />
        {/if}
        <span class="kind">{t(kindLabel[s.kind])}</span>
        <span class="target">{s.target}</span>
        <span class="outcome">{t(outcomeLabel[s.outcome])}</span>
        <span class="ms">{t("wtSeconds", { s: seconds.format(s.ms / 1000) })}</span>
        {#if s.detail}<p class="detail">{s.detail}</p>{/if}
      </li>
    {/each}
  </ol>
{/snippet}

<div class="setup">
  <div class="pane">
    <header class="head">
      <div>
        <div class="title">
          <h2>{t("wtTitle")}</h2>
          {#if view?.draft}
            <span class="badge draft">{t("wtDraftBadge")}</span>
          {:else if view?.saved}
            <span class="badge saved">{t("wtSaved")}</span>
          {:else if view}
            <span class="badge detected">{t("wtDetected")}</span>
          {/if}
        </div>
        <p class="intro">{t("wtIntro")}</p>
      </div>
    </header>

    <div class="actions">
      <button class="btn" onclick={() => refresh("worktree_recipe_propose", {}, "propose")} disabled={!!running || !view}>
        {#if running === "propose"}
          <LoaderCircle size={14} aria-hidden="true" class="spin" />
        {:else}
          <Sparkles size={14} aria-hidden="true" />
        {/if}
        {t("wtPropose")}
      </button>
      {#if view && (view.saved || view.draft)}
        <button class="btn quiet" onclick={() => view && (edit = editable(view.detected))} disabled={!!running}>
          {t("wtReset")}
        </button>
      {/if}
      <span class="grow muted small">{dirty ? t("convUnsaved") : ""}</span>
      {#if view?.draft}
        <button class="btn quiet" onclick={() => refresh("worktree_recipe_discard", {}, "save")} disabled={!!running}>
          {t("convDiscard")}
        </button>
      {/if}
      <button
        class="btn primary"
        onclick={() => refresh("worktree_recipe_save", { recipe: edit }, "save")}
        disabled={!!running || !edit || (!dirty && !!view?.saved && !view?.draft)}
      >
        {t("save")}
      </button>
    </div>

    {#if running === "propose"}
      <p class="notice working" role="status">
        <LoaderCircle size={14} aria-hidden="true" class="spin" />{t("wtProposing", { model: view?.model ?? "" })}
      </p>
    {/if}
    {#if error}
      <p class="notice error" role="alert">{error}</p>
    {/if}
    {#if view?.draft}
      <div class="notice proposal">
        <Sparkles size={14} aria-hidden="true" />
        <div>
          <p>
            {t("wtDraft")}
            {#if view.draft.cost != null}<span class="muted">· {t("convCost", { cost: money.format(view.draft.cost) })}</span>{/if}
          </p>
          {#if view.draft.dropped.length}
            <details>
              <summary>{t("convDropped", { count: view.draft.dropped.length })}</summary>
              <ul>
                {#each view.draft.dropped as why, i (i)}<li>{why}</li>{/each}
              </ul>
            </details>
          {/if}
        </div>
      </div>
    {/if}

    {#if view && edit}
      {#each view.repos as info (info.name)}
        {@const repo = edit.repos[info.name]}
        {@const db = database(info, repo)}
        <article class="repo">
          <header>
            <h3>{info.name}</h3>
            <span class="path">{shortPath(info.path)}</span>
            <div class="stack">
              {#each repo.stack as s (s)}<span class="badge">{s.replace(":", " · ")}</span>{/each}
            </div>
          </header>

          {#if !known(repo)}
            <div class="notice unknown">
              <span>{t("wtUnknown")}</span>
              <button class="btn sm" onclick={() => refresh("worktree_recipe_propose", {}, "propose")} disabled={!!running}>
                <Sparkles size={14} aria-hidden="true" />{t("wtPropose")}
              </button>
            </div>
          {/if}

          <section class="part">
            <h4><Package size={14} aria-hidden="true" />{t("wtDeps")}</h4>
            <p class="help">{t("wtDepsHelp")}</p>
            {#each repo.deps as dep, i (i)}
              <div class="dep">
                <input
                  class="field mono dir"
                  bind:value={dep.dir}
                  placeholder={t("wtDepShared")}
                  aria-label={t("wtDepDir")}
                  disabled={!!running}
                />
                <label class="check">
                  <input type="checkbox" bind:checked={dep.clone} disabled={!!running || !dep.dir} />
                  {t("wtDepClone")}
                </label>
                <button
                  class="icon-btn sm remove"
                  onclick={() => repo.deps.splice(i, 1)}
                  aria-label={t("wtDepRemove")}
                  title={t("wtDepRemove")}
                  disabled={!!running}
                >
                  <X size={14} aria-hidden="true" />
                </button>
                <textarea
                  class="field mono install"
                  rows={Math.max(1, dep.install.length)}
                  value={dep.install.join("\n")}
                  oninput={(e) => (dep.install = lines(e.currentTarget.value))}
                  placeholder={t("wtDepInstall")}
                  aria-label={t("wtDepInstall")}
                  disabled={!!running}
                ></textarea>
              </div>
            {/each}
            <button
              class="btn sm quiet add"
              onclick={() => repo.deps.push({ dir: "", clone: false, install: [] })}
              disabled={!!running}
            >
              <Plus size={14} aria-hidden="true" />{t("wtDepAdd")}
            </button>
          </section>

          <section class="part">
            <h4><FileLock size={14} aria-hidden="true" />{t("wtInclude")}</h4>
            <p class="help">{t("wtIncludeHelp")}</p>
            <textarea
              class="field mono"
              rows={Math.max(2, repo.include.length)}
              value={repo.include.join("\n")}
              oninput={(e) => (repo.include = lines(e.currentTarget.value))}
              aria-label={t("wtInclude")}
              disabled={!!running}
            ></textarea>
          </section>

          <section class="part">
            <h4><Database size={14} aria-hidden="true" />{t("wtDatabase")}</h4>
            <p class="help">{t("wtDatabaseHelp")}</p>
            <label class="inline">
              <span>{t("wtDatabaseSource")}</span>
              <input
                class="field mono"
                list="db-{info.name}"
                value={repo.database ?? ""}
                oninput={(e) => (repo.database = e.currentTarget.value || null)}
                placeholder={t("wtDatabaseNone")}
                disabled={!!running}
              />
              <datalist id="db-{info.name}">
                {#each Object.keys(info.databases) as from (from)}<option value={from}></option>{/each}
              </datalist>
            </label>
            {#if db}
              <p class="db" class:refused={db.refused}>{db.text}</p>
            {/if}
          </section>

          <section class="part">
            <h4><Container size={14} aria-hidden="true" />{t("wtCompose")}</h4>
            <label class="check">
              <input type="checkbox" bind:checked={repo.compose} disabled={!!running} />
              {t("wtComposeName")}
            </label>
            <label class="inline">
              <span>{t("wtPorts")}</span>
              <input
                class="field mono"
                value={repo.ports.join(",")}
                oninput={(e) => (repo.ports = commas(e.currentTarget.value))}
                placeholder="web,vite"
                disabled={!!running}
              />
            </label>
            <p class="help">{t("wtPortsHelp")}</p>
          </section>

          <section class="part">
            <h4><Terminal size={14} aria-hidden="true" />{t("wtCommands")}</h4>
            <p class="help">{t("wtCommandsHelp")}</p>
            <textarea
              class="field mono"
              rows={Math.max(2, repo.commands.length)}
              value={repo.commands.join("\n")}
              oninput={(e) => (repo.commands = lines(e.currentTarget.value))}
              aria-label={t("wtCommands")}
              disabled={!!running}
            ></textarea>
          </section>
        </article>
      {/each}

      <article class="repo agent">
        <label class="toggle">
          <input class="switch" type="checkbox" role="switch" bind:checked={edit.agent} disabled={!!running} />
          <Bot size={16} aria-hidden="true" />
          <span>{t("wtAgent")}</span>
        </label>
        <p class="help">{t("wtAgentHelp")}</p>
        <label class="notes">
          <span>{t("wtNotes")}</span>
          <textarea
            class="field"
            rows="3"
            bind:value={edit.notes}
            placeholder={t("wtNotesPlaceholder")}
            disabled={!!running}
          ></textarea>
        </label>
      </article>

      <section class="runs">
        <h3>{t("wtRuns")}</h3>
        {#each runs as item (item.run.id)}
          {@const bad = item.steps.filter(failed).length}
          <article class="run">
            <header>
              <span class="key">{item.run.ticket.key}</span>
              <span class="run-title">{item.run.title}</span>
              <span class="muted small">{repoName(item.run.repo)}</span>
              <span class="grow"></span>
              <span class="muted small">
                {item.at ? t(item.saved ? "wtRanAt" : "wtRanDetected", { when: ago(item.at) }) : t("wtNeverRun")}
              </span>
              {#if bad}<span class="badge failed">{t("wtStepsFailed", { count: bad })}</span>{/if}
              <button class="btn sm" onclick={() => rerun(item)} disabled={!!running || !item.run.worktree}>
                {#if running === `rerun:${item.run.id}`}
                  <LoaderCircle size={14} aria-hidden="true" class="spin" />
                {:else}
                  <RotateCw size={14} aria-hidden="true" />
                {/if}
                {t("wtRerun")}
              </button>
            </header>
            {#if item.steps.length}{@render stepList(item.steps)}{/if}
          </article>
        {:else}
          <p class="muted small">{t("wtRunsEmpty")}</p>
        {/each}
      </section>
    {:else if !error}
      <p class="muted loading"><LoaderCircle size={14} aria-hidden="true" class="spin" /></p>
    {/if}
  </div>
</div>

<style>
  .setup {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .pane {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 900px;
    padding: 20px 28px 40px;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  h4 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  h4 :global(svg) {
    color: var(--muted);
  }

  .intro {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  .saved {
    border-color: transparent;
    background: var(--done-soft);
    color: var(--done);
  }

  .detected {
    border-color: transparent;
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .draft {
    border-color: transparent;
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .failed {
    border-color: transparent;
    background: var(--danger-soft);
    color: var(--danger);
  }

  .actions {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin: -8px 0;
    padding: 8px 0;
    background: var(--panel);
  }

  .grow {
    flex: 1;
  }

  .small {
    font-size: var(--text-sm);
  }

  .working {
    align-items: center;
    background: var(--working-soft);
    color: var(--working);
  }

  .proposal {
    background: var(--accent-soft);
    color: var(--text);
  }

  .proposal :global(svg) {
    flex-shrink: 0;
    margin-top: 3px;
    color: var(--accent-ink);
  }

  .proposal p,
  .proposal ul {
    margin: 0;
  }

  .proposal details {
    margin-top: 6px;
  }

  .proposal summary {
    color: var(--accent-ink);
    cursor: pointer;
  }

  .repo {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-1);
  }

  .repo > header {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 4px 10px;
  }

  .repo h3 {
    font-family: var(--font-mono);
  }

  .path {
    color: var(--muted);
    font: var(--text-xs) var(--font-mono);
  }

  .stack {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-left: auto;
  }

  .unknown {
    align-items: center;
    justify-content: space-between;
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .part {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .help {
    margin: -2px 0 2px;
    color: var(--muted);
    font-size: var(--text-xs);
    line-height: 1.45;
  }

  .mono {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  textarea.field {
    height: auto;
    padding: 6px 10px;
    line-height: 1.5;
    resize: vertical;
  }

  .dep {
    display: grid;
    grid-template-columns: 200px auto 1fr auto;
    grid-template-areas: "dir clone . remove" "install install install install";
    align-items: center;
    gap: 6px 10px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
  }

  .dir {
    grid-area: dir;
  }

  .dep .check {
    grid-area: clone;
  }

  .remove {
    grid-area: remove;
  }

  .install {
    grid-area: install;
  }

  .add {
    align-self: flex-start;
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .inline {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--text-sm);
  }

  .inline span {
    flex-shrink: 0;
    color: var(--muted);
  }

  .inline .field {
    max-width: 320px;
  }

  .db {
    margin: 0;
    font-size: var(--text-sm);
  }

  .db.refused {
    padding: 6px 10px;
    border-radius: var(--radius);
    background: var(--danger-soft);
    color: var(--danger);
  }

  .agent {
    gap: 8px;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 600;
    cursor: pointer;
  }

  .toggle :global(svg) {
    color: var(--accent-ink);
  }

  .notes {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .runs {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 6px;
  }

  .runs > p {
    margin: 0;
  }

  .run {
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .run header {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }

  .key {
    font: 600 var(--text-sm) var(--font-mono);
  }

  .run-title {
    font-weight: 500;
  }

  .steps {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .steps li {
    display: grid;
    grid-template-columns: 16px 96px minmax(0, 1fr) auto 48px;
    align-items: baseline;
    gap: 2px 8px;
  }

  .steps li :global(svg) {
    align-self: center;
    color: var(--done);
  }

  .steps li.bad :global(svg) {
    color: var(--danger);
  }

  .steps li.skip :global(svg) {
    color: var(--muted);
  }

  .steps .kind {
    color: var(--muted);
  }

  .steps .target {
    overflow: hidden;
    font: var(--text-xs) var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .steps .outcome {
    font-weight: 500;
  }

  .steps li.bad .outcome {
    color: var(--danger);
  }

  .steps .ms {
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .detail {
    grid-column: 3 / -1;
    margin: 0 0 2px;
    color: var(--muted);
    font: var(--text-xs) / 1.45 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .steps li.bad .detail {
    color: var(--danger);
  }

  .loading {
    margin-top: 40px;
    text-align: center;
  }
</style>
