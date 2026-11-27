<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Bot from "@lucide/svelte/icons/bot";
  import Boxes from "@lucide/svelte/icons/boxes";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Cylinder from "@lucide/svelte/icons/cylinder";
  import FileCode from "@lucide/svelte/icons/file-code";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import Network from "@lucide/svelte/icons/network";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Rocket from "@lucide/svelte/icons/rocket";
  import Server from "@lucide/svelte/icons/server";
  import Variable from "@lucide/svelte/icons/variable";
  import Workflow from "@lucide/svelte/icons/workflow";
  import RepoChoice from "$lib/RepoChoice.svelte";
  import StackDeps from "$lib/StackDeps.svelte";
  import StackMap from "$lib/StackMap.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { provide } from "$lib/ask.svelte";
  import { ci, showCi, type Target } from "$lib/ci.svelte";
  import type { Listing } from "$lib/connectors-logic";
  import { openAt } from "$lib/design.svelte";
  import { chooseRepo, currentProject, scopeRepos, shownRepo, shownRepos } from "$lib/files.svelte";
  import { showGit } from "$lib/git.svelte";
  import { ago, t } from "$lib/i18n";
  import { ALL_REPOS } from "$lib/nav-logic";
  import { loadSchema, ops, showOps } from "$lib/ops.svelte";
  import type { Connection } from "$lib/ops-logic";
  import { editProject } from "$lib/project.svelte";
  import { app, onScreen } from "$lib/state.svelte";
  import { ending, headline, rows, short, tally, type Check, type Database, type RepoMap, type StackMap as Map } from "$lib/stack-logic";

  const inProject = $derived(app.project !== null);
  /** The map is the project's; out of a project, the repository chosen. */
  const mapped = $derived(inProject ? scopeRepos().map((r) => r.path) : shownRepos());
  const chosen = $derived(shownRepo());
  const today = new Date().toISOString().slice(0, 10);

  let data = $state<Map | null>(null);
  let readFor = "";
  let reading = $state(false);
  let error = $state("");
  let checks = $state<Record<string, Check | { error: string }>>({});
  let checking = $state(0);
  let round = 0;

  $effect(() => {
    const repos = mapped;
    if (onScreen("stack") && repos.length) untrack(() => read(repos, false));
  });

  // ⌘I: devops answers, about the repository shown.
  $effect(() => provide("stack", { repo: chosen || undefined, chips: [] }));

  /** The map from the files at once, then each repository's check as it comes: from the cache unless `refresh`. */
  async function read(repos: string[], refresh: boolean) {
    const n = ++round;
    const key = repos.join("\n");
    if (key !== readFor) {
      data = null;
      checks = {};
      readFor = key;
    }
    reading = true;
    error = "";
    try {
      const found = await invoke<Map>("stack_map", { repos });
      if (n === round) data = found;
    } catch (e) {
      if (n === round) error = String(e);
    } finally {
      if (n === round) reading = false;
    }
    for (const repo of repos) {
      checking++;
      invoke<Check>("stack_check", { repo, refresh })
        .then(
          (c) => {
            if (n === round) checks[repo] = c;
          },
          (e) => {
            if (n === round) checks[repo] = { error: String(e) };
          },
        )
        .finally(() => checking--);
    }
  }

  const checkOf = (path: string): Check | null => {
    const c = checks[path];
    return c && !("error" in c) ? c : null;
  };
  const tallies = $derived(
    Object.fromEntries((data?.repos ?? []).map((r) => [r.path, checks[r.path] ? tally(rows(r, checkOf(r.path), today), today) : ("loading" as const)])),
  );
  const selected = $derived(data?.repos.find((r) => r.path === chosen) ?? null);
  const done = $derived(Object.values(checks).filter((c): c is Check => !("error" in c)));
  const offline = $derived(done.some((c) => c.offline));
  const failed = $derived(Object.values(checks).filter((c): c is { error: string } => "error" in c));
  const checkedAt = $derived(done.length ? Math.min(...done.map((c) => c.checked)) : 0);

  // What surrounds the repositories: the project's deployments, Ops connections and agents.
  let targets = $state<Target[] | null>(null);
  let connections = $state<Connection[] | null>(null);
  let listing = $state<Listing | null>(null);
  $effect(() => {
    const project = app.project;
    if (!project || !onScreen("stack")) return;
    untrack(() => {
      invoke<Target[]>("deploy_targets", { project }).then((v) => (targets = v), () => (targets = []));
      invoke<{ connections: Connection[] }>("ops_overview", { project }).then((v) => (connections = v.connections), () => (connections = []));
      invoke<Listing>("connectors_list", { project }).then((v) => (listing = v), () => (listing = null));
    });
  });
  const envs = ["prod", "staging", "dev"] as const;
  const workflows = $derived((data?.repos ?? []).flatMap((r) => r.workflows.map((w) => ({ ...w, repo: r }))));

  const DB_KINDS = ["postgres", "mysql", "mariadb", "sqlite", "clickhouse", "redis"];
  /** The Ops connections to a database, by its engine and name. */
  const connectionsOf = (db: Database) =>
    (connections ?? []).filter((c) => {
      if (!DB_KINDS.includes(c.kind)) return false;
      const engine = c.kind === "mariadb" ? "mysql" : c.kind;
      const name = c.kind === "sqlite" ? (c.path ?? "").split("/").pop() : c.database;
      return (engine === db.engine || c.kind === db.engine) && !!name && name === db.name;
    });
  const schemaOf = (conn: Connection) => (app.project ? ops.studio.schemas[`${app.project}/${conn.name}`] : undefined);

  function toGit(path: string) {
    chooseRepo(path);
    showGit();
  }
  function toCi(path: string) {
    chooseRepo(path);
    ci.tab = "runs";
    showCi();
  }
  function toDeploys() {
    ci.tab = "deploys";
    showCi();
  }
  function toConnection(name: string) {
    ops.tab = "connections";
    ops.connection = name;
    showOps();
  }
  function toStudio(conn: Connection) {
    if (conn.kind === "redis") {
      ops.tab = "redis";
      ops.redis.connection = conn.name;
    } else {
      ops.tab = "sql";
      ops.sql.connection = conn.name;
    }
    showOps();
  }
  /** The manifests the dependencies were read from, each once. */
  const manifests = (repo: RepoMap) => [...new Set(repo.deps.map((d) => d.manifest))];
</script>

{#snippet summary(repo: RepoMap)}
  {@const count = tallies[repo.path]}
  <li>
    <button class="item" title={repo.path} onclick={() => chooseRepo(repo.path)}>
      <span class="name">{repo.name}</span>
      <span class="tags">
        {#each headline(repo.tags, 4) as tag (tag.name)}
          <span class="tag">{tag.name}{#if tag.version}&nbsp;<span class="v">{short(tag.version)}</span>{/if}</span>
        {/each}
      </span>
      <span class="what">
        <span>{t("stackDeps", { count: repo.deps.length })}</span>
        {#if count === "loading"}
          <span class="bone" aria-hidden="true"></span>
        {:else if count}
          {#if count.vulnerable}<span class="sev sev-{count.worst}">{t("stackBadgeVulns", { count: count.vulnerable })}</span>{/if}
          {#if count.eol}<span class="sev sev-high">{t("stackBadgeEol", { count: count.eol })}</span>{/if}
          {#if count.updates}<span class="upd">{t("stackBadgeUpdates", { count: count.updates })}</span>{/if}
        {/if}
      </span>
      <ChevronRight size={14} aria-hidden="true" class="go" />
    </button>
  </li>
{/snippet}

<div class="stack">
  <ViewHeader title={t("stackView")} context={currentProject()?.name ?? t("ticketsAll")}>
    {#if scopeRepos().length > 1}
      <label class="labelled inline">
        <span>{t("filesRepo")}</span>
        <RepoChoice />
      </label>
    {/if}
    {#snippet actions()}
      {#if checking}
        <span class="when">{t("stackChecking")}</span>
      {:else if checkedAt}
        <span class="when">{t("stackChecked", { when: ago(checkedAt) })}</span>
      {/if}
      <button class="btn sm" onclick={() => read(mapped, true)} disabled={!mapped.length || reading || checking > 0}>
        <RefreshCw size={14} aria-hidden="true" class={checking ? "spin" : ""} />{t("stackRecheck")}
      </button>
    {/snippet}
  </ViewHeader>

  <div class="page">
    {#if error}<Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>{/if}
    {#if offline}<Banner kind="warn">{t("stackOffline")}</Banner>{/if}
    {#each failed as f (f.error)}<Banner kind="error">{f.error}</Banner>{/each}

    {#if !mapped.length}
      <EmptyState
        icon={Network}
        title={t("stackNoRepo")}
        text={t("stackNoRepoHelp")}
        action={app.project ? { label: t("stackAddRepo"), run: () => app.project && editProject(app.project, "project") } : undefined}
      />
    {:else if !data}
      <section class="card"><Skeleton shape="block" height="220px" label={t("stackReading")} /></section>
      <section class="card"><Skeleton shape="table" rows={6} /></section>
    {:else}
      <section class="card" aria-labelledby="stack-map">
        <header class="card-head">
          <h2 id="stack-map">{t("stackMap")}</h2>
          <span class="privacy">{t("stackPrivacy")}</span>
        </header>
        <StackMap map={data} {tallies} selected={chosen} onpick={(path) => chooseRepo(path === chosen && inProject && mapped.length > 1 ? ALL_REPOS : path)} />
      </section>

      {#if inProject}
        <section class="around" aria-label={t("stackAround")}>
          <div class="tile">
            <h3><Workflow size={14} aria-hidden="true" />{t("stackCi")}</h3>
            {#if workflows.length}
              <ul>
                {#each workflows as w (`${w.repo.path}/${w.file}`)}
                  <li>
                    <button class="line" onclick={() => toCi(w.repo.path)} title={w.file}>
                      <span class="main">{w.name}</span><span class="sub">{w.repo.name}{w.jobs ? ` · ${t("stackJobs", { count: w.jobs })}` : ""}</span>
                    </button>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="none">{t("stackCiNone")}</p>
            {/if}
          </div>
          <div class="tile">
            <h3><Rocket size={14} aria-hidden="true" />{t("stackDeploys")}</h3>
            {#if targets === null}
              <Skeleton rows={2} />
            {:else if targets.length}
              <ul>
                {#each targets as target (target.name)}
                  <li>
                    <button class="line" onclick={toDeploys}>
                      <span class="main">{target.name}</span><span class="sub">{target.tool} · <span class="env env-{target.env}">{target.env}</span></span>
                    </button>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="none">{t("stackDeploysNone")}</p>
            {/if}
          </div>
          <div class="tile">
            <h3><Server size={14} aria-hidden="true" />{t("stackOps")}</h3>
            {#if connections === null}
              <Skeleton rows={2} />
            {:else if connections.length}
              <ul>
                {#each envs as env (env)}
                  {#each connections.filter((c) => c.env === env) as c (c.name)}
                    <li>
                      <button class="line" onclick={() => toConnection(c.name)}>
                        <span class="main">{c.name}</span><span class="sub">{c.kind} · <span class="env env-{env}">{env}</span></span>
                      </button>
                    </li>
                  {/each}
                {/each}
              </ul>
            {:else}
              <p class="none">{t("stackOpsNone")}</p>
            {/if}
          </div>
          <div class="tile">
            <h3><Bot size={14} aria-hidden="true" />{t("stackAgents")}</h3>
            {#if listing}
              <button class="line" onclick={() => app.project && editProject(app.project, "team")}>
                <span class="main">{t("stackRoles")}</span>
                <span class="sub wrap">{listing.roles.map((r) => r.name).join(", ")}</span>
              </button>
              <button class="line" onclick={() => app.project && editProject(app.project, "connectors")}>
                <span class="main">{t("stackTools")}</span>
                <span class="sub wrap">{listing.connectors.map((c) => c.name).join(", ") || t("stackToolsNone")}</span>
              </button>
            {:else}
              <Skeleton rows={2} />
            {/if}
          </div>
        </section>
      {/if}

      {#if selected}
        {@const check = checkOf(selected.path)}
        <section class="repo" aria-labelledby="stack-repo">
          <header class="repo-head">
            <h2 id="stack-repo">{selected.name}</h2>
            <span class="path">{selected.path}</span>
            <span class="spacer"></span>
            <button class="btn sm" onclick={() => toGit(selected.path)}><GitBranch size={14} aria-hidden="true" />{t("stackOpenGit")}</button>
          </header>
          <div class="strip">
            {#each selected.tags as tag (tag.name)}
              {@const s = check?.support.find((x) => x.tag === tag.name)}
              {@const end = ending(s ?? null, today)}
              <span class="chip" class:ended={end === "ended"} class:soon={end === "soon"} title={tag.source}>
                {tag.name}{#if tag.version}<span class="v">{tag.version}</span>{/if}
                {#if s?.currentLts && end}<span class="lts">{t("stackCurrentLts", { cycle: s.currentLts })}</span>{/if}
              </span>
            {/each}
          </div>
          <div class="facts">
            <div class="fact">
              <h3><FileCode size={14} aria-hidden="true" />{t("stackManifests")}</h3>
              <ul>
                {#each manifests(selected) as file (file)}
                  <li><button class="line" title={t("stackManifest", { file })} onclick={() => openAt(selected.path, file, 1)}><span class="main mono">{file}</span></button></li>
                {/each}
              </ul>
            </div>
            <div class="fact">
              <h3><Cylinder size={14} aria-hidden="true" />{t("stackDatabases")}</h3>
              {#each selected.databases as db (db.source)}
                {@const conns = connectionsOf(db)}
                <div class="db">
                  <span class="main mono">{db.name}</span>
                  <span class="sub">{db.engine}{db.host ? ` · ${db.host}${db.port ? `:${db.port}` : ""}` : ""} · {db.source}</span>
                  {#each conns as c (c.name)}
                    {@const schema = schemaOf(c)}
                    <div class="conn">
                      <button class="link" onclick={() => toStudio(c)}>{t(c.kind === "redis" ? "stackOpenRedis" : "stackOpenStudio", { name: c.name })}</button>
                      <span class="env env-{c.env}">{c.env}</span>
                      {#if Array.isArray(schema)}
                        <span class="sub">{t("stackTables", { count: schema.length, names: schema.slice(0, 4).map((x) => x.name).join(", ") + (schema.length > 4 ? "…" : "") })}</span>
                      {:else if c.kind !== "redis" && schema !== "loading"}
                        <button class="link" onclick={() => app.project && loadSchema(app.project, c.name)}>{t("stackReadSchema")}</button>
                      {/if}
                    </div>
                  {:else}
                    <div class="conn">
                      <span class="sub">{t("stackNoConnection")}</span>
                      <button class="link" onclick={() => toConnection("")}>{t("stackAddConnection")}</button>
                    </div>
                  {/each}
                </div>
              {:else}
                <p class="none">–</p>
              {/each}
            </div>
            <div class="fact">
              <h3><Boxes size={14} aria-hidden="true" />{t("stackServices")}</h3>
              {#each selected.services as s (s.name)}
                <div class="svc">
                  <span class="main">{s.name}</span>
                  <span class="sub mono">{s.image ?? "build"}{s.ports.length ? ` · ${s.ports.join(", ")}` : ""}</span>
                </div>
              {:else}
                <p class="none">–</p>
              {/each}
              {#if selected.ports.length}
                <p class="ports"><span>{t("stackPorts")}</span>{#each selected.ports as port (port)}<span class="port">{port}</span>{/each}</p>
              {/if}
            </div>
            <div class="fact">
              <h3><Variable size={14} aria-hidden="true" />{t("stackEnv")}{#if selected.envFile}<button class="link file" onclick={() => selected.envFile && openAt(selected.path, selected.envFile, 1)}>{selected.envFile}</button>{/if}</h3>
              {#if selected.env.length}
                <p class="keys">{#each selected.env as key (key)}<span class="key">{key}</span>{/each}</p>
              {:else}
                <p class="none">{t("stackEnvNone")}</p>
              {/if}
              {#if selected.workflows.length}
                <h3 class="later"><Workflow size={14} aria-hidden="true" />{t("stackWorkflows")}</h3>
                {#each selected.workflows as w (w.file)}
                  <button class="line" onclick={() => toCi(selected.path)} title={w.file}><span class="main">{w.name}</span><span class="sub">{w.jobs ? t("stackJobs", { count: w.jobs }) : ""}</span></button>
                {/each}
              {/if}
            </div>
          </div>
          {#if checks[selected.path] && "error" in checks[selected.path]}
            <StackDeps repo={selected} check={{ deps: [], support: [], errors: [], offline: false, checked: 0 }} {today} />
          {:else}
            <StackDeps repo={selected} {check} {today} />
          {/if}
        </section>
      {:else}
        <section aria-label={t("stackRepos")}>
          <ul class="rows">
            {#each data.repos as repo (repo.path)}{@render summary(repo)}{/each}
          </ul>
        </section>
      {/if}
    {/if}
  </div>
</div>

<style>
  .stack {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--panel);
  }

  .inline {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }

  .inline :global(select) {
    width: 200px;
  }

  .when {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .page {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 16px;
    min-height: 0;
    padding: 16px 24px 48px;
    overflow-y: auto;
  }

  .card {
    padding: 16px 18px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg);
  }

  .card-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 6px 16px;
    margin-bottom: 10px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 8px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  h3.later {
    margin-top: 14px;
  }

  .privacy {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .around,
  .facts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
    gap: 12px;
  }

  .tile,
  .fact {
    min-width: 0;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
  }

  .tile ul,
  .fact ul {
    display: flex;
    flex-direction: column;
    max-height: 168px;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  .line {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    width: 100%;
    padding: 5px 6px;
    margin: 0 -6px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
  }

  .line:hover {
    background: var(--hover);
  }

  .main {
    max-width: 100%;
    overflow: hidden;
    font-size: var(--text-sm);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub {
    max-width: 100%;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub.wrap {
    white-space: normal;
  }

  .mono {
    font-family: var(--font-mono);
    font-weight: 500;
  }

  .none {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .env {
    padding: 0 5px;
    border-radius: var(--radius-sm);
    background: var(--well);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .env-prod {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .env-staging {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .repo {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .repo-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 6px 12px;
  }

  .repo-head h2 {
    font-size: var(--text-lg);
  }

  .path {
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .spacer {
    flex: 1;
  }

  .strip {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--panel);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .chip .v {
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .chip.ended {
    border-color: var(--danger-line);
    background: var(--danger-soft);
    color: var(--danger);
  }

  .chip.soon {
    border-color: var(--waiting-line);
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .lts {
    font-size: var(--text-xs);
    opacity: 0.85;
  }

  .db + .db,
  .svc + .svc {
    margin-top: 8px;
  }

  .db,
  .svc {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .conn {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font-size: var(--text-xs);
    text-align: left;
  }

  .link:hover {
    text-decoration: underline;
  }

  .link.file {
    margin-left: auto;
    font-family: var(--font-mono);
    letter-spacing: 0;
    text-transform: none;
  }

  .ports,
  .keys {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 10px 0 0;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .keys {
    max-height: 120px;
    margin: 0;
    overflow-y: auto;
  }

  .port,
  .key {
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--well);
    color: var(--soft);
    font-family: var(--font-mono);
  }

  .rows {
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
    list-style: none;
  }

  .rows li + li {
    border-top: 1px solid var(--border);
  }

  .item {
    display: grid;
    grid-template-columns: minmax(120px, 180px) minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 14px;
    width: 100%;
    min-height: var(--row-lg);
    padding: 8px 14px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text);
    text-align: left;
  }

  .item:hover {
    background: var(--hover);
  }

  .item .name {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item :global(.go) {
    color: var(--muted);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .tag {
    padding: 1px 7px;
    border-radius: var(--radius-pill);
    background: var(--well);
    color: var(--soft);
    font-size: var(--text-xs);
  }

  .tag .v {
    color: var(--muted);
    font-family: var(--font-mono);
  }

  .what {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 6px 10px;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .sev {
    padding: 1px 7px;
    border-radius: var(--radius-sm);
    font-weight: 600;
  }

  .sev-critical,
  .sev-high {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .sev-moderate {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .sev-low,
  .sev-unknown {
    background: var(--well);
    color: var(--soft);
  }

  .upd {
    color: var(--accent-ink);
    font-weight: 600;
  }

  .bone {
    width: 120px;
    height: 12px;
    border-radius: var(--radius-pill);
    background: var(--well);
  }
</style>
