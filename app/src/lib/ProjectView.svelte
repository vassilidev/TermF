<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import Plus from "@lucide/svelte/icons/plus";
  import Search from "@lucide/svelte/icons/search";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import X from "@lucide/svelte/icons/x";
  import ClaudeTrust from "$lib/ClaudeTrust.svelte";
  import LibraryView from "$lib/LibraryView.svelte";
  import MemorySettings from "$lib/MemorySettings.svelte";
  import ProjectConnectors from "$lib/ProjectConnectors.svelte";
  import { initial, mark } from "$lib/ProjectSwitcher.svelte";
  import OnboardingJournal from "$lib/OnboardingJournal.svelte";
  import RepoCard from "$lib/RepoCard.svelte";
  import RemoteSettings from "$lib/RemoteSettings.svelte";
  import RepoPicker from "$lib/RepoPicker.svelte";
  import StackOffers from "$lib/StackOffers.svelte";
  import TeamDecisions from "$lib/TeamDecisions.svelte";
  import TeamReadFreely from "$lib/TeamReadFreely.svelte";
  import TeamRoles from "$lib/TeamRoles.svelte";
  import TeamSquads from "$lib/TeamSquads.svelte";
  import TicketSources from "$lib/TicketSources.svelte";
  import WorktreeSetup from "$lib/WorktreeSetup.svelte";
  import { openSetup } from "$lib/onboarding.svelte";
  import { steps, type Step } from "$lib/onboarding-logic";
  import Banner from "$lib/ui/Banner.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import SectionNav from "$lib/ui/SectionNav.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { provide } from "$lib/ask.svelte";
  import { t } from "$lib/i18n";
  import type { RepoStack } from "$lib/library.svelte";
  import { hasWords } from "$lib/nav-logic";
  import { cards, forgetCards, readCards } from "$lib/newproject.svelte";
  import { closeProject, config, configSections, type ConfigSection } from "$lib/project.svelte";
  import { app, held, setProject, ui, type Project } from "$lib/state.svelte";

  type Log = { at: number; what: string; decision?: "accepted" | "rejected"; detail: string };

  // Mounted per project (keyed on `ui.editing`), so the project only seeds the form; "" is every project's page.
  const id = ui.editing ?? "";
  const seed = app.workspace?.projects.find((p) => p.id === id);
  const project = $derived(app.workspace?.projects.find((p) => p.id === id));
  let name = $state(seed?.name ?? "");
  /** Its mark's color (`#rrggbb`) and icon, "" for none. */
  let color = $state(seed?.look?.color ?? "");
  let icon = $state(seed?.look?.icon ?? "");
  const look = $derived({ color: color || null, icon: icon.trim() || null });
  const preview = $derived(mark({ name: name || "?", look }));
  let chosen = $state(seed?.repos.map((r) => r.path) ?? []);
  /** Each repository's check command, by path: what the team runs before delivering. */
  let checks = $state<Record<string, string>>({ ...(seed?.checks ?? {}) });
  let adding = $state(false);
  let confirming = $state(false);
  let busy = $state(false);
  let error = $state("");
  let query = $state("");
  let sources = $state<TicketSources>();
  /** Bumped to read the ticket sources again, which drops their changes. */
  let round = $state(0);
  /** null while it is read, undefined if it could not be. */
  let setup = $state<{ step: Step; log: Log[] } | null | undefined>(null);
  let question = $state<HTMLDialogElement>();
  /** What was held back while the form had changes, until the user answers: leave, or stay (and how). */
  let pending = $state<{ leave: () => void; stay?: () => void }>();
  /** Set once the user chose to leave: nothing asks again on the way out. */
  let released = false;
  const form = () => JSON.stringify([name, chosen, chosen.map((p) => checks[p]?.trim() ?? ""), color, icon.trim()]);
  let savedForm = $state(form());
  const dirty = $derived(form() !== savedForm || !!sources?.dirty());
  // An id that names no project (deleted, or gone after a reload) never shows a form that would create a copy.
  const missing = $derived(!!id && !project);

  /** What depends on the repositories, read again once they are saved. */
  let stacks = $state<RepoStack[]>([]);
  function readRepos() {
    invoke<RepoStack[]>("library_stack", { project: id }).then((s) => (stacks = s), () => {});
  }
  /** The cards could not be read: without this, they would show their placeholders forever. */
  let unread = $state(false);
  if (seed) {
    forgetCards();
    readCards(seed.repos.map((r) => r.path)).catch(() => (unread = true));
    readRepos();
    invoke<{ step: Step; log: Log[] }>("onboarding_state", { project: id }).then((s) => (setup = s), () => (setup = undefined));
  }

  // Under every project, the global library; a new project is created by its setup.
  const sections = $derived(project ? configSections : configSections.filter((s) => s.id === "library"));
  const found = $derived(sections.filter((s) => hasWords([s.label, ...s.words].map((k) => t(k)).join(" "), query)));
  const current = $derived.by(() => {
    const list = found.length ? found : sections;
    return (list.find((s) => s.id === config.section) ?? list[0]).id;
  });

  function select(section: ConfigSection) {
    if (id) config.section = section;
  }

  // ⌘I: the section open goes with the question.
  $effect(() => {
    const open = sections.find((s) => s.id === current);
    return provide("project", { chips: open ? [{ id: "config-section", kind: "object", label: t(open.label) }] : [] });
  });

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
    const saved = choice === "save" ? await persist() : true;
    pending = undefined;
    question?.close();
    if (choice === "stay" || !saved) return asked.stay?.();
    released = true;
    asked.leave();
  }

  /** The current project when the page was last in step with it. */
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
    if (!ask(() => follow(to), () => setProject(id))) follow(to);
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented || e.isComposing || pending || ui.palette || ui.help) return;
    // Beside another view, only while the keyboard is in its panel.
    if (ui.view !== "project") return;
    // Escape typed in a terminal (the quick terminal over the page) is the terminal's.
    if ((e.target as Element | null)?.closest?.(".xterm")) return;
    e.preventDefault();
    if (adding) adding = false;
    else closeProject();
  }

  /** A repository as the setup shows it: what the project or discovery knows, then its card once read. */
  function cardOf(path: string) {
    const known = [...(seed?.repos ?? []), ...(app.workspace?.repos ?? [])].find((r) => r.path === path);
    const base = known ?? { name: path.split(/[\\/]/).pop() ?? path, path, modified: null };
    return { ...base, ...(cards[path] ?? (unread ? { stack: null } : {})) };
  }

  const others = (path: string) =>
    (app.workspace?.projects ?? []).filter((p) => p.id !== id && p.repos.some((r) => r.path === path)).map((p) => p.name);

  /** Saves the form: false when it failed. */
  async function persist(): Promise<boolean> {
    if (!id || !name.trim() || busy) return false;
    busy = true;
    error = "";
    try {
      const asked = Object.fromEntries(chosen.map((p) => [p, checks[p] ?? ""]));
      app.workspace!.projects = await invoke<Project[]>("project_save", { id, name, repos: chosen, checks: asked, look });
      savedForm = form();
      await sources?.save();
      readRepos();
      return true;
    } catch (e) {
      error = String(e);
      return false;
    } finally {
      busy = false;
    }
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (await persist()) adding = false;
  }

  /** The form goes back to what is saved. */
  function discard() {
    const [n, c, k, col, ic] = JSON.parse(savedForm) as [string, string[], string[], string, string];
    [name, chosen, checks, color, icon] = [n, c, Object.fromEntries(c.map((p, i) => [p, k[i]])), col, ic];
    adding = false;
    round++;
  }

  async function remove() {
    busy = true;
    error = "";
    try {
      const gone = project?.name ?? id;
      app.workspace!.projects = await invoke<Project[]>("project_delete", { id });
      released = true;
      closeProject();
      toast(t("restConfigDeleted", { name: gone }));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
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
    <ViewHeader title={t("ux3t3Config")} context={project?.name ?? t("ticketsAll")}>
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
          label: t(s.label),
          icon: s.icon,
          count: s.id === "project" && chosen.length ? chosen.length : null,
        }))}
        {current}
        label={t("ux3t3Config")}
        onselect={select}
        top={search}
        empty={t("filesNoResults")}
      />

      <div class="page" class:full={current === "library"}>
        <section class="sec" hidden={current !== "project"} aria-labelledby="config-project">
          <h2 id="config-project">{t("runMetaProject")}</h2>
          <form id="project-form" class="form" onsubmit={save}>
            <div>
              <h3><label for="project-name">{t("projectName")}</label></h3>
              <input
                id="project-name"
                class="field name"
                bind:value={name}
                autocomplete="off"
                spellcheck="false"
              />
            </div>

            <div>
              <h3>{t("projectLook")}</h3>
              <div class="look">
                <span class="mark" style={preview.style} aria-hidden="true">{preview.text}</span>
                <label class="labelled">
                  <span>{t("projectColor")}</span>
                  <!-- Without a color, the picker opens on the accent the mark has by default. -->
                  <input class="swatch" type="color" value={color || "#4446d6"} oninput={(e) => (color = e.currentTarget.value)} />
                </label>
                {#if color}
                  <button type="button" class="btn sm quiet" onclick={() => (color = "")}>{t("projectColorNone")}</button>
                {/if}
                <label class="labelled">
                  <span>{t("projectIcon")}</span>
                  <input class="field icon" bind:value={icon} maxlength="8" placeholder={initial(name || "?")} autocomplete="off" spellcheck="false" />
                </label>
              </div>
            </div>

            <div>
              <h3>{t("repos")} <small>{chosen.length}</small></h3>
              <ul class="repos">
                {#each chosen as path (path)}
                  {@const card = cardOf(path)}
                  <li>
                    <RepoCard {card} elsewhere={others(path)}>
                      {#snippet trailing()}
                        <button
                          type="button"
                          class="icon-btn"
                          aria-label={t("t5RemoveRepo", { name: card.name })}
                          title={t("t5RemoveRepo", { name: card.name })}
                          onclick={() => (chosen = chosen.filter((p) => p !== path))}
                        >
                          <X size={14} aria-hidden="true" />
                        </button>
                      {/snippet}
                    </RepoCard>
                    <label class="checks">
                      <span>{t("recetteChecks")}</span>
                      <input
                        class="field mono"
                        bind:value={checks[path]}
                        aria-describedby="checks-hint"
                        autocomplete="off"
                        spellcheck="false"
                      />
                    </label>
                  </li>
                {/each}
                <li>
                  <button type="button" class="add" aria-expanded={adding} aria-controls="config-add" onclick={() => (adding = !adding)}>
                    <Plus size={14} aria-hidden="true" />{t("t5AddRepos")}
                  </button>
                </li>
              </ul>
              <p id="checks-hint" class="checks-hint">{t("recetteChecksHint")}</p>
            </div>
          </form>
          <!-- Outside the form: Enter in its search must not save the project. -->
          {#if adding}
            <div id="config-add" class="adding">
              <RepoPicker bind:chosen project={id} autofocus />
            </div>
          {/if}

          {#if stacks.some((s) => s.mcp.length || s.sources.length || s.servers.length)}
            <div class="offers">
              <h3>{t("stackOffersTitle")}</h3>
              <StackOffers {stacks} project={id} />
            </div>
          {/if}

          {#if project}
            <section class="danger-zone" aria-labelledby="config-danger">
              <h3 id="config-danger">{t("restConfigDanger")}</h3>
              <div class="danger-row">
                <div class="danger-text">
                  <strong>{t("t5DeleteProject")}</strong>
                  <p>{t("restConfigDeleteLost", { name: project.name })}</p>
                  <p class="kept">{t("restConfigDeleteKept")}</p>
                </div>
                <button type="button" class="btn sm quiet danger" disabled={busy} onclick={() => (confirming = true)}>
                  <Trash2 size={14} aria-hidden="true" />{t("t5DeleteProject")}
                </button>
              </div>
            </section>
            <ConfirmDialog
              bind:open={confirming}
              title={t("t5DeleteProject")}
              confirm={t("restConfigDeleteConfirm", { name: project.name })}
              danger
              onconfirm={remove}
            >
              <p>{t("restConfigDeleteLost", { name: project.name })}</p>
              <p>{t("restConfigDeleteKept")}</p>
            </ConfirmDialog>
          {/if}
        </section>

        <!-- The project's library in place, its tabs and its sheet included: the sections stay in reach. -->
        <section class="sec lib" hidden={current !== "library"} aria-label={t("viewLibrary")}>
          {#if current === "library"}<LibraryView embedded />{/if}
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
            {:else if setup === null}
              <Skeleton rows={4} />
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
            <TeamReadFreely project={id} />
            <TeamSquads project={id} />
            <TeamRoles project={id} />
            <TeamDecisions project={id} />
          </section>

          <section class="sec" hidden={current !== "connectors"} aria-labelledby="config-connectors">
            <!-- Only once shown: it asks Claude Code to check its servers. -->
            {#if current === "connectors"}<ProjectConnectors project={id} />{/if}
          </section>

          <!-- The Library's own editor, read again when the repositories change. -->
          <section class="sec flow worktrees" hidden={current !== "worktrees"}>
            {#if project}
              {#key project.repos.map((r) => r.path).join("\n")}
                <WorktreeSetup {project} />
              {/key}
            {/if}
          </section>

          <section class="sec flow" hidden={current !== "memory"}>
            <MemorySettings project={id} />
          </section>

          <section class="sec" hidden={current !== "remote"} aria-labelledby="config-remote">
            <RemoteSettings project={id} />
          </section>

          <section class="sec" hidden={current !== "trust"} aria-labelledby="config-trust">
            <h2 id="config-trust">{t("obTrustTitle")}</h2>
            <ClaudeTrust project={id} />
          </section>

        {/if}
      </div>
    </div>
  </div>

{/if}

<dialog bind:this={question} class="question" oncancel={() => answer("stay")}>
  <p>{t("ux3t3Unsaved", { name: project?.name ?? "" })}</p>
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
    /* Stacked, the sections take the height they need (the first row) and the page the rest. */
    grid-template: auto / 224px minmax(0, 1fr);
    grid-auto-rows: minmax(0, 1fr);
    min-height: 0;
    container: sections / inline-size;
  }

  @container sections (max-width: 640px) {
    .body > :global(*) {
      grid-column: 1 / -1;
    }
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

  /* The library scrolls its own list beside its sheet, over the page's whole width and height. */
  .page.full {
    display: flex;
    padding: 0;
    overflow: hidden;
  }

  .sec.lib {
    flex: 1;
    max-width: none;
    min-height: 0;
  }

  .lib > :global(.library) {
    flex: 1;
  }

  /* The memory brings its own spacing. */
  .sec.flow {
    display: block;
  }

  /* The recipe's editor pads itself for the Library: here the page already does. */
  .worktrees :global(.pane) {
    padding: 0;
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

  .look {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
  }

  .look .labelled {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  /* As big as the Home's: a preview of the navigation's mark. */
  .look .mark {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    overflow: hidden;
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: var(--text-md);
    font-weight: 700;
  }

  .swatch {
    width: 32px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: none;
    cursor: pointer;
  }

  .look .icon {
    width: 72px;
    height: 28px;
    text-align: center;
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

  .repos li + li {
    border-top: 1px solid var(--border);
  }

  .checks {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px 12px 60px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .checks span {
    flex-shrink: 0;
  }

  .checks input {
    flex: 1;
    max-width: 360px;
    height: 28px;
    font-size: var(--text-sm);
  }

  .checks-hint {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  /* Right under the list it adds to. */
  .adding {
    margin-top: -4px;
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


  /* Apart from the rest, at the bottom: what cannot be undone. */
  .danger-zone {
    margin-top: 16px;
    padding-top: 20px;
    border-top: 1px solid var(--border);
  }

  .danger-zone h3 {
    margin: 0 0 10px;
    color: var(--danger);
  }

  .danger-row {
    display: flex;
    align-items: center;
    gap: 16px;
    max-width: 760px;
    padding: 14px 16px;
    border: 1px solid var(--danger-line);
    border-radius: var(--radius-md);
    background: var(--panel);
  }

  .danger-text {
    flex: 1;
    min-width: 0;
  }

  .danger-text p {
    margin: 2px 0 0;
    color: var(--soft);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .danger-text .kept {
    color: var(--muted);
  }

  .danger-row .btn {
    flex-shrink: 0;
  }

  .delete {
    color: var(--danger);
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
    background: var(--scrim, rgb(10 12 20 / 0.3));
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
