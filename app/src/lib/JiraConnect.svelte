<script lang="ts" module>
  export type JiraTracker = { site: string; email: string; jql: string | null; project: string | null; comment: boolean; transition: string | null };
  export type McpTracker = { server: string; tools: string[]; prompt: string | null };
  /** What the wizard connects: Jira through its API, or an MCP source on the user's Atlassian server. */
  export type Connected = { jira: JiraTracker } | { mcp: { name: string; tracker: McpTracker } };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Info from "@lucide/svelte/icons/info";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import Wizard, { type WizardStep } from "$lib/ui/Wizard.svelte";
  import { t } from "$lib/i18n";
  import { guessSite, inboxJql, mcpName, mcpPrompt, preChecked } from "$lib/tickets-logic";
  import { openUrl, type TrackerProject } from "$lib/tickets.svelte";

  /**
   * Jira connected in a few steps: the user's Atlassian server in Claude Code when there is one (nothing to create),
   * else the site and e-mail TermF guesses from the repositories, an API token kept in the keychain, then the
   * projects the API lists, those the commits cite checked.
   */
  let {
    open = $bindable(false),
    project,
    taken,
    onconnected,
  }: {
    open?: boolean;
    project: string;
    /** The project's MCP sources' names. */
    taken: string[];
    onconnected: (added: Connected) => Promise<void>;
  } = $props();

  const TOKENS = "https://id.atlassian.com/manage-profile/security/api-tokens";

  type Hints = { keys: string[]; sites: string[]; email: string | null };
  type Server = { name: string; status: string; tools: string[] };

  let dialog = $state<HTMLDialogElement>();
  let step = $state(0);
  let reached = $state(0);
  let how = $state<"mcp" | "token" | null>(null);
  let servers = $state<Server[] | null>(null);
  let hints = $state<Hints>({ keys: [], sites: [], email: null });
  let site = $state("");
  let siteFrom = $state<"commits" | "email" | null>(null);
  let email = $state("");
  let token = $state("");
  let hasToken = $state(false);
  let projects = $state<TrackerProject[] | null>(null);
  let checked = $state<string[]>([]);
  let mcpSite = $state("");
  let filter = $state("");
  let busy = $state(false);
  let error = $state("");

  const atlassian = $derived(servers?.find((s) => /atlassian/i.test(s.name)) ?? null);
  const found = $derived((projects ?? []).filter((p) => !filter.trim() || `${p.key} ${p.name}`.toLowerCase().includes(filter.trim().toLowerCase())));
  const kinds = $derived(how === "mcp" ? (["how", "projects"] as const) : (["how", "site", "email", "token", "projects"] as const));
  const at = $derived(kinds[step] ?? "how");

  const labels = { how: "tjdStepHow", site: "tjdStepSite", email: "tjdStepEmail", token: "tjdStepToken", projects: "tjdStepProjects" } as const;
  const steps: WizardStep[] = $derived(
    kinds.map((id) => ({
      id,
      label: t(labels[id]),
      answer:
        id === "how"
          ? t(how === "mcp" ? "tjdViaMcp" : "tjdViaToken")
          : id === "site"
            ? site
            : id === "email"
              ? email
              : id === "token"
                ? t("tjdTokenKept")
                : t("tjdChosen", { count: checked.length }),
      live: id === at && busy ? t(at === "projects" && how === "mcp" ? "tjdReading" : "tjdChecking", { server: atlassian?.name ?? "" }) : undefined,
    })),
  );

  const can = $derived(
    at === "how"
      ? how === "token" || (how === "mcp" && !!atlassian)
      : at === "site"
        ? /^[a-z0-9.-]+\.[a-z]{2,}(:\d+)?$/i.test(site.trim().replace(/^https?:\/\//, "").replace(/\/.*$/, ""))
        : at === "email"
          ? /^\S+@\S+\.\S+$/.test(email.trim())
          : at === "token"
            ? hasToken || !!token.trim()
            : !!projects,
  );

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) {
      dialog.showModal();
      start();
    }
    if (!open && dialog.open) dialog.close();
  });

  function start() {
    [step, reached, how, servers, projects, checked, token, error, filter] = [0, 0, null, null, null, [], "", "", ""];
    invoke<Hints>("tickets_jira_hints", { project }).then(
      (h) => {
        hints = h;
        const guess = guessSite(h);
        [site, siteFrom] = [guess.site, guess.from];
        email = h.email ?? "";
      },
      () => {},
    );
    invoke<Server[]>("tickets_mcp_servers", { project }).then(
      (list) => {
        servers = list;
        how ??= list.some((s) => /atlassian/i.test(s.name)) ? "mcp" : "token";
      },
      () => {
        servers = [];
        how ??= "token";
      },
    );
  }

  function go(to: number) {
    error = "";
    step = Math.max(0, Math.min(to, kinds.length - 1));
    reached = Math.max(reached, step);
    if (kinds[step] === "token") checkToken();
    if (kinds[step] === "projects" && how === "mcp" && !projects) void readMcp();
  }

  async function checkToken() {
    try {
      hasToken = await invoke<boolean>("tickets_jira_token", { site: site.trim(), email: email.trim() });
    } catch {
      hasToken = false;
    }
  }

  async function readMcp() {
    if (!atlassian) return;
    busy = true;
    try {
      const got = await invoke<{ site: string; projects: TrackerProject[] }>("tickets_mcp_projects", { project, server: atlassian.name });
      [projects, mcpSite, checked] = [got.projects, got.site, preChecked(got.projects, hints.keys)];
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  /** The token is checked by reading the projects with it; then only is it kept. */
  async function readJira() {
    busy = true;
    try {
      const typed = token.trim() || null;
      const list = await invoke<TrackerProject[]>("tickets_jira_projects", { site: site.trim(), email: email.trim(), token: typed });
      if (typed) await invoke("tickets_jira_token_save", { site: site.trim(), email: email.trim(), token: typed });
      [projects, checked, token, hasToken] = [list, preChecked(list, hints.keys), "", true];
      go(step + 1);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function connect() {
    busy = true;
    try {
      const keys = (projects ?? []).filter((p) => checked.includes(p.key)).map((p) => p.key);
      await onconnected(
        how === "mcp" && atlassian
          ? { mcp: { name: mcpName(atlassian.name, taken), tracker: { server: atlassian.name, tools: [], prompt: mcpPrompt(keys, mcpSite) } } }
          : {
              jira: {
                site: site.trim(),
                email: email.trim(),
                jql: inboxJql(keys),
                project: keys.join(", ") || null,
                comment: false,
                transition: null,
              },
            },
      );
      open = false;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function next() {
    if (at === "token") return void readJira();
    if (at === "projects") return void connect();
    go(step + 1);
  }

  /** Another way in reads the projects again: the server's and the API's may differ. */
  function choose(to: "mcp" | "token") {
    if (how !== to) [how, projects, checked, reached] = [to, null, [], step];
  }

  function toggle(key: string, on: boolean) {
    checked = on ? [...checked, key] : checked.filter((k) => k !== key);
  }
</script>

<dialog bind:this={dialog} class="jira" aria-label={t("tjdTitle")} onclose={() => (open = false)}>
  {#if open}
    <Wizard
      title={t("tjdTitle")}
      {steps}
      current={step}
      {reached}
      onstep={go}
      onback={() => go(step - 1)}
      oncontinue={next}
      canContinue={can && !busy}
      {busy}
      continueLabel={at === "projects" ? t("tjdConnectGo") : undefined}
    >
      {#snippet head()}
        <p class="title">{t("tjdTitle")}</p>
      {/snippet}
      {#snippet skip()}
        <button class="btn quiet" onclick={() => (open = false)}>{t("cancel")}</button>
      {/snippet}

      {#if at === "how"}
        <h2>{t("tjdHowTitle")}</h2>
        <div class="choices" role="radiogroup" aria-label={t("tjdHowTitle")}>
          {#if servers === null}
            <div class="choice wait" aria-busy="true">
              <Skeleton shape="lines" rows={2} delay={0} />
              <span class="muted small">{t("tjdLooking")}</span>
            </div>
          {:else if atlassian}
            <button class="choice" role="radio" aria-checked={how === "mcp"} class:on={how === "mcp"} onclick={() => choose("mcp")}>
              <span class="head"><Sparkles size={16} aria-hidden="true" />{t("tjdUseMcp")}<span class="badge">{t("tjdRecommended")}</span></span>
              <span class="muted">{t("tjdUseMcpHelp", { server: atlassian.name })}</span>
            </button>
          {/if}
          <button class="choice" role="radio" aria-checked={how === "token"} class:on={how === "token"} onclick={() => choose("token")}>
            <span class="head"><KeyRound size={16} aria-hidden="true" />{t("tjdUseToken")}</span>
            <span class="muted">{t("tjdUseTokenHelp")}</span>
          </button>
        </div>
        {#if servers && !atlassian}
          <p class="notice"><Info size={14} aria-hidden="true" />{t("tjdNoMcp")}</p>
        {/if}
      {:else if at === "site"}
        <h2>{t("tjdSiteLabel")}</h2>
        <input class="field big mono" bind:value={site} placeholder="acme.atlassian.net" spellcheck="false" aria-label={t("tjdSiteLabel")} />
        <p class="muted small">
          {siteFrom === "commits" ? t("tjdSiteFromCommits") : siteFrom === "email" && site ? t("tjdSiteFromEmail") : t("tjdSiteHelp")}
        </p>
      {:else if at === "email"}
        <h2>{t("tjdEmailLabel")}</h2>
        <input class="field big" type="email" bind:value={email} spellcheck="false" aria-label={t("tjdEmailLabel")} />
        {#if hints.email && email === hints.email}<p class="muted small">{t("tjdEmailFromGit")}</p>{/if}
      {:else if at === "token"}
        <h2>{t("tjdTokenLabel")}</h2>
        {#if hasToken}
          <p class="saved"><CircleCheck size={14} aria-hidden="true" />{t("tjdTokenSaved", { email: email.trim(), site: site.trim() })}</p>
        {:else}
          <p class="muted">{t("tjdTokenHelp")}</p>
          <button class="btn" onclick={() => openUrl(TOKENS)}><ExternalLink size={14} aria-hidden="true" />{t("tjdTokenCreate")}</button>
          <input class="field big mono" type="password" autocomplete="off" bind:value={token} aria-label={t("tjdTokenLabel")} />
        {/if}
      {:else}
        <h2>{t("tjdStepProjects")}</h2>
        {#if projects === null}
          <Skeleton shape="list" rows={5} delay={0} label={t(how === "mcp" ? "tjdReading" : "tjdChecking", { server: atlassian?.name ?? "" })} />
        {:else if !projects.length}
          <p class="notice"><Info size={14} aria-hidden="true" />{t("tjdNoProjects")}</p>
        {:else}
          <p class="muted">{t("tjdProjectsHelp")}</p>
          {#if projects.length > 8}
            <input class="field" type="search" bind:value={filter} placeholder={t("tjdFilter")} aria-label={t("tjdFilter")} />
          {/if}
          <ul class="projects">
            {#each found as p (p.key)}
              <li>
                <label>
                  <input type="checkbox" checked={checked.includes(p.key)} onchange={(e) => toggle(p.key, e.currentTarget.checked)} />
                  <span class="mono key">{p.key}</span>
                  <span class="pname">{p.name}</span>
                  {#if hints.keys.includes(p.key)}<span class="badge">{t("tjdCited")}</span>{/if}
                </label>
              </li>
            {/each}
          </ul>
          {#if !checked.length}<p class="muted small">{t("tjdProjectsNone")}</p>{/if}
        {/if}
      {/if}
      {#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
    </Wizard>
  {/if}
</dialog>

<style>
  .jira {
    width: min(920px, 96vw);
    height: min(620px, 92vh);
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg, 12px);
    background: var(--panel);
    color: inherit;
  }

  .jira::backdrop {
    background: rgb(0 0 0 / 0.4);
  }

  .title {
    margin: 0 0 4px 10px;
    font-weight: 600;
  }

  h2 {
    margin: 0 0 12px;
    font-size: var(--text-lg);
  }

  .choices {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 12px;
  }

  .choice {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius, 8px);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .choice.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .choice.wait {
    cursor: default;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
  }

  .badge {
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
  }

  .big {
    width: 100%;
    max-width: 420px;
    margin: 8px 0;
  }

  .small {
    font-size: var(--text-sm);
  }

  .saved {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .projects {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 8px 0;
    padding: 0;
    list-style: none;
  }

  .projects label {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .projects label:hover {
    background: var(--hover);
  }

  .key {
    min-width: 56px;
    font-weight: 600;
  }

  .pname {
    flex: 1;
  }

  .mono {
    font-family: var(--font-mono);
  }
</style>
