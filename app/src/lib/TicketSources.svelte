<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Plus from "@lucide/svelte/icons/plus";
  import Plug from "@lucide/svelte/icons/plug";
  import X from "@lucide/svelte/icons/x";
  import JiraConnect, { type Connected } from "$lib/JiraConnect.svelte";
  import { loadInbox, tickets } from "$lib/tickets.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { t } from "$lib/i18n";

  type GithubTracker = {
    repo: string | null;
    comment: boolean;
    issues: boolean;
    project: string | null;
    columns: string[];
    mine: boolean;
    transition: string | null;
  };
  type JiraTracker = {
    site: string;
    email: string;
    jql: string | null;
    project: string | null;
    comment: boolean;
    transition: string | null;
  };
  type LinearTracker = {
    workspace: string;
    team: string | null;
    mine: boolean;
    states: string[];
    comment: boolean;
    transition: string | null;
  };
  type GitlabTracker = { host: string; repo: string | null; mine: boolean; comment: boolean };
  type AzureTracker = {
    organization: string;
    project: string;
    wiql: string | null;
    comment: boolean;
    transition: string | null;
  };
  type McpTracker = { server: string; tools: string[]; prompt: string | null };
  type Trackers = {
    github: GithubTracker | null;
    jira: JiraTracker | null;
    linear?: LinearTracker | null;
    gitlab?: GitlabTracker | null;
    azure?: AzureTracker | null;
    mcp: Record<string, McpTracker>;
    /** No `[trackers]` in the project: GitHub Issues on its repositories, as before sources existed. */
    implicit: boolean;
  };
  /** The sources whose token TermF keeps in the keychain, by workspace, host or organization. */
  type Kind = "linear" | "gitlab" | "azure";
  type Key = Parameters<typeof t>[0];
  type Sources = {
    trackers: Trackers;
    jiraToken: boolean;
    githubRepos: string[];
    mcpTools: Record<string, string[]>;
    tokens: Record<Kind, boolean>;
    gitlabRepos: string[];
  };
  type Account = { site: string; email: string };
  type McpServer = { name: string; status: string; tools: string[] };
  type TicketSummary = { key: string; url: string; title: string; state: string };
  type Test = { busy: boolean; error?: string; tickets?: TicketSummary[] };

  const DEFAULT_JQL = "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";
  const DEFAULT_WIQL =
    "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project AND [System.AssignedTo] = @me AND [System.State] NOT IN ('Closed', 'Done', 'Removed', 'Resolved') ORDER BY [System.ChangedDate] DESC";

  let { project }: { project: string } = $props();

  let trackers = $state<Trackers | null>(null);
  /** The sources as saved, so an unchanged form writes nothing (an MCP source keeps its last answer). */
  let saved = "";
  /** The Jira account `jiraToken` tells about, and the one the Forget button removes. */
  let tokenAccount = $state<Account | null>(null);
  let jiraToken = $state(false);
  let githubRepos = $state<string[]>([]);
  let mcpTools = $state<Record<string, string[]>>({});
  let error = $state("");
  let tests = $state<Record<string, Test>>({});
  let token = $state("");
  let tokenBusy = $state(false);
  let servers = $state<McpServer[] | null>(null);
  let serversBusy = $state(false);
  let adding = $state("");
  let gitlabRepos = $state<string[]>([]);
  /** Whether the keychain holds a token, and for which workspace, host or organization. */
  let tokens = $state<Record<Kind, { target: string | null; saved: boolean }>>({
    linear: { target: null, saved: false },
    gitlab: { target: null, saved: false },
    azure: { target: null, saved: false },
  });
  let secrets = $state<Record<Kind, string>>({ linear: "", gitlab: "", azure: "" });
  let secretBusy = $state<Kind | null>(null);

  /** The workspace, host or organization a token belongs to, as the form holds it. */
  const target = (kind: Kind) =>
    (kind === "linear"
      ? trackers?.linear?.workspace
      : kind === "gitlab"
        ? trackers?.gitlab?.host
        : trackers?.azure?.organization) ?? "";

  function load(sources: Sources) {
    trackers = sources.trackers;
    saved = JSON.stringify(sources.trackers);
    const jira = sources.trackers.jira;
    tokenAccount = jira && { site: jira.site, email: jira.email };
    jiraToken = sources.jiraToken;
    githubRepos = sources.githubRepos;
    mcpTools = sources.mcpTools;
    gitlabRepos = sources.gitlabRepos ?? [];
    for (const kind of ["linear", "gitlab", "azure"] as const) {
      tokens[kind] = { target: target(kind) || null, saved: !!sources.tokens?.[kind] };
    }
  }

  const hasToken = (kind: Kind) => tokens[kind].saved && tokens[kind].target === target(kind);

  /** Once the workspace, host or organization changed: is there a token for it? */
  async function checkSecret(kind: Kind) {
    const to = target(kind).trim();
    if (!to) return;
    try {
      tokens[kind] = { target: target(kind), saved: await invoke<boolean>("tickets_token", { kind, target: to }) };
    } catch (e) {
      error = String(e);
    }
  }

  async function saveSecret(kind: Kind) {
    const to = target(kind);
    secretBusy = kind;
    try {
      await invoke("tickets_token_save", { kind, target: to.trim(), token: secrets[kind] });
      secrets[kind] = "";
      tokens[kind] = { target: to, saved: true };
      error = "";
    } catch (e) {
      error = String(e);
    }
    secretBusy = null;
  }

  async function forgetSecret(kind: Kind) {
    const to = tokens[kind].target;
    if (!to) return;
    try {
      await invoke("tickets_token_forget", { kind, target: to.trim() });
      tokens[kind] = { target: to, saved: false };
    } catch (e) {
      error = String(e);
    }
  }

  function setGitlab(on: boolean) {
    trackers!.gitlab = on ? { host: "gitlab.com", repo: null, mine: true, comment: false } : null;
    if (on) checkSecret("gitlab");
  }

  function setLinear(on: boolean) {
    trackers!.linear = on
      ? { workspace: "", team: null, mine: true, states: [], comment: false, transition: null }
      : null;
  }

  function setAzure(on: boolean) {
    trackers!.azure = on ? { organization: "", project: "", wiql: null, comment: false, transition: null } : null;
  }

  // Mounted per project editor, like the form around it.
  // svelte-ignore state_referenced_locally
  invoke<Sources>("tickets_sources", { project }).then(load, (e) => {
    error = String(e);
  });

  /** Whether the form changed since the sources were read or saved. */
  export function dirty() {
    return !!trackers && JSON.stringify(trackers) !== saved;
  }

  /** Called by the project editor's Save; writes only what changed. */
  export async function save() {
    if (dirty()) {
      load(await invoke<Sources>("tickets_sources_save", { project, trackers }));
    }
  }

  const tokenFor = (jira: JiraTracker) =>
    jiraToken && tokenAccount?.site === jira.site && tokenAccount.email === jira.email;

  /** Once the site or the e-mail changed: is there a token for them? */
  async function checkToken() {
    const jira = trackers?.jira;
    if (!jira?.site.trim() || !jira.email.trim()) return;
    const account = { site: jira.site, email: jira.email };
    try {
      jiraToken = await invoke<boolean>("tickets_jira_token", account);
      tokenAccount = account;
    } catch (e) {
      error = String(e);
    }
  }

  function github(): GithubTracker {
    return (trackers!.github ??= {
      repo: null,
      comment: false,
      issues: false,
      project: null,
      columns: [],
      mine: true,
      transition: null,
    });
  }

  function setIssues(on: boolean) {
    github().issues = on;
    if (!on && trackers!.github!.project === null) trackers!.github = null;
  }

  function setBoard(on: boolean) {
    github().project = on ? "" : null;
    if (!on && !trackers!.github!.issues) trackers!.github = null;
  }

  function setJira(on: boolean) {
    trackers!.jira = on
      ? { site: "", email: "", jql: null, project: null, comment: false, transition: null }
      : null;
  }

  const list = (text: string) =>
    text
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);

  async function test(source: string) {
    tests[source] = { busy: true };
    try {
      const tickets = await invoke<TicketSummary[]>("tickets_source_test", {
        project,
        trackers,
        source,
      });
      tests[source] = { busy: false, tickets };
    } catch (e) {
      tests[source] = { busy: false, error: String(e) };
    }
  }

  async function saveToken() {
    const jira = trackers!.jira!;
    const account = { site: jira.site, email: jira.email };
    tokenBusy = true;
    try {
      await invoke("tickets_jira_token_save", { ...account, token });
      token = "";
      tokenAccount = account;
      jiraToken = true;
      error = "";
    } catch (e) {
      error = String(e);
    }
    tokenBusy = false;
  }

  async function forgetToken() {
    if (!tokenAccount) return;
    try {
      await invoke("tickets_jira_token_forget", { ...tokenAccount });
      jiraToken = false;
    } catch (e) {
      error = String(e);
    }
  }

  async function listServers() {
    serversBusy = true;
    try {
      servers = await invoke<McpServer[]>("tickets_mcp_servers", { project });
      adding = servers.find((s) => !Object.values(trackers!.mcp).some((m) => m.server === s.name))?.name ?? "";
    } catch (e) {
      error = String(e);
    }
    serversBusy = false;
  }

  function addMcp() {
    const server = servers?.find((s) => s.name === adding);
    if (!server) return;
    const base =
      server.name
        .toLowerCase()
        .replace(/^claude\.ai /, "")
        .replace(/[^a-z0-9_-]+/g, "-")
        .replace(/^-+|-+$/g, "") || "mcp";
    let name = base;
    for (let n = 2; name in trackers!.mcp; n++) name = `${base}-${n}`;
    trackers!.mcp[name] = { server: server.name, tools: [], prompt: null };
    mcpTools[name] = server.tools;
    servers = null;
  }

  function removeMcp(name: string) {
    delete trackers!.mcp[name];
    delete tests[`mcp:${name}`];
  }

  let connecting = $state(false);
  // An agent asked for the Jira assistant (`tickets.connect_jira`): it opens once the sources are read.
  $effect(() => {
    if (tickets.connectJira !== project || !trackers) return;
    tickets.connectJira = "";
    connecting = true;
  });

  /** What the Jira wizard connected, saved at once with the rest of the form. */
  async function connected(added: Connected) {
    const next = $state.snapshot(trackers)!;
    if ("jira" in added) next.jira = added.jira;
    else next.mcp = { ...next.mcp, [added.mcp.name]: added.mcp.tracker };
    load(await invoke<Sources>("tickets_sources_save", { project, trackers: next }));
    error = "";
    toast(t("tjdConnected"));
    void loadInbox();
  }
</script>

{#snippet result(source: string)}
  {@const run = tests[source]}
  <div class="test">
    <button type="button" class="btn sm" onclick={() => test(source)} disabled={run?.busy}>
      {#if run?.busy}<LoaderCircle size={13} aria-hidden="true" class="spin" />{/if}
      {t(run?.busy ? "ticketsTesting" : "ticketsTest")}
    </button>
  </div>
  {#if run?.error}
    <p class="notice error" role="alert">{run.error}</p>
  {:else if run?.tickets}
    <div class="found" role="status">
      <p class="ok">
        <CircleCheck size={13} aria-hidden="true" />
        {t(run.tickets.length ? "ticketsTestOk" : "ticketsTestEmpty")}
      </p>
      <ul>
        {#each run.tickets as ticket (ticket.url)}
          <li title={ticket.url}><span class="key">{ticket.key}</span> {ticket.title}</li>
        {/each}
      </ul>
    </div>
  {/if}
{/snippet}

{#snippet secret(kind: Kind, label: Key, help: Key)}
  <div class="token">
    <span class="label">{t(label)}</span>
    {#if hasToken(kind)}
      <p class="saved">
        <CircleCheck size={13} aria-hidden="true" />{t("ticketsTokenSaved")}
        <button type="button" class="btn sm quiet forget" onclick={() => forgetSecret(kind)}>
          {t("ticketsTokenForget")}
        </button>
      </p>
    {:else}
      <p class="help">{t(help)}</p>
      <div class="inline">
        <input
          class="field"
          type="password"
          bind:value={secrets[kind]}
          placeholder={t("ticketsTokenPlaceholder")}
          aria-label={t(label)}
          autocomplete="off"
        />
        <button
          type="button"
          class="btn sm"
          onclick={() => saveSecret(kind)}
          disabled={!secrets[kind].trim() || !target(kind).trim() || secretBusy === kind}
        >
          {t("ticketsTokenSave")}
        </button>
      </div>
    {/if}
  </div>
{/snippet}

<section class="sources" aria-labelledby="tickets-{project}">
  <h2 id="tickets-{project}">{t("ticketsTitle")}</h2>
  <p class="help">{t("ticketsHelp")}</p>

  {#if trackers}
    {@const gh = trackers.github}
    <div class="source">
      <label class="switch-row">
        <input
          type="checkbox"
          role="switch"
          class="switch"
          checked={!!gh?.issues}
          onchange={(e) => setIssues(e.currentTarget.checked)}
        />
        <span class="name">{t("ticketsGithubIssues")}</span>
      </label>
      {#if gh?.issues}
        <div class="fields">
          {#if trackers.implicit}
            <p class="implicit">{t("ticketsImplicit")}</p>
          {/if}
          <p class="help">{t("ticketsGithubIssuesHelp")}</p>
          <p class="help">
            {githubRepos.length
              ? t("ticketsGithubRepos", { repos: githubRepos.join(", ") })
              : t("ticketsGithubNoRepo")}
          </p>
          <label>
            <span>{t("ticketsIssuesRepo")}</span>
            <input
              class="field"
              bind:value={gh.repo}
              placeholder={t("ticketsIssuesRepoPlaceholder")}
              spellcheck="false"
            />
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={gh.comment} />
            {t("ticketsComment")}
          </label>
          {@render result("github")}
        </div>
      {/if}
    </div>

    <div class="source">
      <label class="switch-row">
        <input
          type="checkbox"
          role="switch"
          class="switch"
          checked={gh?.project != null}
          onchange={(e) => setBoard(e.currentTarget.checked)}
        />
        <span class="name">{t("ticketsGithubProject")}</span>
      </label>
      {#if gh && gh.project != null}
        <div class="fields">
          <label>
            <span>{t("ticketsBoard")}</span>
            <input
              class="field"
              bind:value={gh.project}
              placeholder={t("ticketsBoardPlaceholder")}
              spellcheck="false"
            />
          </label>
          <label>
            <span>{t("ticketsColumns")}</span>
            <input
              class="field"
              value={gh.columns.join(", ")}
              onchange={(e) => {
                gh.columns = list(e.currentTarget.value);
              }}
              placeholder={t("ticketsColumnsAll")}
            />
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={gh.mine} />
            {t("ticketsMine")}
          </label>
          <label>
            <span>{t("ticketsBoardMoveTo")}</span>
            <input class="field" bind:value={gh.transition} placeholder={t("ticketsLeaveAsIs")} />
          </label>
          {#if !gh.issues}
            <label class="check">
              <input type="checkbox" bind:checked={gh.comment} />
              {t("ticketsComment")}
            </label>
          {/if}
          {@render result("githubProject")}
        </div>
      {/if}
    </div>

    <div class="source">
      <label class="switch-row">
        <input
          type="checkbox"
          role="switch"
          class="switch"
          checked={!!trackers.gitlab}
          onchange={(e) => setGitlab(e.currentTarget.checked)}
        />
        <span class="name">{t("tickets2Gitlab")}</span>
      </label>
      {#if trackers.gitlab}
        {@const gl = trackers.gitlab}
        <div class="fields">
          <p class="help">{t("tickets2GitlabHelp")}</p>
          <p class="help">
            {gitlabRepos.length
              ? t("ticketsGithubRepos", { repos: gitlabRepos.join(", ") })
              : t("tickets2GitlabNoRepo")}
          </p>
          <label>
            <span>{t("tickets2GitlabHost")}</span>
            <input
              class="field"
              bind:value={gl.host}
              onchange={() => checkSecret("gitlab")}
              placeholder={t("tickets2GitlabHostPlaceholder")}
              spellcheck="false"
            />
          </label>
          <label>
            <span>{t("tickets2GitlabRepo")}</span>
            <input
              class="field"
              bind:value={gl.repo}
              placeholder={t("tickets2GitlabRepoPlaceholder")}
              spellcheck="false"
            />
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={gl.mine} />
            {t("ticketsMine")}
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={gl.comment} />
            {t("tickets2GitlabComment")}
          </label>
          {@render secret("gitlab", "tickets2GitlabToken", "tickets2GitlabTokenHelp")}
          {@render result("gitlab")}
        </div>
      {/if}
    </div>

    <div class="source">
      <label class="switch-row">
        <input
          type="checkbox"
          role="switch"
          class="switch"
          checked={!!trackers.jira}
          onchange={(e) => setJira(e.currentTarget.checked)}
        />
        <span class="name">{t("ticketsJira")}</span>
      </label>
      {#if !trackers.jira}
        <button type="button" class="btn sm primary connect" onclick={() => (connecting = true)}>
          <Plug size={14} aria-hidden="true" />{t("tjdConnect")}
        </button>
      {/if}
      {#if trackers.jira}
        {@const jira = trackers.jira}
        <div class="fields">
          <label>
            <span>{t("ticketsJiraSite")}</span>
            <input
              class="field"
              bind:value={jira.site}
              onchange={checkToken}
              placeholder={t("ticketsJiraSitePlaceholder")}
              spellcheck="false"
            />
          </label>
          <label>
            <span>{t("ticketsJiraEmail")}</span>
            <input class="field" type="email" bind:value={jira.email} onchange={checkToken} spellcheck="false" />
          </label>
          <div class="token">
            <span class="label">{t("ticketsToken")}</span>
            {#if tokenFor(jira)}
              <p class="saved">
                <CircleCheck size={13} aria-hidden="true" />{t("ticketsTokenSaved")}
                <button type="button" class="btn sm quiet forget" onclick={forgetToken}>
                  {t("ticketsTokenForget")}
                </button>
              </p>
            {:else}
              <p class="help">{t("ticketsTokenHelp")}</p>
              <div class="inline">
                <input
                  class="field"
                  type="password"
                  bind:value={token}
                  placeholder={t("ticketsTokenPlaceholder")}
                  aria-label={t("ticketsToken")}
                  autocomplete="off"
                />
                <button
                  type="button"
                  class="btn sm"
                  onclick={saveToken}
                  disabled={!token.trim() || !jira.site.trim() || !jira.email.trim() || tokenBusy}
                >
                  {t("ticketsTokenSave")}
                </button>
              </div>
            {/if}
          </div>
          <label>
            <span>{t("ticketsJql")}</span>
            <textarea class="field area" bind:value={jira.jql} placeholder={DEFAULT_JQL} rows="3" spellcheck="false"
            ></textarea>
          </label>
          <label>
            <span>{t("ticketsJiraKey")}</span>
            <input
              class="field"
              bind:value={jira.project}
              placeholder={t("ticketsJiraKeyPlaceholder")}
              spellcheck="false"
            />
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={jira.comment} />
            {t("ticketsJiraLink")}
          </label>
          <label>
            <span>{t("ticketsJiraMoveTo")}</span>
            <input class="field" bind:value={jira.transition} placeholder={t("ticketsLeaveAsIs")} />
          </label>
          {@render result("jira")}
        </div>
      {/if}
    </div>

    <div class="source">
      <label class="switch-row">
        <input
          type="checkbox"
          role="switch"
          class="switch"
          checked={!!trackers.linear}
          onchange={(e) => setLinear(e.currentTarget.checked)}
        />
        <span class="name">{t("tickets2Linear")}</span>
      </label>
      {#if trackers.linear}
        {@const lin = trackers.linear}
        <div class="fields">
          <label>
            <span>{t("tickets2LinearWorkspace")}</span>
            <input
              class="field"
              bind:value={lin.workspace}
              onchange={() => checkSecret("linear")}
              placeholder={t("tickets2LinearWorkspacePlaceholder")}
              spellcheck="false"
            />
          </label>
          {@render secret("linear", "tickets2LinearToken", "tickets2LinearTokenHelp")}
          <label>
            <span>{t("tickets2LinearTeam")}</span>
            <input class="field" bind:value={lin.team} placeholder={t("tickets2LinearTeamPlaceholder")} spellcheck="false" />
          </label>
          <label>
            <span>{t("tickets2LinearStates")}</span>
            <input
              class="field"
              value={lin.states.join(", ")}
              onchange={(e) => {
                lin.states = list(e.currentTarget.value);
              }}
              placeholder={t("tickets2LinearStatesAll")}
            />
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={lin.mine} />
            {t("ticketsMine")}
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={lin.comment} />
            {t("ticketsJiraLink")}
          </label>
          <label>
            <span>{t("ticketsJiraMoveTo")}</span>
            <input class="field" bind:value={lin.transition} placeholder={t("ticketsLeaveAsIs")} />
          </label>
          {@render result("linear")}
        </div>
      {/if}
    </div>

    <div class="source">
      <label class="switch-row">
        <input
          type="checkbox"
          role="switch"
          class="switch"
          checked={!!trackers.azure}
          onchange={(e) => setAzure(e.currentTarget.checked)}
        />
        <span class="name">{t("tickets2Azure")}</span>
      </label>
      {#if trackers.azure}
        {@const az = trackers.azure}
        <div class="fields">
          <label>
            <span>{t("tickets2AzureOrg")}</span>
            <input
              class="field"
              bind:value={az.organization}
              onchange={() => checkSecret("azure")}
              placeholder={t("tickets2AzureOrgPlaceholder")}
              spellcheck="false"
            />
          </label>
          <label>
            <span>{t("tickets2AzureProject")}</span>
            <input class="field" bind:value={az.project} spellcheck="false" />
          </label>
          {@render secret("azure", "tickets2AzureToken", "tickets2AzureTokenHelp")}
          <label>
            <span>{t("tickets2AzureWiql")}</span>
            <textarea class="field area" bind:value={az.wiql} placeholder={DEFAULT_WIQL} rows="4" spellcheck="false"
            ></textarea>
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={az.comment} />
            {t("ticketsJiraLink")}
          </label>
          <label>
            <span>{t("ticketsJiraMoveTo")}</span>
            <input class="field" bind:value={az.transition} placeholder={t("ticketsLeaveAsIs")} />
          </label>
          {@render result("azure")}
        </div>
      {/if}
    </div>

    <div class="source mcp">
      <p class="name">{t("ticketsMcp")}</p>
      <p class="help">{t("ticketsMcpHelp")}</p>
      {#each Object.entries(trackers.mcp) as [name, source] (name)}
        <div class="fields card">
          <div class="head">
            <span class="title" title={source.server}>{name} · {source.server}</span>
            <button
              type="button"
              class="icon-btn sm"
              onclick={() => removeMcp(name)}
              aria-label={t("ticketsMcpRemove", { name })}
              title={t("ticketsMcpRemove", { name })}
            >
              <X size={13} aria-hidden="true" />
            </button>
          </div>
          <label>
            <span>{t("ticketsMcpTools")}</span>
            <input
              class="field"
              value={source.tools.join(", ")}
              onchange={(e) => {
                source.tools = list(e.currentTarget.value);
              }}
              placeholder={mcpTools[name]?.length ? mcpTools[name].join(", ") : t("ticketsMcpToolsNone")}
              spellcheck="false"
            />
          </label>
          <p class="help">{t("ticketsMcpToolsHelp")}</p>
          <label>
            <span>{t("ticketsMcpPrompt")}</span>
            <input class="field" bind:value={source.prompt} placeholder={t("ticketsMcpPromptPlaceholder")} />
          </label>
          {@render result(`mcp:${name}`)}
        </div>
      {/each}
      {#if servers}
        {#if servers.length}
          <div class="inline">
            <select class="field" bind:value={adding} aria-label={t("ticketsMcpServer")}>
              {#each servers as server (server.name)}
                <option value={server.name}>{server.name} — {server.status}</option>
              {/each}
            </select>
            <button type="button" class="btn sm" onclick={addMcp} disabled={!adding}>{t("ticketsMcpAddOne")}</button>
          </div>
        {:else}
          <p class="help">{t("ticketsMcpNone")}</p>
        {/if}
      {:else}
        <button type="button" class="btn sm add" onclick={listServers} disabled={serversBusy}>
          {#if serversBusy}
            <LoaderCircle size={13} aria-hidden="true" class="spin" />{t("ticketsMcpLoading")}
          {:else}
            <Plus size={13} aria-hidden="true" />{t("ticketsMcpAdd")}
          {/if}
        </button>
      {/if}
    </div>
  {:else if !error}
    <Skeleton shape="list" rows={5} />
  {/if}

  {#if error}
    <p class="notice error" role="alert">{error}</p>
  {/if}
</section>

<JiraConnect bind:open={connecting} {project} taken={Object.keys(trackers?.mcp ?? {})} onconnected={connected} />

<style>
  .connect {
    align-self: flex-start;
    margin-top: 6px;
  }

  .sources {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 650;
  }

  .help {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.4;
  }

  .source {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .switch-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 24px;
    cursor: pointer;
  }

  .name {
    margin: 0;
    font-weight: 500;
  }

  .mcp {
    padding-top: 6px;
    border-top: 1px solid var(--border);
  }

  .fields {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 0 4px 12px;
    border-left: 2px solid var(--border);
  }

  .card {
    padding: 6px 0 6px 10px;
  }

  label:not(.switch-row, .check) {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  label:not(.switch-row, .check) > span,
  .label {
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
  }

  .field {
    height: 26px;
    font-size: var(--text-sm);
  }

  .area {
    height: auto;
    padding: 4px 8px;
    font: var(--text-xs) / 1.4 var(--font-mono);
    font-variant-ligatures: none;
    resize: vertical;
  }

  .check {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .check input {
    margin: 2px 0 0;
  }

  .token {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .inline {
    display: flex;
    gap: 4px;
  }

  .inline .field {
    min-width: 0;
  }

  .implicit {
    margin: 0;
    padding: 4px 8px;
    border-radius: var(--radius);
    background: var(--hover);
    font-size: var(--text-sm);
    line-height: 1.4;
  }

  .saved {
    display: flex;
    align-items: center;
    gap: 4px;
    margin: 0;
    color: var(--done);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .forget {
    margin-left: auto;
    padding: 0 4px;
    color: var(--muted);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .test {
    display: flex;
  }

  .add {
    align-self: flex-start;
  }

  .found {
    padding: 6px 8px;
    border-radius: var(--radius);
    background: var(--done-soft);
    font-size: var(--text-xs);
  }

  .ok {
    display: flex;
    align-items: center;
    gap: 4px;
    margin: 0 0 2px;
    color: var(--done);
    font-weight: 500;
  }

  .found ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .found li {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .key {
    font-family: var(--font-mono);
    font-weight: 500;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .sources :global(.spin) {
    animation: spin 0.9s linear infinite;
  }

  @media (prefers-reduced-motion: reduce) {
    .sources :global(.spin) {
      animation: none;
    }
  }
</style>
