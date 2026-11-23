<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import FileCode from "@lucide/svelte/icons/file-code";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import Layers from "@lucide/svelte/icons/layers";
  import LibraryBig from "@lucide/svelte/icons/library-big";
  import ListChecks from "@lucide/svelte/icons/list-checks";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Ticket from "@lucide/svelte/icons/ticket";
  import Users from "@lucide/svelte/icons/users";
  import Workflow from "@lucide/svelte/icons/workflow";
  import { untrack, type Component, type Snippet } from "svelte";
  import type { Detected } from "$lib/RepoDetection.svelte";
  import Segments from "$lib/ui/Segments.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import Switch from "$lib/ui/Switch.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { lang, t } from "$lib/i18n";
  import { analyze, draft, endNewProject, found, runningFor, type Notify, type Row } from "$lib/newproject.svelte";
  import {
    githubIssues,
    members,
    ownTeam,
    recipeLines,
    teamAnswers,
    teamWhy,
    trustState,
    type Analysis,
    type RepoTrust,
    type TeamChoices,
    type TeamEdit,
    type TeamStatus,
    type Trackers,
  } from "$lib/onboarding-logic";
  import { githubSlugs, gitlabRepos, projectId, type Card } from "$lib/setup-logic";
  import { app, load, setProject, shortPath, ui, type Project } from "$lib/state.svelte";
  import { added } from "$lib/team-logic";

  /** The last step: what TermF sets up, all kept by default, each line opened to adjust it; then it creates. */
  let {
    project,
    chosen,
    name,
    busy = $bindable(false),
    ready = $bindable(false),
  }: { project: Project | null; chosen: Card[]; name: string; busy?: boolean; ready?: boolean } = $props();

  type RoleChoice = { role: string; keep: boolean; why: string; detail: string[] };
  type Skill = { name: string; by: string[]; package: string | null };
  type Proposal = {
    choices: TeamChoices;
    splittable: boolean;
    roles: RoleChoice[];
    steps: string[];
    team: TeamEdit;
    skills: Skill[];
    trust: RepoTrust[];
  };
  type Team = Omit<Proposal, "team" | "skills" | "trust"> & { proposal: TeamEdit; status: Record<string, TeamStatus> };
  type Sources = { trackers: Trackers; githubRepos: string[] };
  type RecipeView = { saved: boolean; detected: unknown };

  let proposal = $state<Proposal | null>(null);
  let status = $state<Record<string, TeamStatus> | null>(null);
  let sources = $state<Sources | null>(null);
  let recipeView = $state<RecipeView | null>(null);
  let notifyNow = $state<Notify>("mine");
  let preview = $state("");
  let error = $state("");
  let open = $state<Record<string, boolean>>({});

  const paths = $derived(chosen.map((c) => c.path));
  const selection = $derived(paths.join("\n"));
  const detected = $derived(paths.map((p) => found[p]).filter((d) => !!d && typeof d !== "string"));
  const on = (row: Row) => !draft.off.includes(row);
  const toggle = (row: Row, value: boolean) => (draft.off = value ? draft.off.filter((r) => r !== row) : [...draft.off, row]);

  invoke<Notify>("ci_notify").then((n) => (notifyNow = n), () => {});
  const notify = $derived(draft.notify ?? notifyNow);

  // The proposal follows the repositories and the team's choices, a moment after the last change.
  $effect(() => {
    const asked = { repos: [...paths], choices: $state.snapshot(draft.choices) };
    const id = project?.id;
    const timer = setTimeout(async () => {
      try {
        const [d, team] = await Promise.all([
          invoke<Proposal>("setup_draft", asked),
          id ? invoke<Team>("onboarding_team", { project: id, choices: asked.choices }) : null,
        ]);
        if (JSON.stringify(asked.choices) !== JSON.stringify($state.snapshot(draft.choices))) return;
        proposal = team ? { ...d, choices: team.choices, splittable: team.splittable, roles: team.roles, team: team.proposal } : d;
        status = team?.status ?? null;
        error = "";
      } catch (e) {
        error = String(e);
      }
    }, untrack(() => proposal) ? 200 : 0);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    const id = project?.id;
    if (!id) return;
    Promise.all([invoke<Sources>("tickets_sources", { project: id }), invoke<RecipeView>("worktree_recipe", { project: id })]).then(
      ([s, r]) => ((sources = s), (recipeView = r)),
      (e) => (error = String(e)),
    );
  });

  /** What of the team gets written: all of it for a new project, else what is new or what TermF wrote and changed. */
  const teamWrite = $derived.by((): TeamEdit | null => {
    const edit = proposal?.team;
    if (!edit?.team || !on("team")) return null;
    if (!status) return edit;
    const fresh = (key: string) => status![key] === "new" || status![key] === "changed";
    const roles = Object.fromEntries(Object.entries(edit.roles).filter(([r]) => fresh(`roles.${r}`)));
    const write = { team: fresh("team") ? edit.team : null, workflow: fresh("workflow") ? edit.workflow : null, roles };
    return write.team || write.workflow || Object.keys(roles).length ? write : null;
  });
  const teamInPlace = $derived(!!status && !!proposal?.team.team && !teamWrite && on("team"));

  const github = $derived(githubSlugs(chosen));
  const gitlab = $derived(gitlabRepos(chosen));
  const ticketsSaved = $derived(!!sources && !sources.trackers.implicit);
  const trackers = $derived.by<Trackers | null>(() => {
    if ((!github.length && !gitlab) || ticketsSaved) return null;
    const none: Trackers = sources?.trackers ?? { github: null, jira: null, mcp: {}, implicit: false };
    const proposed = github.length ? githubIssues({ ...none, implicit: true }) : { ...none, implicit: false };
    return gitlab ? { ...proposed, gitlab: { host: gitlab.host, repo: null, mine: true, comment: false } } : proposed;
  });
  const keys = $derived([...new Set(detected.flatMap((d) => (typeof d === "string" ? [] : d.keys)))]);

  const recipe = $derived(
    recipeLines(Object.fromEntries(chosen.flatMap((c) => (typeof found[c.path] === "object" ? [[c.name, (found[c.path] as Detected).recipe]] : [])))),
  );
  const recipeSaved = $derived(!!recipeView?.saved);
  /** The recipe in words: what is installed with which tool, what is copied, what each worktree gets of its own. */
  const recipeWords = $derived.by(() => {
    const all = chosen.flatMap((c) => (typeof found[c.path] === "object" ? [(found[c.path] as Detected).recipe] : []));
    const tools = [...new Set(all.flatMap((r) => r.deps.map((d) => (d.install.at(-1) ?? d.dir).split(" ")[0])))];
    const files = [...new Set(all.flatMap((r) => r.include))];
    const ports = all.reduce((n, r) => n + r.ports.length, 0);
    return [
      tools.length ? t("cpRecipeDeps", { tools: tools.join(", ") }) : "",
      files.length ? t("cpRecipeCopy", { files: files.join(", ") }) : "",
      all.some((r) => r.database) ? t("cpRecipeDb") : "",
      ports ? t("cpRecipePorts", { count: ports }) : "",
    ].filter(Boolean);
  });
  const workflows = $derived(
    chosen.flatMap((c) => {
      const d = found[c.path];
      return typeof d === "object" && d.facts.workflows.length ? [`${c.name} (${d.facts.workflows.join(", ")})`] : [];
    }),
  );
  const workflowCount = $derived(chosen.reduce((n, c) => n + (typeof found[c.path] === "object" ? (found[c.path] as Detected).facts.workflows.length : 0), 0));

  /** Each repository's check command: typed, else the project's, else the test command found. */
  const checkOf = (c: Card) => {
    if (c.path in draft.checks) return draft.checks[c.path];
    if (project?.checks?.[c.path] !== undefined) return project.checks[c.path];
    const d = found[c.path];
    return typeof d === "object" ? (d.scripts.find((s) => s.kind === "test")?.run ?? "") : "";
  };
  const checks = $derived(on("checks") ? Object.fromEntries(chosen.flatMap((c) => (checkOf(c).trim() ? [[c.path, checkOf(c).trim()]] : []))) : {});
  const checksChanged = $derived(!!project && JSON.stringify(checks) !== JSON.stringify(project.checks ?? {}));

  const trusted = $derived(draft.trusted === selection);
  const allTrusted = $derived(!!proposal && trustState(proposal.trust) === "on");
  const skillsKept = $derived((proposal?.skills ?? []).filter((s) => !draft.skillsOff.includes(s.name)));

  $effect(() => {
    if (project || !name) return;
    const args = { name, repos: [...paths], team: teamWrite, trackers: on("tickets") ? trackers : null, checks };
    invoke<string>("setup_preview", args).then(
      (text) => (preview = text),
      (e) => (preview = String(e)),
    );
  });

  $effect(() => {
    ready = !!proposal && !!name && paths.length > 0 && paths.every((p) => found[p] !== undefined);
  });

  const why = (r: RoleChoice) => t(teamWhy[r.why as keyof typeof teamWhy] ?? "obTeamWhyOwn", { detail: r.detail.join(", ") });

  function teamSentence(edit: TeamEdit): string {
    const squads = edit.team?.squads ?? [];
    const staff = edit.team?.staff ?? [];
    const head =
      squads.length === 1
        ? t("cpTeamOne", { list: members(squads[0].roles).join(", ") })
        : t("cpTeamSquads", { count: squads.length, list: squads.map((s) => `${s.id} (${members(s.roles).join(", ")})`).join(", ") });
    return staff.length ? `${head} ${t("cpTeamStaff", { list: staff.join(", ") })}` : head;
  }

  const sizes = [1, 2, 3].map((n) => ({ id: String(n), label: String(n) }));
  const models = [
    { id: "", label: t("obTeamQOwn") },
    { id: "sonnet", label: t("obTeamQThrifty") },
    { id: "opus", label: t("obTeamQQuality") },
  ];
  const notifies: { id: Notify; label: string }[] = [
    { id: "mine", label: t("ciNotifyMine") },
    { id: "all", label: t("ciNotifyAll") },
    { id: "off", label: t("ciNotifyOff") },
  ];

  /**
   * Creates the project (or applies to the one that exists), then what was kept, with the assistant's commands and
   * its journal; the answers become the project's convention, whose rules are proposed in the Library afterwards.
   */
  export async function create() {
    if (busy || !ready || !proposal) return;
    busy = true;
    error = "";
    let id = project?.id ?? null;
    if (!id) {
      try {
        const before = app.workspace!.projects;
        const after = await invoke<Project[]>("project_save", { id: null, name, repos: paths, checks });
        app.workspace!.projects = after;
        id = added(before, after);
      } catch (e) {
        error = String(e);
      }
      if (!id) {
        error ||= t("setupNotCreated");
        busy = false;
        return;
      }
    }
    const projectId = id;
    // From here the project exists: a step that fails is reported, never a reason to create it again.
    const failed: string[] = [];
    const attempt = async (what: () => Promise<unknown>) => {
      try {
        await what();
      } catch (e) {
        failed.push(String(e));
      }
    };
    const decide = (what: string, decision: "accepted" | "rejected", detail = "") =>
      invoke("onboarding_decide", { project: projectId, proposal: what, decision, detail });
    if (!project) await attempt(() => invoke("onboarding_seen", { project: projectId }));
    else if (checksChanged)
      await attempt(async () => {
        app.workspace!.projects = await invoke<Project[]>("project_save", { id: projectId, name: project.name, repos: paths, checks });
      });
    const team = teamWrite;
    if (team) await attempt(() => invoke("onboarding_team_save", { project: projectId, edit: team, replace: false }));
    if (on("tickets") && trackers)
      await attempt(async () => {
        await invoke("tickets_sources_save", { project: projectId, trackers });
        await decide("tickets", "accepted", [...github, ...(gitlab?.slugs ?? [])].join(", "));
      });
    if (on("worktree") && !recipeSaved)
      await attempt(async () => {
        const view = await invoke<{ detected: unknown }>("worktree_recipe", { project: projectId });
        await invoke("worktree_recipe_save", { project: projectId, recipe: view.detected });
        await decide("worktree", "accepted", chosen.map((c) => c.name).join(", "));
      });
    if (workflows.length && on("ci"))
      await attempt(async () => {
        if (notify !== notifyNow) await invoke("ci_notify_set", { notify });
        await decide("ci", "accepted", notify);
      });
    for (const skill of draft.skillsOff.filter((s) => proposal!.skills.some((k) => k.name === s)))
      await attempt(async () => {
        await invoke("library_toggle", { kind: "skill", name: skill, project: projectId, enabled: false });
        await decide("skills", "rejected", `-${skill}`);
      });
    if (trusted && !allTrusted) await attempt(() => invoke("claude_trust_set", { project: projectId, on: true }));

    const choices = proposal.choices;
    // The draft is gone once the project exists: an analysis still running answers with what was chosen.
    const given = $state.snapshot(draft.answers);
    const settle = async (analysis: Analysis) => {
      await invoke("onboarding_adopt", { project: projectId, analysis });
      const own = Object.fromEntries(analysis.questions.filter((q) => !ownTeam(q)).map((q) => [q.id, given[q.id] ?? q.default]));
      const answers = { ...own, ...teamAnswers(analysis.questions, choices) };
      const edited = !!project && (await invoke<{ edited: boolean }>("onboarding_state", { project: projectId })).edited;
      await invoke("onboarding_answer", { project: projectId, answers, replace: edited });
      await invoke("onboarding_step", { project: projectId, step: "done" });
      // The rules and scripts the answers call for come in the Library, to review one by one.
      invoke("conventions_propose", { project: projectId, slug: "onboarding", answers: [] }).catch(() => {});
    };
    if (draft.analysis && draft.analyzed === selection) await attempt(() => settle($state.snapshot(draft.analysis!)));
    else if (runningFor(project?.id ?? null, paths))
      // Still reading: it lands on the project once done, even after the setup is left.
      analyze(project?.id ?? null, paths, lang).then(({ analysis }) => settle(analysis)).catch((e) => (app.error = String(e)));
    await attempt(() => invoke("onboarding_step", { project: projectId, step: "done" }));

    if (!project) await load();
    if (failed.length) app.error = t("setupPartial", { error: failed.join(" · ") });
    endNewProject();
    setProject(projectId);
    ui.view = "home";
    busy = false;
    toast(project ? t("cpApplied", { name: project.name }) : t("cpCreated", { name }), { kind: "ok" });
  }

  const id = $derived(project?.id ?? projectId(name, (app.workspace?.projects ?? []).map((p) => p.id)));
</script>

{#snippet row(key: string, Icon: Component, title: string, sentence: string, toggleRow?: Row, details?: Snippet)}
  {@const off = toggleRow ? !on(toggleRow) : false}
  <li class="row" class:off>
    <span class="icon"><Icon size={16} aria-hidden="true" /></span>
    <div class="what">
      <h3>{title}</h3>
      <p>{sentence}</p>
      {#if details && !off}
        <button class="more" aria-expanded={!!open[key]} onclick={() => (open[key] = !open[key])}>
          <ChevronRight size={13} aria-hidden="true" class="caret" />{t("cpAdjust")}
        </button>
        {#if open[key]}<div class="drawer">{@render details()}</div>{/if}
      {/if}
    </div>
    {#if toggleRow}
      <input
        type="checkbox"
        role="switch"
        class="switch"
        aria-label={title}
        checked={!off}
        onchange={(e) => toggle(toggleRow, e.currentTarget.checked)}
      />
    {/if}
  </li>
{/snippet}

{#snippet teamDetails()}
  {#if proposal}
    <div class="controls">
      <span class="control"><span aria-hidden="true">{t("obTeamSize")}</span><Segments
        label={t("obTeamSize")}
        options={sizes}
        selected={String(proposal.choices.size ?? 2)}
        onselect={(v) => (draft.choices = { ...draft.choices, size: Number(v) })}
      /></span>
      {#if proposal.splittable}
        <span class="control"><span aria-hidden="true">{t("obTeamSplit")}</span><Segments
          label={t("obTeamSplit")}
          options={[{ id: "one", label: t("obTeamOneSquad") }, { id: "split", label: t("obTeamPerPart") }]}
          selected={proposal.choices.split ? "split" : "one"}
          onselect={(v) => (draft.choices = { ...draft.choices, split: v === "split" })}
        /></span>
      {/if}
      <span class="control"><span aria-hidden="true">{t("obTeamModels")}</span><Segments
        label={t("obTeamModels")}
        options={models}
        selected={proposal.choices.model ?? ""}
        onselect={(v) => (draft.choices = { ...draft.choices, model: v })}
      /></span>
    </div>
    <ul class="roles">
      {#each proposal.roles as r (r.role)}
        <li>
          <Switch
            checked={r.keep}
            label={r.role}
            hint={why(r)}
            onchange={(keep) => (draft.choices = { ...draft.choices, keep: { ...draft.choices.keep, [r.role]: keep } })}
          />
        </li>
      {/each}
    </ul>
    {#if proposal.steps.length}<p class="note">{t("obTeamSteps", { roles: proposal.steps.join(", ") })}</p>{/if}
  {/if}
{/snippet}

{#snippet recipeDetails()}
  <ul class="lines">
    {#each recipe as line (line.repo)}
      <li>
        <b>{line.repo}</b>
        {#each [...new Set(line.parts)] as part (part)}<code>{part}</code>{:else}<span class="muted">{t("obRecipeNothing")}</span>{/each}
      </li>
    {/each}
  </ul>
{/snippet}

{#snippet checksDetails()}
  <div class="fields">
    {#each chosen as c (c.path)}
      <label class="check">
        <span>{c.name}</span>
        <input
          class="field mono"
          value={checkOf(c)}
          placeholder="npm test"
          spellcheck="false"
          autocomplete="off"
          oninput={(e) => (draft.checks = { ...draft.checks, [c.path]: e.currentTarget.value })}
        />
      </label>
    {/each}
    <p class="note">{t("recetteChecksHint")}</p>
  </div>
{/snippet}

{#snippet ticketsDetails()}
  <p class="note">{t("cpTicketsKeys", { keys: keys.map((k) => `${k}-…`).join(", ") })}</p>
{/snippet}

{#snippet ciDetails()}
  <p class="note">{t("obCiWorkflows", { list: workflows.join(" · ") })}</p>
  <span class="control"><span aria-hidden="true">{t("ciNotify")}</span><Segments label={t("ciNotify")} options={notifies} selected={notify} onselect={(v) => (draft.notify = v as Notify)} /></span>
{/snippet}

{#snippet skillsDetails()}
  <ul class="roles">
    {#each proposal?.skills ?? [] as s (s.name)}
      <li>
        <Switch
          checked={!draft.skillsOff.includes(s.name)}
          label={s.name}
          hint={s.package ?? s.by.join(", ")}
          onchange={(keep) => (draft.skillsOff = keep ? draft.skillsOff.filter((x) => x !== s.name) : [...draft.skillsOff, s.name])}
        />
      </li>
    {/each}
  </ul>
{/snippet}

{#snippet trustDetails()}
  <ul class="lines">
    {#each proposal?.trust ?? [] as r (r.path)}
      <li>
        <b>{r.name}</b>
        <span class="muted">
          {#if r.allow || r.mcp}
            {[r.allow ? t("obTrustRules", { count: r.allow }) : "", r.mcp ? t("obTrustMcp", { count: r.mcp }) : ""].filter(Boolean).join(" · ")}
          {:else}{t("setupTrustNothing")}{/if}
        </span>
        <span class="badge" class:ok={r.trusted}>{t(r.trusted ? "obTrusted" : "obUntrusted")}</span>
      </li>
    {/each}
  </ul>
{/snippet}

{#if error}<p class="notice error" role="alert">{error}</p>{/if}

{#if !proposal}
  <Skeleton shape="list" rows={6} label={t("setupProposing")} />
{:else}
  <ul class="rows">
    {#if proposal.team.team}
      {@render row(
        "team",
        Users,
        t("obTeamTitle"),
        teamInPlace ? t("obTeamSame") : teamSentence(proposal.team),
        teamInPlace ? undefined : "team",
        teamDetails,
      )}
    {/if}
    {#if ticketsSaved}
      {@render row("tickets", Ticket, t("obTicketsTitle"), t("obTicketsSaved", { sources: draft.analysis?.tickets.join(", ") || "—" }))}
    {:else if trackers}
      {@render row(
        "tickets",
        Ticket,
        t("obTicketsTitle"),
        t("cpTicketsSentence", { list: [...github, ...(gitlab?.slugs ?? [])].join(", ") }),
        "tickets",
        keys.length ? ticketsDetails : undefined,
      )}
    {/if}
    {#if recipeSaved}
      {@render row("worktree", FolderGit2, t("obRecipeTitle"), t("obRecipeAlready"))}
    {:else}
      {@render row(
        "worktree",
        FolderGit2,
        t("obRecipeTitle"),
        recipeWords.length ? t("cpRecipeSentence", { list: recipeWords.join(", ") }) : t("cpRecipeNothing"),
        "worktree",
        recipeDetails,
      )}
    {/if}
    {@render row(
      "checks",
      ListChecks,
      t("recetteChecks"),
      Object.keys(checks).length
        ? t("cpChecksSentence", { list: chosen.filter((c) => checks[c.path]).map((c) => t("cpCheckIn", { command: checks[c.path], repo: c.name })).join(", ") })
        : t("cpChecksNone"),
      "checks",
      checksDetails,
    )}
    {#if workflows.length}
      {@render row("ci", Workflow, t("obCiTitle"), t("cpCiSentence", { count: workflowCount, mode: notifies.find((n) => n.id === notify)!.label.toLowerCase() }), "ci", ciDetails)}
    {/if}
    {#if proposal.skills.length}
      {@render row(
        "skills",
        Layers,
        t("stackSkillsTitle"),
        t("cpSkillsSentence", { count: skillsKept.length, list: skillsKept.slice(0, 5).map((s) => s.name).join(", ") + (skillsKept.length > 5 ? "…" : "") }),
        undefined,
        skillsDetails,
      )}
    {/if}
    {@render row("conventions", LibraryBig, t("setupConvTitle"), t("cpConvSentence"))}
  </ul>

  <section class="trust" aria-labelledby="trust-title">
    <h3 id="trust-title"><ShieldCheck size={16} aria-hidden="true" />{t("obTrustTitle")}</h3>
    {#if allTrusted}
      <p class="muted">{t("setupTrustAll")}</p>
    {:else}
      <label class="agree">
        <input type="checkbox" checked={trusted} onchange={(e) => (draft.trusted = e.currentTarget.checked ? selection : "")} />
        <span><b>{t("obTrustConfirm")}</b><span class="muted">{t("obTrustNote")}</span></span>
      </label>
    {/if}
    <details>
      <summary><ChevronRight size={13} aria-hidden="true" class="caret" />{t("cpTrustLoads")}</summary>
      {@render trustDetails()}
    </details>
  </section>

  <section class="then">
    {#if !project}
      <details class="file">
        <summary><FileCode size={14} aria-hidden="true" />{t("cpSeeFile", { path: shortPath(`${app.workspace?.configDir ?? ""}/projects/${id}.toml`) })}</summary>
        <pre><code>{preview}</code></pre>
      </details>
    {/if}
    <p class="muted">{t("cpWritesNothing")}</p>
  </section>
{/if}

<style>
  .rows {
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  .row {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    padding: 16px 18px;
  }

  .row + .row {
    border-top: 1px solid var(--border);
  }

  .icon {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    background: var(--well);
    color: var(--soft);
  }

  .what {
    flex: 1;
    min-width: 0;
  }

  .what h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .what p {
    margin: 4px 0 0;
    color: var(--soft);
    line-height: 1.5;
  }

  .row.off .what p,
  .row.off .icon {
    opacity: 0.55;
  }

  .row .switch {
    flex-shrink: 0;
    margin-top: 6px;
  }

  .more,
  summary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    list-style: none;
  }

  summary::-webkit-details-marker {
    display: none;
  }

  .more :global(.caret),
  summary :global(.caret) {
    transition: transform var(--dur-fast) var(--ease-out);
  }

  .more[aria-expanded="true"] :global(.caret),
  details[open] summary :global(.caret) {
    transform: rotate(90deg);
  }

  .drawer {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin-top: 12px;
    padding: 14px 16px;
    border-radius: var(--radius-md);
    background: var(--well);
  }

  .controls {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .control {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
  }

  .roles {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .lines {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
  }

  .lines li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
  }

  .drawer .lines {
    margin: 0;
  }

  .fields {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .check {
    display: grid;
    grid-template-columns: 140px minmax(0, 1fr);
    align-items: center;
    gap: 12px;
  }

  .check span {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .trust {
    margin-top: 16px;
    padding: 16px 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .trust h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 10px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .trust h3 :global(svg) {
    color: var(--soft);
  }

  .agree {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    cursor: pointer;
  }

  .agree input {
    width: 16px;
    height: 16px;
    margin: 2px 0 0;
    accent-color: var(--accent);
  }

  .agree span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .agree b {
    font-weight: 500;
  }

  .then {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 16px;
  }

  .then p {
    margin: 0;
  }

  .file summary {
    margin: 0;
  }

  pre {
    max-height: 320px;
    margin: 10px 0 0;
    padding: 12px 14px;
    overflow: auto;
    border-radius: var(--radius-md);
    background: var(--well);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    line-height: 1.5;
  }
</style>
