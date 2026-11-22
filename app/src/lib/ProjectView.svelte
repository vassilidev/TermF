<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import LibraryBig from "@lucide/svelte/icons/library-big";
  import Plus from "@lucide/svelte/icons/plus";
  import Search from "@lucide/svelte/icons/search";
  import X from "@lucide/svelte/icons/x";
  import ClaudeTrust from "$lib/ClaudeTrust.svelte";
  import MemorySettings from "$lib/MemorySettings.svelte";
  import OnboardingJournal from "$lib/OnboardingJournal.svelte";
  import StackOffers from "$lib/StackOffers.svelte";
  import TeamDecisions from "$lib/TeamDecisions.svelte";
  import TeamRoles from "$lib/TeamRoles.svelte";
  import TeamSquads from "$lib/TeamSquads.svelte";
  import TicketSources from "$lib/TicketSources.svelte";
  import { openSetup } from "$lib/onboarding.svelte";
  import { steps, type Step } from "$lib/onboarding-logic";
  import Banner from "$lib/ui/Banner.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import SectionNav from "$lib/ui/SectionNav.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { lang, t } from "$lib/i18n";
  import { library, showLibrary, type RepoStack } from "$lib/library.svelte";
  import { hasWords } from "$lib/nav-logic";
  import { closeProject, config, configSections, type ConfigSection } from "$lib/project.svelte";
  import { app, held, repoName, setProject, shortPath, ui, type Project } from "$lib/state.svelte";
  import { added } from "$lib/team-logic";

  type Log = { at: number; what: string; decision?: "accepted" | "rejected"; detail: string };

  // Mounted per project (keyed on `ui.editing`), so the project only seeds the form; "" is a new project.
  const id = ui.editing ?? "";
  const seed = app.workspace?.projects.find((p) => p.id === id);
  const project = $derived(app.workspace?.projects.find((p) => p.id === id));
  let name = $state(seed?.name ?? "");
  let chosen = $state(seed?.repos.map((r) => r.path) ?? []);
  let filter = $state("");
  let adding = $state(false);
  let addButton = $state<HTMLButtonElement>();
  let confirming = $state(false);
  let asked = false;
  let busy = $state(false);
  let error = $state("");
  let query = $state("");
  let sources = $state<TicketSources>();
  /** Bumped to read the ticket sources again, which drops their changes. */
  let round = $state(0);
  let recipe = $state<{ saved: boolean } | null>(null);
  let setup = $state<{ step: Step; log: Log[] } | null>(null);
  let question = $state<HTMLDialogElement>();
  /** What was held back while the form had changes, until the user answers: leave, or stay (and how). */
  let pending = $state<{ leave: () => void; stay?: () => void }>();
  /** Set once the user chose to leave: nothing asks again on the way out. */
  let released = false;
  let savedForm = $state(JSON.stringify([seed?.name ?? "", seed?.repos.map((r) => r.path) ?? []]));
  const dirty = $derived(JSON.stringify([name, chosen]) !== savedForm || !!sources?.dirty());
  // An id that names no project (deleted, or gone after a reload) never shows a form that would create a copy.
  const missing = $derived(!!id && !project);

  /** What depends on the repositories, read again once they are saved. The Worktrees tab tells why a recipe cannot be read. */
  let stacks = $state<RepoStack[]>([]);
  function readRepos() {
    invoke<{ saved: boolean }>("worktree_recipe", { project: id }).then((r) => (recipe = r), () => {});
    invoke<RepoStack[]>("library_stack", { project: id }).then((s) => (stacks = s), () => {});
  }
  if (seed) {
    readRepos();
    invoke<{ step: Step; log: Log[] }>("onboarding_state", { project: id }).then((s) => (setup = s), () => {});
  }

  // A new project has its name and repositories; under every project, the global library too.
  const sections = $derived(
    project ? configSections : configSections.filter((s) => s.id === "project" || (s.id === "library" && app.project === null)),
  );
  const found = $derived(
    sections.filter((s) => hasWords([s.id === "project" && !project ? "newProject" : s.label, ...s.words].map((k) => t(k)).join(" "), query)),
  );
  /** A new project's page keeps its own section: the shared one is the projects'. */
  let own = $state<ConfigSection>("project");
  const current = $derived.by(() => {
    const list = found.length ? found : sections;
    const wanted = id ? config.section : own;
    return (list.find((s) => s.id === wanted) ?? list[0]).id;
  });

  function select(section: ConfigSection) {
    if (id) config.section = section;
    else own = section;
  }

  // A search that hides the section shown opens the first one it found.
  $effect(() => {
    if (query.trim() && found.length) select(current);
  });

  /**
   * While the form has changes, holds back `leave` (whatever closes the page or takes it off screen, or follows
   * another project) and asks; `stay` runs if the user stays.
   */
  function ask(leave: () => void, stay?: () => void): boolean {
    return untrack(() => {
      if (released || !dirty) return false;
      if (!pending) {
        pending = { leave, stay };
        question?.showModal();
      } else if (stay) {
        // Asked twice at once (a run of another project opened): staying undoes both.
        const first = pending.stay;
        pending.stay = () => (first?.(), stay());
      }
      return true;
    });
  }

  // The single way out of the page: the navigation, the panels, Escape and the links all ask it first.
  $effect(() => {
    const page = { view: "project" as const, hold: (leave: () => void) => ask(leave) };
    held.page = page;
    return () => {
      if (held.page === page) held.page = null;
    };
  });

  async function answer(choice: "stay" | "discard" | "save") {
    const asked = pending;
    if (!asked) return;
    const saved = choice === "save" ? await persist() : null;
    pending = undefined;
    question?.close();
    if (choice === "stay" || saved === false) return asked.stay?.();
    released = true;
    if (saved) setProject(saved);
    asked.leave();
  }

  /** The current project when the page was last in step with it: a new project's page follows it while untouched. */
  let shownFor = app.project;

  function follow(to: string | null) {
    shownFor = to;
    ui.editing = to ?? "";
  }

  // The page is the current project's: switching projects switches the page.
  $effect(() => {
    if (missing) {
      released = true;
      return closeProject();
    }
    const to = app.project;
    if (to === (id || shownFor)) return;
    // A project being created stays while the user looks at another one.
    if (!id && untrack(() => dirty)) return;
    if (!ask(() => follow(to), () => setProject(id))) follow(to);
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented || e.isComposing || adding || pending || ui.palette || ui.help) return;
    // Beside another view, only while the keyboard is in its panel.
    if (ui.view !== "project") return;
    // Escape typed in a terminal (the quick terminal over the page) is the terminal's.
    if ((e.target as Element | null)?.closest?.(".xterm")) return;
    e.preventDefault();
    closeProject();
  }

  // The project's own repositories first, even if discovery no longer finds them.
  const candidates = $derived.by(() => {
    const own = seed?.repos ?? [];
    const others = (app.workspace?.repos ?? [])
      .filter((r) => !own.some((o) => o.path === r.path))
      .sort((a, b) => a.name.localeCompare(b.name, lang, { sensitivity: "base" }));
    const q = filter.toLowerCase();
    return [...own, ...others].filter((r) => r.path.toLowerCase().includes(q));
  });

  function pick(path: string, on: boolean) {
    chosen = on ? [...chosen, path] : chosen.filter((p) => p !== path);
  }

  /** Saves the form: the id of a project it created, null for this one, false when it failed. */
  async function persist(): Promise<string | null | false> {
    if (!name.trim() || busy) return false;
    busy = true;
    error = "";
    try {
      const before = app.workspace!.projects;
      const after = await invoke<Project[]>("project_save", { id: id || null, name, repos: chosen });
      app.workspace!.projects = after;
      savedForm = JSON.stringify([name, chosen]);
      await sources?.save();
      if (id) readRepos();
      return id ? null : added(before, after);
    } catch (e) {
      error = String(e);
      return false;
    } finally {
      busy = false;
    }
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    const created = await persist();
    if (created) {
      ui.editing = created;
      setProject(created);
    }
  }

  /** A new project's page closes; a project's form goes back to what is saved. */
  function discard() {
    if (!id) {
      released = true;
      return closeProject();
    }
    [name, chosen] = JSON.parse(savedForm);
    round++;
  }

  async function remove() {
    busy = true;
    error = "";
    try {
      app.workspace!.projects = await invoke<Project[]>("project_delete", { id });
      released = true;
      closeProject();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function openLibrary(tab?: typeof library.tab) {
    Object.assign(library, { global: false, panel: null }, tab ? { tab } : {});
    showLibrary();
  }

  const stepLabel = {
    analysis: "obStepAnalysis",
    questions: "obStepQuestions",
    proposals: "obStepProposals",
    trust: "obStepTrust",
    done: "obStepDone",
  } as const;
</script>

<svelte:window {onkeydown} />

{#if !missing}
  <div class="config">
    <ViewHeader title={t("ux3t3Config")} context={project?.name ?? t(app.project === null ? "ticketsAll" : "newProject")}>
      {#snippet actions()}
        {#if dirty}
          <button class="btn sm" onclick={discard}>{t("cancel")}</button>
          <button type="submit" form="project-form" class="btn sm primary" disabled={!name.trim() || busy}>
            {t("save")}
          </button>
        {/if}
      {/snippet}
    </ViewHeader>

    {#if error}
      <div class="banners">
        <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
      </div>
    {/if}

    <div class="body">
      {#snippet search()}
        <div class="search">
          <Search size={14} aria-hidden="true" />
          <input
            class="field"
            type="search"
            placeholder={t("navSearch")}
            aria-label={t("navSearch")}
            bind:value={query}
            onkeydown={(e) => {
              if (e.key === "Escape" && query) {
                e.preventDefault();
                query = "";
              }
            }}
          />
        </div>
      {/snippet}
      <SectionNav
        items={found.map((s) => ({
          id: s.id,
          label: t(s.id === "project" && !project ? "newProject" : s.label),
          icon: s.icon,
          count: s.id === "project" && chosen.length ? chosen.length : null,
          apart: s.id === "delete",
          danger: s.id === "delete",
        }))}
        {current}
        label={t("ux3t3Config")}
        onselect={select}
        top={search}
        empty={t("filesNoResults")}
      />

      <div class="page">
        <section class="sec" hidden={current !== "project"} aria-labelledby="config-project">
          <h2 id="config-project">{t(project ? "runMetaProject" : "newProject")}</h2>
          <form id="project-form" class="form" onsubmit={save}>
            <div>
              <h3><label for="project-name">{t("projectName")}</label></h3>
              <input
                id="project-name"
                class="field name"
                bind:value={name}
                autocomplete="off"
                spellcheck="false"
                {@attach (el) => {
                  if (!id) el.focus();
                }}
              />
            </div>

            <div>
              <h3>{t("repos")} <small>{chosen.length}</small></h3>
              <ul class="repos">
                {#each chosen as path (path)}
                  <li>
                    <GitBranch size={14} aria-hidden="true" />
                    <b>{repoName(path)}</b>
                    <span class="mono">{shortPath(path)}</span>
                    <button
                      type="button"
                      class="icon-btn sm"
                      aria-label={t("t5RemoveRepo", { name: repoName(path) })}
                      title={t("t5RemoveRepo", { name: repoName(path) })}
                      onclick={() => pick(path, false)}
                    >
                      <X size={14} aria-hidden="true" />
                    </button>
                  </li>
                {/each}
                <li>
                  <button
                    type="button"
                    class="add"
                    bind:this={addButton}
                    aria-haspopup="dialog"
                    aria-expanded={adding}
                    onclick={() => (adding = !adding)}
                  >
                    <Plus size={14} aria-hidden="true" />{t("t5AddRepos")}
                  </button>
                </li>
              </ul>
            </div>
          </form>

          {#if stacks.some((s) => s.mcp.length || s.sources.length)}
            <div class="offers">
              <h3>{t("stackOffersTitle")}</h3>
              <StackOffers {stacks} />
            </div>
          {/if}
        </section>

        <section class="sec" hidden={current !== "library"} aria-labelledby="config-library">
          <h2 id="config-library">{t("viewLibrary")}</h2>
          <p class="help">{t("t6bHelpLibrary")}</p>
          <div>
            <button type="button" class="btn" onclick={() => openLibrary()}>
              <LibraryBig size={14} aria-hidden="true" />{t("showLibrary")}
            </button>
          </div>
        </section>

        {#if project}
          <section class="sec" hidden={current !== "setup"} aria-labelledby="config-setup">
            <h2 id="config-setup">{t("ux3t3Setup")}</h2>
            <p class="help">{t("obConfigureHint")}</p>
            {#if setup}
              <ol class="steps">
                {#each steps.slice(0, 4) as step, i (step)}
                  {@const past = steps.indexOf(step) < steps.indexOf(setup.step)}
                  <li aria-current={step === setup.step ? "step" : undefined}>
                    <span class="num" class:past>
                      {#if past}<Check size={12} aria-hidden="true" />{:else}{i + 1}{/if}
                    </span>
                    {t(stepLabel[step])}
                  </li>
                {/each}
              </ol>
            {/if}
            <div>
              <button
                type="button"
                class="btn"
                class:primary={!!setup && setup.step !== "done" && !dirty}
                onclick={() => openSetup(id)}
              >
                {t("obConfigure")}
              </button>
            </div>
            {#if setup?.log.length}
              <div class="journal">
                <h3>{t("obJournal")}</h3>
                <OnboardingJournal log={setup.log} />
              </div>
            {/if}
          </section>

          <section class="sec" hidden={current !== "tickets"}>
            {#key round}
              <TicketSources project={id} bind:this={sources} />
            {/key}
          </section>

          <section class="sec" hidden={current !== "team"} aria-labelledby="config-team">
            <h2 id="config-team">{t("t5Team")}</h2>
            <p class="help">{t("t6bHelpTeam")}</p>
            <TeamSquads project={id} />
            <TeamRoles project={id} />
            <TeamDecisions project={id} />
          </section>

          <section class="sec" hidden={current !== "worktrees"} aria-labelledby="config-worktrees">
            <h2 id="config-worktrees">{t("wtTitle")}</h2>
            <p class="help">{t("wtIntro")}</p>
            <div class="line">
              <button type="button" class="btn sm" onclick={() => openLibrary("worktrees")}>{t("t5RecipeEdit")}</button>
              {#if recipe}<span class="muted">{recipe.saved ? t("wtSaved") : t("wtDetected")}</span>{/if}
            </div>
          </section>

          <section class="sec flow" hidden={current !== "memory"}>
            <MemorySettings project={id} />
          </section>

          <section class="sec" hidden={current !== "trust"} aria-labelledby="config-trust">
            <h2 id="config-trust">{t("obTrustTitle")}</h2>
            <ClaudeTrust project={id} />
          </section>

          <section class="sec" hidden={current !== "delete"} aria-labelledby="config-delete">
            <h2 id="config-delete">{t("t5DeleteProject")}</h2>
            {#if confirming}
              <div class="danger-zone">
                <p role="alert">{t("confirmDeleteProject", { name: project.name })}</p>
                <button type="button" class="btn sm" onclick={() => (confirming = false)} {@attach (el) => el.focus()}>
                  {t("cancel")}
                </button>
                <button type="button" class="btn sm danger" disabled={busy} onclick={remove}>{t("delete")}</button>
              </div>
            {:else}
              <div>
                <button
                  type="button"
                  class="btn sm delete"
                  onclick={() => ((asked = true), (confirming = true))}
                  {@attach (el) => {
                    if (asked) el.focus();
                  }}
                >
                  {t("delete")}
                </button>
              </div>
            {/if}
          </section>
        {/if}
      </div>
    </div>
  </div>

  <Popover bind:open={adding} trigger={addButton} label={t("t5AddRepos")} width={440}>
    <div class="filter">
      <Search size={14} aria-hidden="true" />
      <input type="search" bind:value={filter} placeholder={t("filter")} aria-label={t("filter")} spellcheck="false" />
    </div>
    <ul class="candidates">
      {#each candidates as repo (repo.path)}
        <li>
          <label>
            <input
              type="checkbox"
              checked={chosen.includes(repo.path)}
              onchange={(e) => pick(repo.path, e.currentTarget.checked)}
            />
            <span class="name">{repo.name}</span>
            <span class="mono">{shortPath(repo.path)}</span>
          </label>
        </li>
      {:else}
        <li class="none">{t("noRepos")}</li>
      {/each}
    </ul>
  </Popover>
{/if}

<dialog bind:this={question} class="question" oncancel={() => answer("stay")}>
  <p>{t("ux3t3Unsaved", { name: project?.name ?? t("newProject") })}</p>
  <div class="answers">
    <button class="btn" onclick={() => answer("stay")}>{t("cancel")}</button>
    <button class="btn delete" onclick={() => answer("discard")}>{t("ux3t3Discard")}</button>
    <button class="btn primary" disabled={!name.trim() || busy} onclick={() => answer("save")}>{t("save")}</button>
  </div>
</dialog>

<style>
  .config {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }

  .body {
    display: grid;
    flex: 1;
    grid-template: minmax(0, 1fr) / 224px minmax(0, 1fr);
    min-height: 0;
  }

  .search {
    position: relative;
    color: var(--muted);
  }

  .search :global(svg) {
    position: absolute;
    top: 8px;
    left: 10px;
    pointer-events: none;
  }

  .search .field {
    padding-left: 30px;
  }

  .page {
    min-width: 0;
    min-height: 0;
    padding: 20px 28px 32px;
    overflow-y: auto;
  }

  .sec {
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 760px;
  }

  /* The memory brings its own spacing. */
  .sec.flow {
    display: block;
  }

  .sec[hidden] {
    display: none;
  }

  /* Each section's title, also the one the ticket sources bring. */
  h2,
  .sec > :global(.sources > h2) {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  /* A title's description sits close under it; the trust panel starts with its own. */
  h2 + .help,
  h2 + :global(.trust) {
    margin-top: -10px;
  }

  h3 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0 0 8px;
    font-size: var(--text-md);
    font-weight: 650;
  }

  h3 small {
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 400;
  }

  .help {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.45;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .name {
    max-width: 360px;
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
    min-height: 42px;
    padding: 0 8px 0 14px;
    color: var(--muted);
  }

  .repos li + li {
    border-top: 1px solid var(--border);
  }

  .repos b {
    color: var(--text);
    font-weight: 500;
  }

  .repos .mono {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .repos li:last-child {
    padding: 0;
  }

  .add {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 42px;
    padding: 0 14px;
    border: 0;
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
    background: none;
    color: var(--accent-ink);
    font-weight: 500;
    text-align: left;
  }

  .repos li:only-child .add {
    border-radius: var(--radius-lg);
  }

  .add:hover,
  .add[aria-expanded="true"] {
    background: var(--hover);
  }

  .offers {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .steps {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .steps li {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-weight: 500;
  }

  .steps li[aria-current="step"] {
    color: var(--accent-ink);
  }

  .num {
    display: inline-grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .num.past {
    border-color: var(--done);
    background: var(--done-soft);
    color: var(--done);
  }

  [aria-current="step"] .num {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
  }

  .journal {
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  .line {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .danger-zone {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    padding: 12px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .danger-zone p {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .delete {
    color: var(--danger);
  }

  .filter {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    height: 40px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
  }

  .filter input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    background: none;
    color: var(--text);
  }

  .filter input:focus-visible {
    outline: none;
  }

  .candidates {
    margin: 0;
    padding: 6px;
    list-style: none;
  }

  .candidates label {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 34px;
    padding: 0 8px;
    border-radius: var(--radius);
    cursor: pointer;
  }

  .candidates label:hover {
    background: var(--hover);
  }

  .candidates input {
    margin: 0;
  }

  .candidates .name {
    flex-shrink: 0;
    font-weight: 500;
  }

  .candidates .mono {
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .none {
    padding: 8px;
    color: var(--muted);
  }

  .question {
    max-width: 420px;
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--shadow-3);
  }

  .question::backdrop {
    background: rgb(10 12 20 / 0.28);
  }

  .question p {
    margin: 0 0 16px;
    overflow-wrap: anywhere;
  }

  .answers {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
