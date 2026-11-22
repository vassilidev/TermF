<script lang="ts">
  import { untrack, type Component, type Snippet } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import Check from "@lucide/svelte/icons/check";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CircleQuestionMark from "@lucide/svelte/icons/circle-question-mark";
  import Eye from "@lucide/svelte/icons/eye";
  import ListChecks from "@lucide/svelte/icons/list-checks";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Plus from "@lucide/svelte/icons/plus";
  import Route from "@lucide/svelte/icons/route";
  import Send from "@lucide/svelte/icons/send";
  import ServerCog from "@lucide/svelte/icons/server-cog";
  import ShieldAlert from "@lucide/svelte/icons/shield-alert";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Users from "@lucide/svelte/icons/users";
  import WandSparkles from "@lucide/svelte/icons/wand-sparkles";
  import Conversation from "$lib/Conversation.svelte";
  import { initial } from "$lib/ProjectSwitcher.svelte";
  import { ticketKey } from "$lib/TeamRunDetail.svelte";
  import OpsRequest from "$lib/OpsRequest.svelte";
  import WaitCards from "$lib/WaitCards.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import StatusPill, { type Status } from "$lib/ui/StatusPill.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { agentProjects, agents, ctoOf, expertOf, showConversations, showConversation } from "$lib/agents.svelte";
  import { focusSession, keys, showRun } from "$lib/commands";
  import { pinned, summary, type Item } from "$lib/conversation-logic";
  import { openRuns, place, primary, type Wait } from "$lib/home-logic";
  import { decisions as decisionsOf, ensure, goToWait, home, learn, projectWaits, waitProject, type Decision } from "$lib/home.svelte";
  import { ago, t } from "$lib/i18n";
  import { openSetup, setup } from "$lib/onboarding.svelte";
  import { ops } from "$lib/ops.svelte";
  import type { Step } from "$lib/onboarding-logic";
  import { editProject, sessionProjects, tallies } from "$lib/project.svelte";
  import { awaitsDelivery, shown } from "$lib/run-logic";
  import { app, onScreen, repoName, setProject, shortPath } from "$lib/state.svelte";
  import { initials } from "$lib/team-logic";
  import { TRIAGE, asks, current, progress, type AskKind, type TeamRun } from "$lib/team-run-logic";
  import { team, upsertRun, waitTitle } from "$lib/team.svelte";
  import { loadInbox, openUrl, showTickets, tickets, upsert, type Resolution, type Run, type TicketSummary } from "$lib/tickets.svelte";

  type Notice = Extract<Item, { type: "notice" }>;
  type Onboarding = {
    step: Step;
    analysis: { repos: { name: string; stack: string[]; workflows: string[] }[]; questions: unknown[] } | null;
  };

  const project = $derived(app.workspace?.projects.find((p) => p.id === app.project));
  const visible = $derived(onScreen("home"));
  const cto = $derived(project ? ctoOf(project.id) : undefined);
  const session = $derived(project ? (home.with === "cto" ? cto : expertOf(project.id)) : undefined);

  let error = $state("");

  // Opening the Home opens its conversation, as the palette does: created without a process, the first message starts it.
  $effect(() => {
    const id = project?.id;
    const who = home.with;
    if (visible && id) untrack(() => ensure(id, who));
  });

  const waits = $derived(projectWaits(app.project));

  $effect(() => {
    if (!visible) return;
    for (const w of waits) {
      if (w.kind !== "agent" && w.kind !== "cto") continue;
      const agent = agents.sessions.find((a) => a.id === w.id);
      if (agent) untrack(() => learn(agent));
    }
  });

  // The cards of the CTO's thread: each step of the project's runs that waits for the user.
  let notices = $state<Notice[]>([]);
  // A delivery's card stays once made, to say so with the pull request, until it has nothing left to show: hidden, or
  // the delivery made from the Run page.
  let delivering = $state<Record<string, Decision>>({});
  $effect(() => {
    void project?.id;
    untrack(() => (delivering = {}));
  });
  const open = $derived(project ? decisionsOf(project.id) : []);
  $effect(() => {
    for (const d of open) if (!d.ask || d.ask.kind === "deliver") untrack(() => (delivering[d.key] = d));
  });
  const finished = $derived(Object.values(delivering).filter((d) => !open.some((o) => o.key === d.key)));
  const decisions = $derived([...open, ...finished.map((d) => ({ ...d, run: team.runs.find((r) => r.run === d.run.run) ?? d.run }))]);
  const placed = $derived(place(notices, decisions.map((d) => ({ run: d.run.run, step: d.step }))));
  const main = $derived(primary(placed, cto?.state === "waiting", finished.map((d) => d.key)));
  const decision = (key: string | undefined) => decisions.find((d) => d.key === key);

  // A new project's setup, until it is done.
  let onboarding = $state<Record<string, Onboarding>>({});
  $effect(() => {
    const id = project?.id;
    void setup.project;
    if (!visible || !id) return;
    invoke<Onboarding>("onboarding_state", { project: id }).then(
      (state) => {
        onboarding[id] = state;
      },
      () => {},
    );
  });
  const setupSteps: Step[] = ["analysis", "questions", "proposals", "trust"];
  const stepLabels = { analysis: "obStepAnalysis", questions: "obStepQuestions", proposals: "obStepProposals", trust: "obStepTrust" } as const;
  const stepActions = { analysis: "obAnalyze", questions: "teamRunsAnswer", proposals: "obContinue", trust: "obContinue" } as const;

  function stepText(step: Step, ob: Onboarding): string {
    const a = ob.analysis;
    if (step === "analysis") {
      if (!a) return "";
      const repos = a.repos.map((r) => [r.name, r.stack.join(", ")].filter(Boolean).join(" · "));
      const ci = [...new Set(a.repos.flatMap((r) => r.workflows))];
      return [...repos, ci.length ? t("ux3t1Ci", { list: ci.join(", ") }) : ""].filter(Boolean).join(" · ");
    }
    if (step === "questions") {
      if (!a) return "";
      return a.questions.length ? t("ux3t1SetupQuestions", { count: a.questions.length }) : t("obNoQuestions");
    }
    return step === "proposals" ? t("ux3t1SetupProposals") : t("ux3t1SetupTrust");
  }

  // The rail's inbox follows the project, until the Tickets view is there to keep it.
  // A failed load is tried again on the next visit.
  let inboxFor: string | undefined;
  let inboxAsked: string | undefined;
  $effect(() => {
    const scope = tickets.scope;
    if (!visible || tickets.loaded || inboxFor === scope || inboxAsked === scope) return;
    inboxAsked = scope;
    untrack(loadInbox).then(() => {
      if (inboxAsked !== scope) return;
      if (!tickets.inboxError) inboxFor = scope;
      inboxAsked = undefined;
    });
  });
  const inbox = $derived(tickets.inbox ?? []);

  /** "With the team" from the rail: the project's workflow and budget, the CTO routes it. */
  let starting = $state("");
  let choosing = $state<{ url: string; resolution: Resolution; repo: string } | null>(null);

  async function withTeam(ticket: TicketSummary) {
    error = "";
    starting = ticket.url;
    let run: Run | null = null;
    try {
      const chosen = choosing?.url === ticket.url ? choosing : null;
      const resolution = chosen?.resolution ?? (await invoke<Resolution>("tickets_resolve", { input: ticket.url, project: ticket.project }));
      const repo = chosen?.repo || resolution.repo;
      if (!repo) {
        choosing = { url: ticket.url, resolution, repo: resolution.repos[0]?.path ?? "" };
        return;
      }
      run = await invoke<Run>("tickets_run", { input: resolution.ticket.url, project: resolution.project, repo, session: false });
      upsert(run);
      upsertRun(await invoke<TeamRun>("team_start", { run: run.id, options: {} }));
      choosing = null;
    } catch (e) {
      error = String(e);
      // The team did not start: its new run goes, as from the inbox.
      if (run) await invoke<Run>("tickets_close", { id: run.id, force: false }).then(upsert, () => {});
    } finally {
      starting = "";
    }
  }

  // Agents of the project at work: tiles, and headless sessions apart from the standing conversations.
  const tiles = $derived(project ? app.sessions.filter((s) => s.running && s.agent !== null && sessionProjects(s).includes(project.id)) : []);
  const headless = $derived(
    project
      ? agents.sessions.filter(
          (a) => (a.state === "working" || a.state === "waiting" || a.state === "idle") && !pinned(a) && agentProjects(a).includes(project.id),
        )
      : [],
  );

  const runs = $derived(openRuns(tickets.runs, team.runs, app.project));

  function showAll(tab: "inbox" | "runs") {
    tickets.tab = tab;
    showTickets();
  }

  const actions: Record<string, Parameters<typeof t>[0]> = {
    mcp__termf__start_team_run: "ctoToolStart",
    mcp__termf__answer: "ctoToolAnswer",
    mcp__termf__approve: "ctoToolApprove",
    mcp__termf__request_changes: "ctoToolChanges",
    mcp__termf__stop_team_run: "ctoToolStop",
    mcp__termf__send_message: "ctoToolSend",
  };

  /** A wait as the rail lists it: its icon, what it asks, and where. */
  function row(w: Wait): { icon: Component; title: string; context: string } {
    if (w.kind === "tile") {
      const s = app.sessions.find((x) => x.id === w.id);
      return { icon: SquareTerminal, title: t("ux3t1RoleWaits", { role: s?.role ?? s?.title ?? "" }), context: s ? shortPath(s.cwd) : "" };
    }
    if (w.kind === "agent" || w.kind === "cto") {
      const a = agents.sessions.find((x) => x.id === w.id);
      const asked = home.asked[w.id];
      const what = asked ? asked.description || summary(asked.input) || asked.tool : "";
      if (w.kind === "cto") {
        const action = asked ? (actions[asked.tool] ? t(actions[asked.tool]) : asked.tool) : t("statusWaiting");
        return { icon: ShieldCheck, title: action, context: [t("ux3t1Cto"), asked ? summary(asked.input) : ""].filter(Boolean).join(" · ") };
      }
      return {
        icon: BellRing,
        title: t("ux3t1RoleWaits", { role: a?.role ?? a?.title ?? "" }),
        context: [what, a ? repoName(a.cwd) : ""].filter(Boolean).join(" · "),
      };
    }
    if (w.kind === "ops") {
      const r = ops.requests.find((x) => x.id === w.id);
      if (!r) return { icon: ShieldAlert, title: "", context: "" };
      const title = t(r.kind === "approve" ? "opsWaitTitle" : "opsWaitPropose", { recipe: r.plan.recipe });
      return { icon: r.kind === "approve" ? ShieldAlert : ServerCog, title, context: `${r.plan.connection} · ${r.plan.env}` };
    }
    const d = "run" in w ? decisionsOf(null).find((x) => x.run.run === w.run && x.step === w.step) : undefined;
    if (!d) return { icon: BellRing, title: "", context: "" };
    const title = !d.ask
      ? t("teamRunsAskDeliver")
      : d.ask.kind === "questions" && d.step === TRIAGE
        ? t("ux3t1CtoQuestions", { count: d.ask.questions.length })
        : waitTitle(d.ask);
    return { icon: askIcons[d.ask?.kind ?? "deliver"], title, context: `${short(d.run.target.ticket)} ${d.run.target.title}` };
  }

  /** A ticket by its number in the rail, its repository being the project's: `acme/api#482` → `#482`. */
  const short = (key: string) => (key.includes("#") ? `#${key.split("#").pop()}` : ticketKey(key));

  const askIcons: Record<AskKind, Component> = {
    route: Route,
    plan: ListChecks,
    questions: CircleQuestionMark,
    visual: Eye,
    deliver: Send,
    gate: BellRing,
    blocked: TriangleAlert,
  };

  /** What a run is at, under its title, and its state: the one the Run page shows. */
  function runLine(o: { run?: Run; team?: TeamRun }): { text: string; pill: [Status, string | undefined] } {
    const team = o.team;
    const pill = (): [Status, string | undefined] => {
      // A team whose ticket run is not known yet is that of an open run.
      const s = shown(o.run ?? { status: "running", prUrl: null, prState: null }, team);
      return [s.status as Status, s.label ? t(s.label) : undefined];
    };
    if (team && team.state !== "done") {
      const done = progress(team);
      const steps = t("teamRunsProgress", { over: done.over, total: done.total });
      const waiting = asks(team)[0];
      const working = current(team)[0];
      const what = waiting ? waitTitle(waiting) : team.state === "running" && working ? t("ux3t1RoleWorks", { role: working.role ?? working.key }) : "";
      return { text: [what, steps].filter(Boolean).join(" · "), pill: pill() };
    }
    const run = o.run;
    if (run?.status === "delivered" && run.prState) return { text: t(run.prState === "merged" ? "ux3t1Merged" : "ticketsUiEventPrClosed"), pill: pill() };
    if (run && awaitsDelivery(run, team)) return { text: t("teamRunsAskDeliver"), pill: pill() };
    return { text: run ? repoName(run.repo) : "", pill: pill() };
  }

  // Every project at a glance, when none is chosen.
  const projects = $derived(app.workspace?.projects ?? []);
  const counts = $derived(tallies());

  function activity(id: string): number | null {
    const times = [
      ...agents.sessions.filter((a) => agentProjects(a).includes(id)).map((a) => a.updated),
      ...team.runs.filter((r) => r.target.project === id).map((r) => r.updated),
      ...tickets.runs.filter((r) => r.project === id).map((r) => r.updated),
    ];
    return times.length ? Math.max(...times) : null;
  }

  function openProject(id: string) {
    setProject(id);
  }

  /**
   * A run's page once its ticket run is known; until then a team that waits goes to its card in the thread, and one
   * that does not is only listed.
   */
  function openRun(o: { run?: Run; team?: TeamRun }): (() => void) | undefined {
    const run = o.run;
    if (run) return () => showRun(run.id);
    const w = o.team && projectWaits(null).find((x) => "run" in x && x.run === o.team?.run);
    return w ? () => goFrom(w) : undefined;
  }

  async function goFrom(w: Wait) {
    error = "";
    await goToWait(w);
  }
</script>

{#snippet more(label: string, onclick: () => void)}
  <button class="more" {onclick}>{label}</button>
{/snippet}

{#snippet waitRow(w: Wait, everywhere: boolean)}
  {@const r = row(w)}
  {@const id = everywhere ? waitProject(w) : null}
  {@const p = id ? projects.find((x) => x.id === id) : undefined}
  <button class="rrow hot" class:shown={home.shown === w.key} onclick={() => goFrom(w)}>
    <span class="bell"><r.icon size={14} aria-hidden="true" /></span>
    <span class="t"><b>{r.title}</b>{#if r.context}<span>{r.context}</span>{/if}</span>
    {#if p}<span class="pm"><span class="mark" aria-hidden="true">{initial(p.name)}</span>{p.name}</span>{/if}
    <ChevronRight size={14} aria-hidden="true" class="chev" />
  </button>
{/snippet}

{#snippet runRow(o: { run?: Run; team?: TeamRun }, everywhere: boolean)}
  {@const line = runLine(o)}
  {@const key = o.team?.target.ticket ?? o.run?.ticket.key ?? ""}
  {@const id = o.team?.target.project ?? o.run?.project}
  {@const p = everywhere ? projects.find((x) => x.id === id) : undefined}
  {@const go = openRun(o)}
  {#snippet content()}
    <span class="t">
      <b><span class="key mono">{short(key)}</span> {o.team?.target.title ?? o.run?.title}</b>
      {#if line.text}<span>{line.text}</span>{/if}
    </span>
    {#if p}<span class="pm"><span class="mark" aria-hidden="true">{initial(p.name)}</span>{p.name}</span>{/if}
    <span class="state"><StatusPill status={line.pill[0]} label={line.pill[1]} /></span>
  {/snippet}
  {#if go}
    <button class="rrow" onclick={go}>{@render content()}</button>
  {:else}
    <div class="rrow still">{@render content()}</div>
  {/if}
{/snippet}

{#snippet lead()}
  <span class="who" class:cto={home.with === "cto"} aria-hidden="true">{home.with === "cto" ? t("ux3t1Cto") : initials("expert")}</span>
  <div class="grow">
    <b>{home.with === "cto" ? t("ux3t1Cto") : t("ux3t1Expert")}</b>
    {#if home.with === "cto"}<span class="sub">{t("ctoStart")}</span>{/if}
  </div>
  <div class="segmented" role="group" aria-label={t("ux3t1With")}>
    <button aria-pressed={home.with === "cto"} onclick={() => (home.with = "cto")}>{t("ux3t1Cto")}</button>
    <button aria-pressed={home.with === "expert"} onclick={() => (home.with = "expert")}>{t("ux3t1Expert")}</button>
  </div>
{/snippet}

{#snippet card(item: Notice, fallback: Snippet<[Notice]>)}
  {@const key = placed.at.get(item.key)}
  {#if key}{@render cards(key)}{:else}<div class="told">{@render fallback(item)}</div>{/if}
{/snippet}

{#snippet cards(key: string)}
  {@const d = decision(key)}
  {#if d}
    <!-- Only the latest open decision keeps its primary button: one main action on the Home. -->
    <div class="decision" data-wait={key} tabindex="-1">
      <WaitCards run={d.run} step={d.step} context compact primary={key === main} ondismiss={() => delete delivering[key]} />
    </div>
  {/if}
{/snippet}

{#snippet top()}
  {@const ob = project ? onboarding[project.id] : undefined}
  {#if project && ob && setupSteps.includes(ob.step)}
    {@const at = setupSteps.indexOf(ob.step)}
    <article class="setup" aria-labelledby="home-setup">
      <header>
        <WandSparkles size={18} aria-hidden="true" />
        <b id="home-setup">{t("obConfigure")}</b>
        <span class="prog">{t("ux3t1Of", { n: at + 1, total: setupSteps.length })}</span>
      </header>
      <ol>
        {#each setupSteps as step, i (step)}
          {@const text = stepText(step, ob)}
          <li class:done={i < at} class:now={i === at} aria-current={i === at ? "step" : undefined}>
            <span class="n">{#if i < at}<Check size={13} aria-hidden="true" />{:else}{i + 1}{/if}</span>
            <span class="t"><b>{t(stepLabels[step as keyof typeof stepLabels])}</b>{#if text}<span>{text}</span>{/if}</span>
            {#if i === at}
              <button class="btn sm" class:primary={main === null && cto?.state !== "waiting"} onclick={() => openSetup(project.id)}>
                {t(stepActions[step as keyof typeof stepActions])}<ChevronRight size={14} aria-hidden="true" />
              </button>
            {/if}
          </li>
        {/each}
      </ol>
    </article>
  {/if}
  {#each placed.rest as key (key)}{@render cards(key)}{/each}
  {#each ops.requests.filter((r) => r.project === project?.id) as r (r.id)}
    <div class="decision"><OpsRequest request={r} primary={main === null} /></div>
  {/each}
{/snippet}

<div class="home-view">
  <ViewHeader title={t("ux3t1Home")} context={project ? [project.name, project.repos.map((r) => r.name).join(", ")].filter(Boolean).join(" · ") : t("ticketsAll")}>
    {#snippet actions()}
      {#if !project}
        <button class="btn sm" onclick={() => editProject("")}><Plus size={14} aria-hidden="true" />{t("newProject")}</button>
      {/if}
    {/snippet}
  </ViewHeader>

  {#if error}
    <div class="banners">
      <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
    </div>
  {/if}

  {#if project}
    <div class="home">
      <section class="talk" aria-label={home.with === "cto" ? t("ux3t1Cto") : t("ux3t1Expert")}>
        {#if session}
          {#key session.id}
            <Conversation
              {session}
              embedded
              {lead}
              card={home.with === "cto" ? card : undefined}
              top={home.with === "cto" ? top : undefined}
              bind:notices
            />
          {/key}
        {:else}
          <header class="bar">{@render lead()}</header>
          <p class="opening"><LoaderCircle size={16} aria-hidden="true" class="spin" /></p>
        {/if}
      </section>

      <aside class="rail">
        <section aria-labelledby="home-waits">
          <h2 id="home-waits">
            {t("teamRunsWaits")} <span class="n">{waits.length}</span>
            {#if keys.next && waits.length}<kbd>{keys.next}</kbd>{/if}
          </h2>
          {#each waits as w (w.key)}
            {@render waitRow(w, false)}
          {:else}
            <p class="empty-line">{t("ux3t1NothingWaits")}</p>
          {/each}
        </section>

        <section aria-labelledby="home-live">
          <h2 id="home-live">
            {t("t5InService")} <span class="n">{tiles.length + headless.length}</span>
            {@render more(t("chatView"), showConversations)}
          </h2>
          {#each tiles.slice(0, 5) as s (s.id)}
            <button class="rrow" onclick={() => focusSession(s.id)}>
              <span class="who sm term" aria-hidden="true"><SquareTerminal size={12} /></span>
              <span class="t"><b>{s.role ?? s.title}</b><span>{s.role ? s.title : shortPath(s.cwd)}</span></span>
              {#if s.status}<StatusPill status={s.status} />{/if}
            </button>
          {/each}
          {#each headless.slice(0, Math.max(0, 5 - tiles.length)) as a (a.id)}
            <button class="rrow" onclick={() => showConversation(a.id)}>
              <span class="who sm" aria-hidden="true">{initials(a.role ?? a.title)}</span>
              <span class="t"><b>{a.role ?? a.title}</b><span>{a.role ? a.title : shortPath(a.cwd)}</span></span>
              <StatusPill status={a.state} />
            </button>
          {/each}
          {#if !tiles.length && !headless.length}<p class="empty-line">{t("ux3t1NobodyWorks")}</p>{/if}
        </section>

        <section aria-labelledby="home-runs">
          <h2 id="home-runs">
            {t("teamRunsTab")} <span class="n">{runs.length}</span>
            {@render more(t("ux3t1AllRuns"), () => showAll("runs"))}
          </h2>
          {#each runs.slice(0, 5) as o (o.run?.id ?? o.team?.run)}
            {@render runRow(o, false)}
          {:else}
            <p class="empty-line">{t("ux3t1NoRuns")}</p>
          {/each}
        </section>

        <section aria-labelledby="home-tickets">
          <h2 id="home-tickets">
            {t("viewTickets")} <span class="n">{inbox.length}</span>
            {@render more(t("ux3t1Inbox"), () => showAll("inbox"))}
          </h2>
          {#each inbox.slice(0, 3) as ticket (ticket.url)}
            {@const picking = choosing?.url === ticket.url ? choosing : null}
            <div class="rrow ticket" class:open={!!picking || starting === ticket.url}>
              <button class="t" title={t("ticketsUiOpenLink")} onclick={() => openUrl(ticket.url)}>
                <b>{ticket.title}</b>
                <span>{ticket.key} · {ago(Date.parse(ticket.updated) / 1000)}</span>
              </button>
              {#if picking}
                <select class="field" aria-label={t("ctoEntryRepo")} bind:value={picking.repo}>
                  {#each picking.resolution.repos as r (r.path)}<option value={r.path}>{r.name}</option>{/each}
                </select>
              {/if}
              <button class="btn sm team" disabled={!!starting} onclick={() => withTeam(ticket)}>
                {#if starting === ticket.url}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Users size={14} aria-hidden="true" />{/if}
                {t("teamRunsWithTeam")}
              </button>
            </div>
          {/each}
        </section>
      </aside>
    </div>
  {:else}
    {@const all = projectWaits(null)}
    <div class="page">
      <div class="inner">
        {#if all.length}
          <h2 class="sec">{t("teamRunsWaits")} <span class="n">{all.length}</span>{#if keys.next}<kbd>{keys.next}</kbd>{/if}</h2>
          <div class="table">
            {#each all as w (w.key)}{@render waitRow(w, true)}{/each}
          </div>
        {/if}

        <h2 class="sec">{t("projects")} <span class="n">{projects.length}</span></h2>
        <div class="table" role="table" aria-label={t("projects")}>
          <div class="trow th" role="row">
            <span class="t" role="columnheader">{t("ctoEntryProject")}</span>
            <span class="agents" role="columnheader">{t("t5InService")}</span>
            <span class="num" role="columnheader">{t("teamRunsTab")}</span>
            <span class="num" role="columnheader">{t("viewTickets")}</span>
            <span class="num" role="columnheader">{t("ux3t1Activity")}</span>
            <span class="chev-space"></span>
          </div>
          {#each projects as p (p.id)}
            {@const waiting = projectWaits(p.id).length}
            {@const tally = counts[p.id]}
            {@const open = openRuns(tickets.runs, team.runs, p.id).length}
            {@const at = activity(p.id)}
            <button class="trow" role="row" onclick={() => openProject(p.id)}>
              <span class="t" role="cell">
                <span class="mark big" aria-hidden="true">{initial(p.name)}</span>
                <span class="names"><b>{p.name}</b><span>{p.repos.map((r) => r.name).join(", ")}</span></span>
              </span>
              <span class="agents" role="cell">
                {#if waiting}<StatusPill status="waiting" label={t(waiting === 1 ? "ux3t1WaitOne" : "ux3t1WaitMany", { count: waiting })} />{/if}
                {#if tally?.working}<StatusPill status="working" label={String(tally.working)} />{/if}
                {#if tally?.failed}<StatusPill status="failed" label={t("ux3t1Failed", { count: tally.failed })} />{/if}
              </span>
              <span class="num" role="cell">{open ? t(open === 1 ? "ux3t1OpenOne" : "ux3t1OpenMany", { count: open }) : "—"}</span>
              <span class="num" role="cell">{#if tickets.inbox}<b>{tickets.inbox.filter((x) => x.project === p.id).length}</b>{:else}—{/if}</span>
              <span class="num" role="cell">{at === null ? "—" : ago(at)}</span>
              <ChevronRight size={14} aria-hidden="true" class="chev" />
            </button>
          {/each}
        </div>

        <h2 class="sec">
          {t("ux3t1OpenRuns")} <span class="n">{runs.length}</span>
          {@render more(t("ux3t1AllRuns"), () => showAll("runs"))}
        </h2>
        <div class="table">
          {#each runs as o (o.run?.id ?? o.team?.run)}
            {@render runRow(o, true)}
          {:else}
            <p class="empty-line pad">{t("ux3t1NoRuns")}</p>
          {/each}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .home-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .home {
    display: grid;
    flex: 1;
    grid-template-columns: minmax(0, 1fr) 384px;
    min-height: 0;
  }

  .talk {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--panel);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
    height: 52px;
    padding: 0 12px 0 20px;
    border-bottom: 1px solid var(--border);
  }

  .opening {
    display: grid;
    flex: 1;
    place-items: center;
    margin: 0;
    color: var(--muted);
  }

  /* The conversation's lead: who is spoken to, and the switch to the other. */
  .who {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .who.cto {
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--accent-text);
    font-size: 9px;
    letter-spacing: 0;
  }

  .who.sm {
    width: 22px;
    height: 22px;
    font-size: 10px;
  }

  .who.term {
    border-radius: var(--radius-sm);
    background: var(--term-bg);
    box-shadow: inset 0 0 0 1px var(--border-strong);
    color: #e7e8eb;
  }

  .grow {
    display: flex;
    flex: 1;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }

  .grow b {
    font-size: var(--text-md);
    font-weight: 650;
  }

  .sub {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
  }

  .segmented button {
    height: 24px;
    padding: 0 10px;
  }

  /* The rail: the project at a glance. */
  .rail {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow-y: auto;
    border-left: 1px solid var(--border);
    background: var(--chrome);
  }

  .rail section {
    padding: 16px 16px 12px;
    border-bottom: 1px solid var(--border);
  }

  .rail section:last-child {
    border-bottom: 0;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 8px;
    font-size: var(--text-md);
    font-weight: 650;
  }

  h2 .n {
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  h2 kbd {
    margin-left: auto;
  }

  .more {
    margin-left: auto;
    padding: 2px 4px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--accent-ink);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
  }

  .more:hover {
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .rrow {
    display: flex;
    align-items: center;
    gap: 10px;
    width: calc(100% + 16px);
    min-height: 44px;
    margin: 0 -8px;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .rrow + .rrow {
    margin-top: 2px;
  }

  .rrow:hover {
    background: var(--hover);
  }

  .rrow.hot {
    background: var(--waiting-soft);
  }

  .rrow.hot:hover,
  .rrow.hot.shown {
    box-shadow: inset 0 0 0 1px var(--waiting-ring);
  }

  .rrow .t {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .rrow .t b {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rrow .t > span {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bell {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--waiting);
    color: var(--waiting-text);
  }

  .rrow.still {
    cursor: default;
  }

  .rrow.still:hover {
    background: none;
  }

  .rrow :global(.chev) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .key {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .empty-line {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  /* A ticket of the rail: its title opens it, "With the team" shows on hover or from the keyboard. */
  .ticket {
    cursor: default;
  }

  .ticket .t {
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .ticket .team {
    flex-shrink: 0;
    visibility: hidden;
  }

  .ticket:hover .team,
  .ticket:focus-within .team,
  .ticket.open .team {
    visibility: visible;
  }

  .ticket select {
    width: 120px;
    height: 26px;
  }

  .decision {
    border-radius: var(--radius-lg);
    scroll-margin: 24px;
  }

  .decision:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* A notice whose step was answered no longer asks: amber stays for what waits. */
  .told :global(.termf.asks) {
    border-color: var(--border);
    background: var(--chrome);
  }

  .told :global(.termf.asks svg) {
    color: var(--muted);
  }

  /* A new project's setup, at the head of the thread. */
  .setup {
    display: flex;
    flex-direction: column;
    padding: 16px 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-1);
  }

  .setup header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }

  .setup header :global(svg) {
    color: var(--accent-ink);
  }

  .setup header b {
    font-size: var(--text-md);
    font-weight: 650;
  }

  .prog {
    margin-left: auto;
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .setup ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .setup li {
    display: grid;
    grid-template-columns: 26px minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    min-height: 52px;
    padding: 6px 0;
    border-top: 1px solid var(--border);
  }

  .setup .n {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 650;
  }

  .setup .done .n {
    border-color: transparent;
    background: var(--done-soft);
    color: var(--done);
  }

  .setup .now .n {
    border-color: transparent;
    background: var(--accent);
    color: var(--accent-text);
  }

  .setup .t {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .setup .t b {
    font-weight: 600;
  }

  .setup li:not(.done):not(.now) .t b {
    color: var(--muted);
    font-weight: 500;
  }

  .setup .t span {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  /* Every project: what waits, the projects, the open runs. */
  .page {
    flex: 1;
    min-height: 0;
    padding: 20px 28px 40px;
    overflow-y: auto;
  }

  .inner {
    max-width: 1080px;
  }

  .sec {
    margin: 22px 0 10px;
  }

  .sec:first-child {
    margin-top: 0;
  }

  .table {
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .table .rrow,
  .trow {
    width: 100%;
    min-height: 52px;
    margin: 0;
    padding: 8px 14px;
    border-radius: 0;
  }


  .table .rrow + .rrow {
    margin-top: 0;
  }

  .table .rrow.hot:hover,
  .table .rrow.hot.shown {
    box-shadow: none;
    background: color-mix(in srgb, var(--waiting-soft), var(--waiting) 12%);
  }

  .trow {
    display: flex;
    align-items: center;
    gap: 12px;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .trow:hover {
    background: var(--hover);
  }

  .table > * + * {
    border-top: 1px solid var(--border);
  }

  .table .state {
    display: flex;
    flex-shrink: 0;
    justify-content: flex-end;
    width: 140px;
  }

  .trow.th {
    min-height: 34px;
    background: var(--chrome);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    cursor: default;
  }

  .trow .t {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .names {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .names b {
    font-weight: 500;
  }

  .names span {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .agents {
    display: flex;
    flex-shrink: 0;
    gap: 4px;
    width: 210px;
  }

  .num {
    flex-shrink: 0;
    width: 96px;
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .num b {
    color: var(--text);
    font-weight: 600;
  }

  .chev-space {
    width: 14px;
  }

  .mark {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: var(--text-xs);
    font-weight: 700;
  }

  .mark.big {
    width: 26px;
    height: 26px;
    font-size: var(--text-sm);
  }

  .pm {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 8px;
    width: 150px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .pad {
    padding: 14px;
  }
</style>
