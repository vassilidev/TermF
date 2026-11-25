<script lang="ts" module>
  export type JiraTracker = { site: string; email: string; jql: string | null; spaces: string[]; comment: boolean; transition: string | null };
  export type McpTracker = { server: string; tools: string[]; prompt: string | null };
  /** What the wizard connects: Jira sites through the API, or an MCP source on the user's Atlassian server. */
  export type Connected = { jira: JiraTracker[] } | { mcp: { name: string; tracker: McpTracker } };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Info from "@lucide/svelte/icons/info";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import Plus from "@lucide/svelte/icons/plus";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import Wizard, { type WizardStep } from "$lib/ui/Wizard.svelte";
  import { t } from "$lib/i18n";
  import { guessSite, mcpName, mcpPrompt, preChecked } from "$lib/tickets-logic";
  import { openUrl, type TrackerProject } from "$lib/tickets.svelte";

  /**
   * Jira connected in a few steps, as many sites and spaces as the project reads: the user's Atlassian server in
   * Claude Code when there is one (nothing to create, its sites and spaces read through it), else the sites TermF
   * finds in the repositories or the user types, one e-mail and API token (kept in the keychain for each site),
   * then each site's spaces, those the commits cite checked.
   */
  let {
    open = $bindable(false),
    project,
    configured,
    taken,
    onconnected,
  }: {
    open?: boolean;
    project: string;
    /** The project's Jira sites now: they show checked, with their spaces. */
    configured: JiraTracker[];
    /** The project's MCP sources' names. */
    taken: string[];
    onconnected: (added: Connected) => Promise<void>;
  } = $props();

  const TOKENS = "https://id.atlassian.com/manage-profile/security/api-tokens";

  type Hints = { keys: string[]; sites: string[]; email: string | null };
  type Server = { name: string; status: string; tools: string[] };
  type Site = { site: string; on: boolean; from: "configured" | "commits" | "email" | "typed" | "mcp" };
  type Spaces = TrackerProject[] | { error: string } | null;

  let dialog = $state<HTMLDialogElement>();
  let step = $state(0);
  let reached = $state(0);
  let how = $state<"mcp" | "token" | null>(null);
  let servers = $state<Server[] | null>(null);
  let hints = $state<Hints>({ keys: [], sites: [], email: null });
  let sites = $state<Site[]>([]);
  let typed = $state("");
  let email = $state("");
  let token = $state("");
  let hasToken = $state(false);
  let spaces = $state<Record<string, Spaces>>({});
  let checked = $state<Record<string, string[]>>({});
  let filter = $state("");
  let busy = $state(false);
  let error = $state("");

  const atlassian = $derived(servers?.find((s) => /atlassian/i.test(s.name)) ?? null);
  const chosen = $derived(sites.filter((s) => s.on).map((s) => s.site));
  const kinds = $derived(how === "mcp" ? (["how", "spaces"] as const) : (["how", "sites", "account", "spaces"] as const));
  const at = $derived(kinds[step] ?? "how");
  const count = $derived(Object.entries(checked).reduce((n, [site, keys]) => n + (chosen.includes(site) || how === "mcp" ? keys.length : 0), 0));
  const host = (site: string) => site.trim().replace(/^https?:\/\//, "").replace(/\/.*$/, "").toLowerCase();
  const validSite = (site: string) => /^[a-z0-9.-]+\.[a-z]{2,}(:\d+)?$/i.test(host(site));

  const from = { configured: "tjdFromConfigured", commits: "tjdFromCommits", email: "tjdFromEmail", mcp: "tjdFromMcp" } as const;
  const labels = { how: "tjdStepHow", sites: "tjdStepSites", account: "tjdStepAccount", spaces: "tjdStepSpaces" } as const;
  const steps: WizardStep[] = $derived(
    kinds.map((id) => ({
      id,
      label: t(labels[id]),
      answer:
        id === "how"
          ? t(how === "mcp" ? "tjdViaMcp" : "tjdViaToken")
          : id === "sites"
            ? chosen.join(", ")
            : id === "account"
              ? email
              : t("tjdChosen", { count }),
      live: id === at && busy ? t(at === "spaces" && how === "mcp" ? "tjdReading" : "tjdChecking", { server: atlassian?.name ?? "" }) : undefined,
    })),
  );

  const can = $derived(
    at === "how"
      ? how === "token" || (how === "mcp" && !!atlassian)
      : at === "sites"
        ? chosen.length > 0 && chosen.every(validSite)
        : at === "account"
          ? /^\S+@\S+\.\S+$/.test(email.trim()) && (hasToken || !!token.trim())
          : Object.values(spaces).some((s) => Array.isArray(s)),
  );

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) {
      dialog.showModal();
      start();
    }
    if (!open && dialog.open) dialog.close();
  });

  function addSite(site: string, from: Site["from"], on = true) {
    const h = host(site);
    if (!h || sites.some((s) => s.site === h)) return;
    sites = [...sites, { site: h, on, from }];
  }

  function start() {
    [step, reached, how, servers, spaces, checked, token, error, filter, typed] = [0, 0, null, null, {}, {}, "", "", "", ""];
    sites = [];
    for (const c of configured) {
      addSite(c.site, "configured");
      checked[host(c.site)] = [...c.spaces];
    }
    email = configured[0]?.email ?? "";
    invoke<Hints>("tickets_jira_hints", { project }).then(
      (h) => {
        hints = h;
        for (const s of h.sites) addSite(s, "commits");
        const guess = guessSite(h);
        if (!sites.length && guess.site) addSite(guess.site, "email");
        email ||= h.email ?? "";
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
    if (kinds[step] === "account") void checkToken();
    if (kinds[step] === "spaces" && how === "mcp" && !Object.keys(spaces).length) void readMcp();
  }

  /** Another way in reads the spaces again: the server's and the API's may differ. */
  function choose(to: "mcp" | "token") {
    if (how !== to) [how, spaces, reached] = [to, {}, step];
  }

  /** Whether the keychain holds a token for every chosen site with this e-mail: then none is typed. */
  async function checkToken() {
    try {
      const has = await Promise.all(chosen.map((site) => invoke<boolean>("tickets_jira_token", { site, email: email.trim() })));
      hasToken = has.length > 0 && has.every(Boolean);
    } catch {
      hasToken = false;
    }
  }

  async function readMcp() {
    if (!atlassian) return;
    busy = true;
    try {
      const got = await invoke<{ sites: { site: string; spaces: TrackerProject[] }[] }>("tickets_mcp_projects", { project, server: atlassian.name });
      sites = got.sites.map((s) => ({ site: s.site, on: true, from: "mcp" as const }));
      spaces = Object.fromEntries(got.sites.map((s) => [s.site, s.spaces]));
      checked = Object.fromEntries(got.sites.map((s) => [s.site, checked[s.site] ?? preChecked(s.spaces, hints.keys)]));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  /** The token is checked by reading each site's spaces with it; then only is it kept, for each site. */
  async function readSites() {
    busy = true;
    const typedToken = token.trim() || null;
    const read = await Promise.all(
      chosen.map(async (site) => {
        try {
          const list = await invoke<TrackerProject[]>("tickets_jira_projects", { site, email: email.trim(), token: typedToken });
          if (typedToken) await invoke("tickets_jira_token_save", { site, email: email.trim(), token: typedToken });
          return [site, list] as const;
        } catch (e) {
          return [site, { error: String(e) }] as const;
        }
      }),
    );
    spaces = Object.fromEntries(read);
    for (const [site, list] of read) if (Array.isArray(list)) checked[site] ??= preChecked(list, hints.keys);
    busy = false;
    if (read.every(([, list]) => !Array.isArray(list))) error = (read[0]?.[1] as { error: string })?.error ?? "";
    else {
      [token, hasToken] = ["", true];
      go(step + 1);
    }
  }

  async function connect() {
    busy = true;
    try {
      const read = Object.keys(spaces).filter((site) => Array.isArray(spaces[site]));
      const keys = (site: string) => (checked[site] ?? []).filter((k) => (spaces[site] as TrackerProject[]).some((p) => p.key === k));
      await onconnected(
        how === "mcp" && atlassian
          ? {
              mcp: {
                name: mcpName(atlassian.name, taken),
                tracker: { server: atlassian.name, tools: [], prompt: mcpPrompt(read.filter((s) => chosen.includes(s)).map((site) => ({ site, keys: keys(site) }))) },
              },
            }
          : {
              jira: read.map((site) => {
                const was = configured.find((c) => host(c.site) === site);
                return { site, email: email.trim(), jql: was?.jql ?? null, spaces: keys(site), comment: was?.comment ?? false, transition: was?.transition ?? null };
              }),
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
    if (at === "account") return void readSites();
    if (at === "spaces") return void connect();
    go(step + 1);
  }

  function toggle(site: string, key: string, on: boolean) {
    const now = checked[site] ?? [];
    checked[site] = on ? [...now, key] : now.filter((k) => k !== key);
  }

  const shown = (list: TrackerProject[]) =>
    list.filter((p) => !filter.trim() || `${p.key} ${p.name}`.toLowerCase().includes(filter.trim().toLowerCase()));
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
      continueLabel={at === "spaces" ? t("tjdConnectGo") : undefined}
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
      {:else if at === "sites"}
        <h2>{t("tjdSitesTitle")}</h2>
        <p class="muted">{t("tjdSitesHelp")}</p>
        <ul class="list">
          {#each sites as s (s.site)}
            <li>
              <label>
                <input type="checkbox" bind:checked={s.on} />
                <span class="mono name">{s.site}</span>
                {#if s.from !== "typed"}<span class="badge">{t(from[s.from])}</span>{/if}
              </label>
            </li>
          {/each}
        </ul>
        <form
          class="add"
          onsubmit={(e) => {
            e.preventDefault();
            if (!validSite(typed)) return;
            addSite(typed, "typed");
            typed = "";
          }}
        >
          <input class="field mono" bind:value={typed} placeholder="acme.atlassian.net" spellcheck="false" aria-label={t("tjdSiteAdd")} />
          <button class="btn sm" type="submit" disabled={!validSite(typed)}><Plus size={14} aria-hidden="true" />{t("tjdSiteAdd")}</button>
        </form>
      {:else if at === "account"}
        <h2>{t("tjdAccountTitle")}</h2>
        <label class="labelled">
          <span>{t("tjdEmailLabel")}</span>
          <input class="field big" type="email" bind:value={email} onchange={checkToken} spellcheck="false" />
        </label>
        {#if hints.email && email === hints.email}<p class="muted small">{t("tjdEmailFromGit")}</p>{/if}
        {#if hasToken}
          <p class="saved"><CircleCheck size={14} aria-hidden="true" />{t("tjdTokenSaved", { email: email.trim(), site: chosen.join(", ") })}</p>
        {:else}
          <p class="muted">{t("tjdTokenHelp")}</p>
          <button class="btn" onclick={() => openUrl(TOKENS)}><ExternalLink size={14} aria-hidden="true" />{t("tjdTokenCreate")}</button>
          <label class="labelled">
            <span>{t("tjdTokenLabel")}</span>
            <input class="field big mono" type="password" autocomplete="off" bind:value={token} />
          </label>
        {/if}
      {:else}
        <h2>{t("tjdStepSpaces")}</h2>
        {#if !Object.keys(spaces).length}
          <Skeleton shape="list" rows={5} delay={0} label={t(how === "mcp" ? "tjdReading" : "tjdChecking", { server: atlassian?.name ?? "" })} />
        {:else}
          <p class="muted">{t("tjdProjectsHelp")}</p>
          {#if Object.values(spaces).reduce((n, s) => n + (Array.isArray(s) ? s.length : 0), 0) > 8}
            <input class="field" type="search" bind:value={filter} placeholder={t("tjdFilter")} aria-label={t("tjdFilter")} />
          {/if}
          {#each Object.entries(spaces) as [site, list] (site)}
            <section class="site" aria-label={site}>
              <h3 class="mono">{site}</h3>
              {#if !list}
                <Skeleton shape="list" rows={2} delay={0} />
              {:else if !Array.isArray(list)}
                <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{list.error}</p>
              {:else if !list.length}
                <p class="muted small">{t("tjdNoProjects")}</p>
              {:else}
                <ul class="list">
                  {#each shown(list) as p (p.key)}
                    <li>
                      <label>
                        <input type="checkbox" checked={checked[site]?.includes(p.key)} onchange={(e) => toggle(site, p.key, e.currentTarget.checked)} />
                        <span class="mono key">{p.key}</span>
                        <span class="name">{p.name}</span>
                        {#if hints.keys.includes(p.key)}<span class="badge">{t("tjdCited")}</span>{/if}
                      </label>
                    </li>
                  {/each}
                </ul>
                {#if !checked[site]?.length}<p class="muted small">{t("tjdProjectsNone")}</p>{/if}
              {/if}
            </section>
          {/each}
        {/if}
      {/if}
      {#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
    </Wizard>
  {/if}
</dialog>

<style>
  .jira {
    width: min(920px, 96vw);
    height: min(640px, 92vh);
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

  h3 {
    margin: 14px 0 4px;
    font-size: var(--text-sm);
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
    white-space: nowrap;
  }

  .big {
    width: 100%;
    max-width: 420px;
  }

  .labelled {
    display: flex;
    flex-direction: column;
    gap: 4px;
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

  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 6px 0;
    padding: 0;
    list-style: none;
  }

  .list label {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .list label:hover {
    background: var(--hover);
  }

  .add {
    display: flex;
    gap: 8px;
    max-width: 480px;
    margin-top: 8px;
  }

  .add input {
    flex: 1;
  }

  .key {
    min-width: 56px;
    font-weight: 600;
  }

  .name {
    flex: 1;
  }

  .mono {
    font-family: var(--font-mono);
  }
</style>
