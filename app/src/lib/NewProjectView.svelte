<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import Check from "@lucide/svelte/icons/check";
  import FileCode from "@lucide/svelte/icons/file-code";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import Layers from "@lucide/svelte/icons/layers";
  import LibraryBig from "@lucide/svelte/icons/library-big";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Ticket from "@lucide/svelte/icons/ticket";
  import Users from "@lucide/svelte/icons/users";
  import Workflow from "@lucide/svelte/icons/workflow";
  import X from "@lucide/svelte/icons/x";
  import { untrack, type Component } from "svelte";
  import { initial } from "$lib/ProjectSwitcher.svelte";
  import RepoDetection, { type Detected } from "$lib/RepoDetection.svelte";
  import RepoPicker from "$lib/RepoPicker.svelte";
  import ForgeIcon from "$lib/ui/ForgeIcon.svelte";
  import { lang, t } from "$lib/i18n";
  import { cards, closeNewProject, creating } from "$lib/newproject.svelte";
  import { analyze, openSetup } from "$lib/onboarding.svelte";
  import {
    githubIssues,
    members,
    recipeLines,
    teamWhy,
    trustState,
    type RepoTrust,
    type TeamEdit,
    type Trackers,
  } from "$lib/onboarding-logic";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { derive, githubSlugs, projectId, unique, type Card } from "$lib/setup-logic";
  import { app, setProject, shortPath, ui, type Project } from "$lib/state.svelte";
  import { added } from "$lib/team-logic";

  type Step = "repos" | "detect" | "setup" | "trust" | "create";
  type Block = "team" | "tickets" | "worktree" | "ci" | "skills";
  type RoleChoice = { role: string; keep: boolean; why: string; detail: string[] };
  type Draft = {
    roles: RoleChoice[];
    steps: string[];
    team: TeamEdit;
    skills: { name: string; by: string[]; package: string | null }[];
    trust: RepoTrust[];
  };
  type Notify = "mine" | "all" | "off";
  type Key = Parameters<typeof t>[0];

  const steps: { id: Step; label: Key }[] = [
    { id: "repos", label: "setupStepRepos" },
    { id: "detect", label: "setupStepDetect" },
    { id: "setup", label: "setupStepSetup" },
    { id: "trust", label: "setupStepTrust" },
    { id: "create", label: "setupStepCreate" },
  ];

  let step = $state<Step>("repos");
  /** The furthest step reached: the stepper goes back to any step up to it. */
  let reached = $state(0);
  let chosen = $state<string[]>([]);
  /** The name typed by the owner; null follows the one derived from the repositories. */
  let typed = $state<string | null>(null);
  let editing = $state(false);
  let found = $state<Record<string, Detected | string>>({});
  let draft = $state<Draft | null>(null);
  /** The selection the draft, or the request for it, is for. */
  let draftFor = "";
  let draftError = $state("");
  let aside = $state<Block[]>([]);
  /** The selection trusted on the Trust step: another one is trusted only once seen there. */
  let trustedFor = $state<string | null>(null);
  let notify = $state<Notify>("mine");
  let preview = $state("");
  let busy = $state(false);
  let error = $state("");
  let main = $state<HTMLElement>();

  const index = $derived(steps.findIndex((s) => s.id === step));
  const home = $derived(app.workspace?.home ?? "");
  const projects = $derived(app.workspace?.projects ?? []);
  const taken = $derived(projects.map((p) => p.name));
  const chosenCards = $derived(
    chosen.map((path): Card => {
      const known = [...(app.workspace?.repos ?? []), ...projects.flatMap((p) => p.repos)].find((r) => r.path === path);
      return { ...(known ?? { name: path.split(/[\\/]/).pop() ?? path, path, modified: null }), ...cards[path] };
    }),
  );
  const guess = $derived(derive(chosenCards, home));
  const suggested = $derived(guess ? unique(guess.name, taken) : "");
  const name = $derived((typed ?? "").trim() || suggested);
  const clash = $derived(typed !== null && !!typed.trim() && unique(typed.trim(), taken) !== typed.trim());
  const github = $derived(githubSlugs(chosenCards));
  const trackers = $derived<Trackers | null>(
    github.length ? githubIssues({ github: null, jira: null, mcp: {}, implicit: true }) : null,
  );
  const detected = $derived(chosen.map((p) => found[p]).filter((d): d is Detected => !!d && typeof d !== "string"));
  const recipe = $derived(
    recipeLines(Object.fromEntries(chosenCards.flatMap((c) => (typeof found[c.path] === "object" ? [[c.name, (found[c.path] as Detected).recipe]] : [])))),
  );
  const workflows = $derived(
    chosenCards.flatMap((c) => {
      const d = found[c.path];
      return typeof d === "object" && d.facts.workflows.length ? [`${c.name} (${d.facts.workflows.join(", ")})`] : [];
    }),
  );
  const keys = $derived([...new Set(detected.flatMap((d) => d.keys))]);
  const kept = $derived(draft?.roles.filter((r) => r.keep) ?? []);
  const left = $derived(draft?.roles.filter((r) => !r.keep) ?? []);
  const id = $derived(projectId(name, projects.map((p) => p.id)));
  const trust = $derived(trustedFor !== null && trustedFor === chosen.join("\n"));
  /** Creating waits for the proposal of the current selection, unless its team is set aside. */
  const waiting = $derived(!draft && !aside.includes("team"));
  const toTrust = $derived(draft?.trust.filter((r) => !r.trusted).length ?? 0);
  const blocks: Block[] = ["team", "tickets", "worktree", "ci", "skills"];
  const blockLabel: Record<Block, Key> = {
    team: "obTeamTitle",
    tickets: "obTicketsTitle",
    worktree: "obRecipeTitle",
    ci: "obCiTitle",
    skills: "stackSkillsTitle",
  };
  const notifyLabel = { mine: "ciNotifyMine", all: "ciNotifyAll", off: "ciNotifyOff" } as const;
  const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD", maximumFractionDigits: 0 });
  const configDir = $derived(app.workspace?.configDir ?? "");

  invoke<Notify>("ci_notify").then((n) => (notify = n), () => {});

  // Each repository is read as soon as it is chosen: its findings are there when the owner looks.
  const started = new Set<string>();
  $effect(() => {
    for (const path of chosen) {
      if (started.has(path)) continue;
      started.add(path);
      invoke<Detected>("setup_detect", { path }).then(
        (d) => (found[path] = d),
        (e) => (found[path] = String(e)),
      );
    }
  });

  // The proposal follows the repositories once the owner gets past the detection.
  $effect(() => {
    const key = chosen.join("\n");
    if (key === draftFor) return;
    draft = null;
    draftError = "";
    draftFor = "";
    if (index < 1 || !chosen.length) return;
    draftFor = key;
    invoke<Draft>("setup_draft", { repos: [...chosen] }).then(
      (d) => {
        if (draftFor === key) draft = d;
      },
      (e) => {
        if (draftFor === key) ((draftError = String(e)), (draftFor = ""));
      },
    );
  });

  // Another selection than the one trusted: back to the Trust step before creating.
  $effect(() => {
    if (trustedFor === null || trustedFor === chosen.join("\n")) return;
    trustedFor = null;
    const at = steps.findIndex((s) => s.id === "trust");
    untrack(() => {
      reached = Math.min(reached, at);
      if (index > at) step = "trust";
    });
  });

  const writes = $derived({
    team: !aside.includes("team") && draft?.team.team ? draft.team : null,
    trackers: !aside.includes("tickets") ? trackers : null,
  });
  $effect(() => {
    if (step !== "create" || !name) return;
    const args = { name, repos: [...chosen], team: writes.team, trackers: writes.trackers };
    invoke<string>("setup_preview", args).then(
      (text) => (preview = text),
      (e) => (preview = String(e)),
    );
  });

  // Every repository removed: back to picking them.
  $effect(() => {
    if (!chosen.length && index > 0) step = "repos";
  });

  function go(to: Step) {
    const i = steps.findIndex((s) => s.id === to);
    if (i > 0 && !chosen.length) return;
    step = to;
    reached = Math.max(reached, i);
    main?.scrollTo({ top: 0 });
  }

  const next = () => (index < steps.length - 1 ? go(steps[index + 1].id) : create());

  function toggle(block: Block) {
    aside = aside.includes(block) ? aside.filter((b) => b !== block) : [...aside, block];
  }

  /** Creates the project, applies what was kept of the proposal, then hands over to the Home or the assistant. */
  async function create() {
    if (busy || !name || !chosen.length || waiting) return;
    busy = true;
    error = "";
    let project: string | null = null;
    try {
      const before = app.workspace!.projects;
      const after = await invoke<Project[]>("project_save", { id: null, name, repos: chosen });
      app.workspace!.projects = after;
      project = added(before, after);
    } catch (e) {
      error = String(e);
    }
    if (!project) {
      error ||= t("setupNotCreated");
      busy = false;
      return;
    }
    // From here the project exists: a step that fails is reported, never a reason to create it again.
    const failed: string[] = [];
    const attempt = async (what: () => Promise<unknown>) => {
      try {
        await what();
      } catch (e) {
        failed.push(String(e));
      }
    };
    // The setup replaces the assistant's first opening.
    await attempt(() => invoke("onboarding_seen", { project }));
    const decide = (proposal: string, detail = "") =>
      invoke("onboarding_decide", { project, proposal, decision: "accepted", detail });
    if (writes.team) await attempt(() => invoke("onboarding_team_save", { project, edit: writes.team }));
    if (writes.trackers)
      await attempt(async () => {
        await invoke("tickets_sources_save", { project, trackers: writes.trackers });
        await decide("tickets", github.join(", "));
      });
    if (!aside.includes("worktree"))
      await attempt(async () => {
        const view = await invoke<{ detected: unknown }>("worktree_recipe", { project });
        await invoke("worktree_recipe_save", { project, recipe: view.detected });
        await decide("worktree", chosenCards.map((c) => c.name).join(", "));
      });
    if (!aside.includes("ci")) await attempt(() => decide("ci", notify));
    if (trust) await attempt(() => invoke("claude_trust_set", { project, on: true }));
    // Runs in the background: the Home and the assistant follow it, and say why if it fails.
    analyze(project, lang).catch(() => {});
    if (failed.length) app.error = t("setupPartial", { error: failed.join(" · ") });
    setProject(project);
    creating.open = false;
    creating.round++;
    ui.view = "home";
    if (aside.length) openSetup(project, "proposals");
  }

  function onkeydown(e: KeyboardEvent) {
    if (ui.view !== "newProject" || ui.palette || ui.help || e.isComposing) return;
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      if (!busy && chosen.length && name) next();
    } else if (e.key === "Escape" && !e.defaultPrevented && !editing) {
      if ((e.target as Element | null)?.closest?.(".xterm")) return;
      e.preventDefault();
      closeNewProject();
    }
  }

  const sourceText = $derived.by(() => {
    if (typed !== null && typed.trim()) return clash ? t("setupNameTaken", { name: typed.trim() }) : t("setupNameTyped");
    if (!guess) return t("setupNameEmpty");
    const why = {
      repo: t("setupNameFromRepo"),
      owner: t("setupNameFromOwner", { owner: guess.name }),
      folder: t("setupNameFromFolder", { folder: guess.name }),
      first: t("setupNameFromFirst"),
    }[guess.from];
    return suggested !== guess.name ? `${why} ${t("setupNameNumbered", { name: guess.name })}` : why;
  });

  const blockIcon: Record<Block, Component> = {
    team: Users,
    tickets: Ticket,
    worktree: FolderGit2,
    ci: Workflow,
    skills: Layers,
  };
  const why = (r: RoleChoice) =>
    t(teamWhy[r.why as keyof typeof teamWhy] ?? "obTeamWhyOwn", { detail: r.detail.join(", ") });
</script>

<svelte:window {onkeydown} />

{#snippet blockState(block: Block)}
  {@const off = aside.includes(block)}
  <span class="state" class:off>{off ? t("setupAside") : t("setupRecommended")}</span>
  <button type="button" class="btn sm" aria-pressed={off} onclick={() => toggle(block)}>
    {off ? t("setupKeep") : t("setupCustomize")}
  </button>
{/snippet}

<div class="new-project">
  <ViewHeader title={t("newProject")}>
    <ol class="stepper" aria-label={t("setupSteps")}>
      {#each steps as s, i (s.id)}
        {@const done = i < index}
        <li>
          <button
            type="button"
            class:done
            aria-current={s.id === step ? "step" : undefined}
            disabled={i > reached || (i > 0 && !chosen.length) || busy}
            onclick={() => go(s.id)}
          >
            <span class="dot">{#if done}<Check size={11} strokeWidth={3} aria-hidden="true" />{:else}{i + 1}{/if}</span>
            <span class="label">{t(s.label)}</span>
          </button>
        </li>
      {/each}
    </ol>
    {#snippet actions()}
      <button type="button" class="btn sm quiet cancel" onclick={closeNewProject} disabled={busy}>
        {t("cancel")}<kbd>Esc</kbd>
      </button>
    {/snippet}
  </ViewHeader>

  <div class="body">
    <main class="main" bind:this={main}>
      {#key step}
        <div class="step">
          {#if step === "repos"}
            <h2>{t("setupReposTitle")}</h2>
            <p class="lead">{t("setupReposHelp")}</p>
            <RepoPicker bind:chosen autofocus />
          {:else if step === "detect"}
            <h2>{t("setupDetectTitle")}</h2>
            <p class="lead">{t("setupDetectHelp")}</p>
            <div class="stack">
              {#each chosenCards as card (card.path)}
                <RepoDetection {card} found={found[card.path]} />
              {/each}
              <aside class="claude">
                <Sparkles size={16} aria-hidden="true" />
                <div>
                  <b>{t("setupClaudeTitle")}</b>
                  <p>{t("setupClaudeLater", { budget: money.format(2) })}</p>
                </div>
              </aside>
            </div>
          {:else if step === "setup"}
            <h2>{t("setupProposeTitle")}</h2>
            <p class="lead">{t("setupProposeHelp")}</p>
            {#if draftError}<p class="notice error" role="alert">{draftError}</p>{/if}
            <ul class="blocks">
              {#each blocks as block (block)}
                {@const Icon = blockIcon[block]}
                {@const off = aside.includes(block)}
                <li class:off>
                  <span class="icon"><Icon size={16} aria-hidden="true" /></span>
                  <div class="what">
                    <h3>{t(blockLabel[block])}</h3>
                    {#if block === "team"}
                      {#if draft}
                        <div class="lines">
                          {#each draft.team.team?.squads ?? [] as sq (sq.id)}
                            <p class="row">
                              <span class="key">{t("obTeamSquad", { id: sq.id })}</span>
                              {#each members(sq.roles) as m (m)}<span class="badge">{m}</span>{/each}
                            </p>
                          {/each}
                          {#if draft.team.team?.staff.length}
                            <p class="row">
                              <span class="key">{t("obTeamStaff")}</span>
                              {#each draft.team.team.staff as r (r)}
                                <span class="badge" title={kept.find((k) => k.role === r) ? why(kept.find((k) => k.role === r)!) : ""}>{r}</span>
                              {/each}
                            </p>
                          {/if}
                          {#if left.length}
                            <p class="note">{t("obTeamLeftOut", { roles: left.map((r) => `${r.role} (${why(r)})`).join(", ") })}</p>
                          {/if}
                        </div>
                      {:else}
                        <p class="note wait"><LoaderCircle size={13} aria-hidden="true" class="spin" />{t("setupProposing")}</p>
                      {/if}
                    {:else if block === "tickets"}
                      <p>{github.length ? t("setupTicketsGithub", { repos: github.join(", ") }) : t("obTicketsNone")}</p>
                      {#if keys.length}<p class="note">{t("setupTicketsKeys", { keys: keys.map((k) => `${k}-…`).join(", ") })}</p>{/if}
                    {:else if block === "worktree"}
                      <ul class="recipe">
                        {#each recipe as line (line.repo)}
                          <li>
                            <span class="key">{line.repo}</span>
                            {#each [...new Set(line.parts)] as part (part)}<code>{part}</code>{:else}<span class="muted">{t("obRecipeNothing")}</span>{/each}
                          </li>
                        {/each}
                      </ul>
                    {:else if block === "ci"}
                      <p>{workflows.length ? t("obCiWorkflows", { list: workflows.join(" · ") }) : t("obCiNone")}</p>
                      <p class="note">{t("setupCiNotify", { mode: t(notifyLabel[notify]) })}</p>
                    {:else if block === "skills"}
                      {#if draft}
                        {#if draft.skills.length}
                          <p class="row">
                            {#each draft.skills as s (s.name)}
                              <span class="badge" title={s.package ?? s.by.join(", ")}>{s.name}</span>
                            {/each}
                          </p>
                        {:else}
                          <p class="muted">{t("setupSkillsNone")}</p>
                        {/if}
                      {:else}
                        <p class="note wait"><LoaderCircle size={13} aria-hidden="true" class="spin" />{t("setupProposing")}</p>
                      {/if}
                    {/if}
                    {#if off}<p class="note aside-note">{t("setupAsideHint")}</p>{/if}
                  </div>
                  <div class="act">{@render blockState(block)}</div>
                </li>
              {/each}
              <li class="later">
                <span class="icon"><LibraryBig size={16} aria-hidden="true" /></span>
                <div class="what">
                  <h3>{t("setupConvTitle")}</h3>
                  <p>{t("setupConvText")}</p>
                </div>
                <div class="act"><span class="state">{t("setupLater")}</span></div>
              </li>
            </ul>
          {:else if step === "trust"}
            <h2>{t("setupTrustTitle")}</h2>
            <p class="lead">{t("setupTrustHelp")}</p>
            <div class="trust">
              <ul class="loads">
                <li><FileCode size={15} aria-hidden="true" /><span>{t("setupTrustRules")}</span></li>
                <li><FileCode size={15} aria-hidden="true" /><span>{t("setupTrustMcp")}</span></li>
                <li><Layers size={15} aria-hidden="true" /><span>{t("setupTrustSkills")}</span></li>
              </ul>
              {#if draft}
                <ul class="repos">
                  {#each draft.trust as r (r.path)}
                    <li>
                      <span class="forge"><ForgeIcon host={cards[r.path]?.remote?.host} size={14} /></span>
                      <b>{r.name}</b>
                      <span class="mono path" title={r.root}>{shortPath(r.root)}</span>
                      <span class="loaded">
                        {#if r.allow || r.mcp}
                          {[r.allow ? t("obTrustRules", { count: r.allow }) : "", r.mcp ? t("obTrustMcp", { count: r.mcp }) : ""].filter(Boolean).join(" · ")}
                        {:else}{t("setupTrustNothing")}{/if}
                      </span>
                      <span class="badge" class:ok={r.trusted}>{t(r.trusted ? "obTrusted" : "obUntrusted")}</span>
                    </li>
                  {/each}
                </ul>
                {#if trustState(draft.trust) === "on"}
                  <p class="notice ok">{t("setupTrustAll")}</p>
                {:else}
                  <label class="agree" class:on={trust}>
                    <input
                      type="checkbox"
                      checked={trust}
                      onchange={(e) => (trustedFor = e.currentTarget.checked ? chosen.join("\n") : null)}
                    />
                    <span>
                      <b>{t("obTrustConfirm")}</b>
                      <span class="muted">{t("obTrustNote")}</span>
                    </span>
                  </label>
                  <p class="note">{t("setupTrustLater")}</p>
                {/if}
              {:else}
                <p class="note wait"><LoaderCircle size={13} aria-hidden="true" class="spin" />{t("setupProposing")}</p>
              {/if}
            </div>
          {:else}
            <h2>{t("setupCreateTitle", { name })}</h2>
            <p class="lead">{t("setupCreateHelp")}</p>
            <div class="recap">
              <section>
                <h3>{t("setupWrites")}</h3>
                <figure class="file">
                  <figcaption>
                    <FileCode size={14} aria-hidden="true" />
                    <span class="mono">{shortPath(`${configDir}/projects/${id}.toml`)}</span>
                  </figcaption>
                  <pre><code>{preview}</code></pre>
                </figure>
                <ul class="also">
                  {#if !aside.includes("worktree")}<li><Check size={14} aria-hidden="true" />{t("setupWritesRecipe")}</li>{/if}
                  {#if trust && toTrust}<li><ShieldCheck size={14} aria-hidden="true" />{t("setupWritesTrust", { count: toTrust })}</li>{/if}
                  <li class="none"><X size={14} aria-hidden="true" />{t("setupNothingInRepos")}</li>
                </ul>
              </section>
              <section>
                <h3>{t("setupThen")}</h3>
                <ol class="then">
                  <li>{t("setupThenHome")}</li>
                  <li>{t("setupThenAnalysis")}</li>
                  {#if aside.length}
                    <li>{t("setupThenAssistant", { list: aside.map((b) => t(blockLabel[b])).join(", ") })}</li>
                  {/if}
                </ol>
              </section>
            </div>
          {/if}
        </div>
      {/key}
    </main>

    <aside class="project" aria-label={t("setupYourProject")}>
      <div class="identity">
        <span class="mark" class:empty={!name} aria-hidden="true">{name ? initial(name) : ""}</span>
        {#if editing}
          <input
            class="name-field"
            aria-label={t("projectName")}
            placeholder={suggested || t("projectName")}
            value={typed ?? name}
            spellcheck="false"
            autocomplete="off"
            oninput={(e) => (typed = e.currentTarget.value)}
            onblur={() => (editing = false)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                if (e.key === "Escape") typed = null;
                editing = false;
              }
            }}
            {@attach (el) => {
              el.focus();
              el.select();
            }}
          />
        {:else}
          <button type="button" class="name" class:placeholder={!name} title={t("setupEditName")} onclick={() => (editing = true)}>
            <span>{name || t("newProject")}</span>
            <Pencil size={14} aria-hidden="true" />
          </button>
        {/if}
      </div>
      <p class="source" class:warn={clash}>
        {sourceText}
        {#if clash}
          <button type="button" class="link" onclick={() => (typed = unique(typed!.trim(), taken))}>
            {t("setupNameUse", { name: unique(typed!.trim(), taken) })}
          </button>
        {:else if typed !== null && typed.trim() && suggested}
          <button type="button" class="link" onclick={() => (typed = null)}>{t("setupNameReset", { name: suggested })}</button>
        {/if}
      </p>

      <h3 class="count">{t("repos")} <span>{chosen.length}</span></h3>
      {#if chosenCards.length}
        <ul class="chosen">
          {#each chosenCards as card (card.path)}
            {@const d = found[card.path]}
            <li>
              <span class="forge"><ForgeIcon host={card.remote?.host} size={13} /></span>
              <span class="n" title={shortPath(card.path)}>{card.name}</span>
              {#if index >= 1}
                <span class="st" title={d === undefined ? t("setupDetecting", { name: card.name }) : ""}>
                  {#if d === undefined}<LoaderCircle size={12} aria-hidden="true" class="spin" />{:else if typeof d !== "string"}<Check size={12} aria-hidden="true" />{/if}
                </span>
              {/if}
              <button
                type="button"
                class="icon-btn sm"
                aria-label={t("t5RemoveRepo", { name: card.name })}
                title={t("t5RemoveRepo", { name: card.name })}
                disabled={busy}
                onclick={() => (chosen = chosen.filter((p) => p !== card.path))}
              >
                <X size={13} aria-hidden="true" />
              </button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="empty">{t("setupNoneChosen")}</p>
      {/if}

      <div class="grow"></div>
      {#if error}<p class="notice error" role="alert">{error}</p>{/if}
      {#if step === "create" && waiting}
        {#if draftError}
          <p class="notice error" role="alert">{draftError}</p>
        {:else}
          <p class="note wait"><LoaderCircle size={13} aria-hidden="true" class="spin" />{t("setupProposing")}</p>
        {/if}
      {/if}
      <div class="actions">
        {#if index > 0}
          <button type="button" class="btn back" disabled={busy} onclick={() => go(steps[index - 1].id)}>
            <ArrowLeft size={14} aria-hidden="true" />{t("obBack")}
          </button>
        {/if}
        <button
          type="button"
          class="btn primary go"
          disabled={!chosen.length || !name || busy || (step === "create" && waiting)}
          onclick={next}
        >
          {#if busy}
            <LoaderCircle size={14} aria-hidden="true" class="spin" />{t("setupCreating")}
          {:else if step === "create"}
            {t("setupCreate")}
          {:else if step === "setup"}
            {aside.length ? t("obContinue") : t("setupKeepAll")}<ArrowRight size={14} aria-hidden="true" />
          {:else}
            {t("obContinue")}<ArrowRight size={14} aria-hidden="true" />
          {/if}
        </button>
      </div>
    </aside>
  </div>
</div>

<style>
  .new-project {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }

  .stepper {
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .stepper li {
    display: flex;
    align-items: center;
  }

  .stepper li + li::before {
    content: "";
    width: 16px;
    height: 1px;
    margin: 0 2px;
    background: var(--border-strong);
  }

  .stepper button {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 28px;
    padding: 0 10px 0 4px;
    border: 0;
    border-radius: var(--radius-pill);
    background: none;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
    transition:
      background-color var(--dur-fast) ease-out,
      color var(--dur-fast) ease-out;
  }

  .stepper button:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }

  .stepper button:disabled {
    opacity: 1;
    color: color-mix(in srgb, var(--muted) 70%, transparent);
  }

  .dot {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    transition:
      background-color var(--dur) ease-out,
      border-color var(--dur) ease-out;
  }

  .stepper .done .dot {
    border-color: var(--done);
    background: var(--done-soft);
    color: var(--done);
  }

  .stepper [aria-current="step"] {
    color: var(--text);
  }

  .stepper [aria-current="step"] .dot {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
  }

  .cancel {
    gap: 8px;
    color: var(--muted);
  }

  .body {
    display: grid;
    flex: 1;
    grid-template: minmax(0, 1fr) / minmax(0, 1fr) 300px;
    min-height: 0;
  }

  .main {
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
  }

  .step {
    display: flex;
    flex-direction: column;
    max-width: 860px;
    margin: 0 auto;
    padding: 32px 36px 48px;
    animation: enter var(--dur-slow) var(--ease-out);
  }

  @keyframes enter {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 650;
    letter-spacing: -0.02em;
    line-height: 1.25;
    text-wrap: balance;
  }

  .lead {
    max-width: 64ch;
    margin: 6px 0 24px;
    color: var(--muted);
    font-size: var(--text-md);
    line-height: 1.5;
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .claude {
    display: flex;
    gap: 12px;
    padding: 14px 16px;
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .claude :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .claude b {
    color: var(--text);
    font-weight: 600;
  }

  .claude p {
    margin: 2px 0 0;
    color: var(--text);
    line-height: 1.5;
  }

  .blocks {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  .blocks > li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 14px;
    padding: 16px 16px 16px 18px;
    transition: opacity var(--dur) ease-out;
  }

  .blocks > li + li {
    border-top: 1px solid var(--border);
  }

  .blocks .icon {
    display: inline-grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .blocks li.off .icon,
  .blocks li.later .icon {
    background: var(--hover);
    color: var(--muted);
  }

  .blocks li.off .what > :not(h3, .aside-note) {
    color: var(--muted);
  }

  .what {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    padding-top: 5px;
  }

  .what p {
    margin: 0;
    line-height: 1.5;
  }

  .lines {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 6px;
  }

  .key {
    min-width: 92px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .note {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .wait {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .aside-note {
    color: var(--accent-ink);
  }

  .recipe {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .recipe li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 6px;
  }

  code {
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .act {
    display: flex;
    align-items: center;
    align-self: start;
    gap: 10px;
    padding-top: 4px;
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--done);
    font-size: var(--text-sm);
    font-weight: 500;
    white-space: nowrap;
  }

  .state::before {
    content: "";
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }

  .state.off,
  .later .state {
    color: var(--muted);
  }

  .trust {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .loads {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 14px 16px;
    border-radius: var(--radius-lg);
    background: var(--hover);
    list-style: none;
  }

  .loads li {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    line-height: 1.5;
  }

  .loads :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--muted);
  }

  .loads :global(code) {
    padding: 0 4px;
  }

  .repos {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  .repos li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 0 14px;
  }

  .repos li + li {
    border-top: 1px solid var(--border);
  }

  .repos .forge {
    display: inline-flex;
  }

  .repos b {
    font-weight: 600;
  }

  .repos .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .loaded {
    color: var(--muted);
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  .agree {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 14px 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--panel);
    cursor: pointer;
    transition:
      border-color var(--dur-fast) ease-out,
      background-color var(--dur-fast) ease-out;
  }

  .agree.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .agree input {
    width: 16px;
    height: 16px;
    margin: 2px 0 0;
  }

  .agree > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .agree b {
    font-weight: 600;
  }

  .agree .muted {
    font-size: var(--text-sm);
  }

  .trust > .note {
    margin: -6px 0 0;
  }

  .recap {
    display: flex;
    flex-direction: column;
    gap: 28px;
  }

  .recap section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .file {
    margin: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .file figcaption {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
    color: var(--muted);
  }

  pre {
    max-height: 340px;
    margin: 0;
    padding: 14px 16px;
    overflow: auto;
    font-size: var(--text-sm);
    line-height: 1.6;
  }

  pre code {
    padding: 0;
    background: none;
    font-size: inherit;
  }

  .also,
  .then {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .also li {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .also :global(svg) {
    color: var(--done);
  }

  .also .none :global(svg) {
    color: var(--muted);
  }

  .then {
    counter-reset: then;
  }

  .then li {
    display: flex;
    align-items: baseline;
    gap: 10px;
    line-height: 1.5;
    counter-increment: then;
  }

  .then li::before {
    content: counter(then);
    display: inline-grid;
    flex-shrink: 0;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    transform: translateY(-1px);
  }

  /* The project as it takes shape, beside every step. */
  .project {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 28px 20px 20px;
    overflow-y: auto;
    border-left: 1px solid var(--border);
    background: var(--chrome);
  }

  .identity {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .mark {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-lg);
    background: var(--accent);
    color: var(--accent-text);
    font-size: var(--text-lg);
    font-weight: 700;
    transition: background-color var(--dur) ease-out;
  }

  .mark.empty {
    border: 1.5px dashed var(--border-strong);
    background: none;
    color: var(--muted);
  }

  .name,
  .name-field {
    flex: 1;
    min-width: 0;
    height: 36px;
    margin: 0;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: var(--radius);
    font-size: var(--text-xl);
    font-weight: 650;
    letter-spacing: -0.015em;
  }

  .name {
    display: flex;
    align-items: center;
    gap: 8px;
    background: none;
    color: var(--text);
    text-align: left;
  }

  .name span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
    opacity: 0;
    transition: opacity var(--dur-fast) ease-out;
  }

  .name:hover,
  .name:focus-visible {
    border-color: var(--border);
    background: var(--panel);
  }

  .name:hover :global(svg),
  .name:focus-visible :global(svg) {
    opacity: 1;
  }

  .name.placeholder {
    color: var(--muted);
  }

  .name-field {
    border-color: var(--focus);
    outline: 1px solid var(--focus);
    background: var(--panel);
    color: var(--text);
  }

  .source {
    margin: 8px 0 0 52px;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.45;
  }

  .source.warn {
    color: var(--warning);
  }

  .link {
    display: inline;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font-size: inherit;
    font-weight: 500;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .count {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 28px 0 8px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .count span {
    color: var(--muted);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .chosen {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0 -8px;
    padding: 0;
    list-style: none;
  }

  .chosen li {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 4px 0 8px;
    border-radius: var(--radius);
    animation: enter var(--dur) var(--ease-out);
  }

  .chosen li:hover {
    background: var(--hover);
  }

  .chosen .forge {
    display: inline-flex;
    color: var(--muted);
  }

  .chosen .n {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chosen .st {
    display: inline-flex;
    color: var(--done);
  }

  .chosen .st :global(.spin) {
    color: var(--muted);
  }

  .empty {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .grow {
    flex: 1;
    min-height: 20px;
  }

  .project .notice {
    margin-bottom: 10px;
  }

  .actions {
    display: flex;
    gap: 8px;
  }

  .actions .go {
    flex: 1;
    height: 36px;
    border-radius: var(--radius-lg);
    font-size: var(--text-md);
  }

  .actions .back {
    height: 36px;
    border-radius: var(--radius-lg);
  }

  @media (max-width: 1080px) {
    .body {
      grid-template-columns: minmax(0, 1fr) 260px;
    }

    .stepper .label {
      display: none;
    }

    .stepper [aria-current="step"] .label {
      display: inline;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .step,
    .chosen li {
      animation: none;
    }
  }
</style>
