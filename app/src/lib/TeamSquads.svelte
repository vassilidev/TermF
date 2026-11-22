<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import SquadEditor from "$lib/SquadEditor.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import { t } from "$lib/i18n";
  import { library, type Item, type Layer } from "$lib/library.svelte";
  import { app, onScreen, shortPath } from "$lib/state.svelte";
  import { offered, shown, source, type SquadForm } from "$lib/team-logic";

  /** `team_squads`: a project's squads and staff, each role with its settings as the layers make them. */
  type Setting = { key: string; value: unknown; from: ("role" | "global" | "project" | "squad" | "repo")[] };
  type Member = {
    role: string;
    lead: boolean;
    count: number | null;
    description: string | null;
    layer: Layer | null;
    plannable: boolean;
    credits: string[];
    settings: Setting[];
    error: string | null;
  };
  type Squad = {
    id: string;
    lead: string;
    roles: Record<string, number>;
    repos: string[];
    paths: string[];
    domains: string[];
    maxParallel: number | null;
    members: Member[];
  };
  type View = { declared: boolean; squads: Squad[]; staff: Member[] };

  let view = $state<View | null>(null);
  let error = $state("");
  /** The member whose settings show: its squad (null for the staff) and role. */
  let picked = $state<{ squad: string | null; role: string } | null>(null);

  /** The current project's, unless the page around it says which. */
  let { project: given }: { project?: string } = $props();
  const project = $derived(given ?? app.project);
  const file = $derived(project && app.workspace ? `${shortPath(app.workspace.configDir)}/projects/${project}.toml` : "");
  const member = $derived.by(() => {
    if (!view || !picked) return null;
    const { squad, role } = picked;
    const list = squad === null ? view.staff : view.squads.find((s) => s.id === squad)?.members;
    return list?.find((m) => m.role === role) ?? null;
  });

  const from: Record<Setting["from"][number], Parameters<typeof t>[0]> = {
    role: "rsFromRole",
    global: "rsFromGlobal",
    project: "rsFromProject",
    squad: "rsFromSquad",
    repo: "rsFromRepo",
  };

  /** The squad in the editor: the one it was (null for a new one), and what seeds the form. */
  let editing = $state<{ from: string | null; form: SquadForm } | null>(null);
  /** The library's roles, for the editor. */
  let roles = $state<Item[]>([]);
  /** The onboarding assistant's squads, read while the project declares none. */
  let proposals = $state<SquadForm[]>([]);
  let adding = $state(false);

  const usable = $derived(roles.filter((r) => r.layer !== "repo" || r.active));
  const known = $derived(usable.map((r) => r.name));
  const assignable = $derived(
    usable.filter((r) => r.meta.plannable !== false && r.name !== "orchestrator").map((r) => r.name),
  );
  const repos = $derived(app.workspace?.projects.find((p) => p.id === project)?.repos.map((r) => r.name) ?? []);
  const forms = $derived(view?.declared ? view.squads.map(formOf) : []);
  const offering = $derived(view ? offered(proposals, view.declared, view.squads) : []);

  /** A squad as the editor takes it: the default lead left empty. */
  function formOf(s: Pick<Squad, "id" | "lead" | "roles" | "repos" | "paths" | "domains">): SquadForm {
    const { id, lead, roles, repos, paths, domains } = s;
    return { id, lead: lead === "lead" ? "" : lead, roles, repos, paths, domains };
  }

  async function load(asked: string) {
    invoke<Item[]>("library_list", { kind: "role", project: asked }).then(
      (list) => asked === project && (roles = list),
      () => {},
    );
    try {
      const got = await invoke<View>("team_squads", { project: asked });
      if (asked !== project) return;
      view = got;
      error = "";
    } catch (e) {
      if (asked !== project) return;
      view = null;
      error = String(e);
      return;
    }
    if (view.declared) return;
    // Without the assistant's proposals, a squad is still created by hand.
    const team = await invoke<{ proposal: { team: { squads: SquadForm[] } | null } }>("onboarding_team", {
      project: asked,
      fresh: false,
    }).catch(() => null);
    if (asked === project) proposals = (team?.proposal.team?.squads ?? []).map(formOf);
  }

  function saved(got: unknown) {
    view = got as View;
    editing = null;
  }

  /** The assistant's squads, one after the other: a refusal stops there and tells why. */
  async function adopt(list: SquadForm[]) {
    adding = true;
    error = "";
    try {
      for (const squad of list) view = await invoke<View>("team_squad_save", { project, from: null, squad });
    } catch (e) {
      error = String(e);
    } finally {
      adding = false;
    }
  }

  function edit(from: string | null, form: SquadForm) {
    picked = null;
    editing = { from, form };
  }

  // Again on coming back: the project's file or the library may have changed meanwhile.
  $effect(() => {
    void library.version;
    if (!onScreen("project")) return;
    if (!project) {
      view = null;
      return;
    }
    load(project);
  });

  $effect(() => {
    void project;
    picked = null;
    editing = null;
    proposals = [];
  });

  function pick(squad: string | null, role: string) {
    picked = picked?.squad === squad && picked.role === role ? null : { squad, role };
  }
</script>

{#snippet chip(squad: string | null, m: Member)}
  {@const on = picked?.squad === squad && picked.role === m.role}
  <button
    class="chip"
    class:lead={m.lead}
    class:broken={m.error}
    aria-pressed={on}
    title={m.error ?? m.description ?? undefined}
    onclick={() => pick(squad, m.role)}
  >
    {m.role}{#if m.count && m.count > 1}<small>×{m.count}</small>{/if}
  </button>
{/snippet}

{#if project}
  <section aria-labelledby="team-squads">
    <div class="head">
      <h2 id="team-squads">{t("rsSquads")} {#if view?.declared}<small>{view.squads.length}</small>{/if}</h2>
      {#if view}
        <button
          class="btn sm"
          aria-expanded={editing?.from === null}
          onclick={() => edit(null, { id: "", lead: "", roles: {}, repos: [], paths: [], domains: [] })}
        >
          <Plus size={14} aria-hidden="true" />{t("teams2SqNew")}
        </button>
      {/if}
    </div>
    <p class="intro">{t("teams2SqIntro")}</p>
    <p class="where muted">{view && !view.declared ? `${t("rsImplicit")} ` : ""}{t("rsWhere", { file })}</p>
    {#if error}
      <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
    {/if}
    {#if offering.length}
      <div class="proposals">
        <p>{t("teams2SqProposed")}</p>
        <ul>
          {#each offering as p (p.id)}
            <li>
              <b>{p.id}</b>
              <span class="muted">
                {Object.entries(p.roles)
                  .map(([r, n]) => (n > 1 ? `${r} ×${n}` : r))
                  .join(", ")}
              </span>
              {#if p.repos.length || p.paths.length}
                <span class="mono muted">{[...p.repos, ...p.paths].join("  ")}</span>
              {/if}
              <button class="btn sm" disabled={adding} onclick={() => adopt([p])}>{t("teams2SqAdd")}</button>
            </li>
          {/each}
        </ul>
        {#if offering.length > 1}
          <div>
            <button class="btn sm primary" disabled={adding} onclick={() => adopt(offering)}>
              {t("teams2SqAddAll", { n: offering.length })}
            </button>
          </div>
        {/if}
      </div>
    {/if}
    {#if view}
      <div class="squads">
        {#each view.squads as squad (squad.id)}
          {@const lead = squad.members.find((m) => m.lead)}
          <article class="squad" aria-label={squad.id}>
            <header>
              <b>{squad.id}</b>
              <span class="tools">
                {#if squad.maxParallel}<span class="badge">{t("rsMaxParallel", { n: squad.maxParallel })}</span>{/if}
                {#if view.declared}
                  <button
                    class="icon-btn sm"
                    aria-label={t("teams2SqEdit", { id: squad.id })}
                    title={t("teams2SqEdit", { id: squad.id })}
                    aria-expanded={editing?.from === squad.id}
                    onclick={() => edit(squad.id, formOf(squad))}
                  >
                    <Pencil size={14} aria-hidden="true" />
                  </button>
                {/if}
              </span>
            </header>
            <dl>
              {#if lead}
                <dt>{t("rsLead")}</dt>
                <dd>{@render chip(squad.id, lead)}</dd>
              {/if}
              <dt>{t("rsMembers")}</dt>
              <dd class="chips">
                {#each squad.members.filter((m) => !m.lead) as m (`${m.lead}:${m.role}`)}{@render chip(squad.id, m)}{/each}
              </dd>
              <dt>{t("rsRepos")}</dt>
              {#if squad.repos.length}
                <dd class="mono">{squad.repos.join(", ")}</dd>
              {:else}
                <dd class="muted">{t("teams2SqEveryRepo")}</dd>
              {/if}
              <dt>{t("rsPaths")}</dt>
              {#if squad.paths.length}
                <dd class="mono">{squad.paths.join(", ")}</dd>
              {:else}
                <dd class="muted">{t("teams2SqWholeRepo")}</dd>
              {/if}
              {#if squad.domains.length}
                <dt>{t("rsDomains")}</dt>
                <dd>{squad.domains.join(", ")}</dd>
              {/if}
            </dl>
          </article>
        {/each}
        {#if view.staff.length}
          <article class="squad staff" aria-label={t("rsStaff")}>
            <header><b>{t("rsStaff")}</b></header>
            <div class="chips">
              {#each view.staff as m (`${m.lead}:${m.role}`)}{@render chip(null, m)}{/each}
            </div>
          </article>
        {/if}
      </div>

      {#if editing && project}
        {#key editing}
          <SquadEditor
            {project}
            from={editing.from}
            form={editing.form}
            others={forms.filter((f) => f.id !== editing?.from)}
            roles={known}
            {assignable}
            {repos}
            onsaved={saved}
            oncancel={() => (editing = null)}
          />
        {/key}
      {/if}

      {#if member && picked}
        <div class="detail">
          <header>
            <div>
              <b>
                {picked.squad === null
                  ? t("rsInStaff", { role: member.role })
                  : t("rsInSquad", { role: member.role, squad: picked.squad })}
              </b>
              {#if member.description}<small class="muted">{member.description}</small>{/if}
            </div>
            <button class="icon-btn sm" onclick={() => (picked = null)} aria-label={t("close")} title={t("close")}>
              <X size={14} aria-hidden="true" />
            </button>
          </header>
          {#if member.error}
            <Banner kind="error">{member.error}</Banner>
          {:else if member.settings.length}
            <table>
              <thead>
                <tr>
                  <th scope="col">{t("rsSetting")}</th>
                  <th scope="col">{t("rsValue")}</th>
                  <th scope="col">{t("rsFrom")}</th>
                </tr>
              </thead>
              <tbody>
                {#each member.settings as s (s.key)}
                  <tr>
                    <td class="mono">{s.key}</td>
                    <td class="mono value">{shown(s.value)}</td>
                    <td>
                      <span class="from">
                        {#each s.from as f, i (i)}<span class="badge from-{f}">{t(from[f])}</span>{/each}
                      </span>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {:else}
            <p class="muted">{t("rsNoSettings")}</p>
          {/if}
          {#if member.credits.length}
            <p class="credits muted">{t("rsCredits", { sources: member.credits.map(source).join(", ") })}</p>
          {/if}
        </div>
      {/if}
    {/if}
  </section>
{/if}

<style>
  h2 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0 0 4px;
    font-size: var(--text-md);
    font-weight: 650;
  }

  h2 small {
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 400;
  }

  .where {
    margin: 0 0 10px;
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 4px;
  }

  .head h2 {
    margin: 0;
  }

  .intro {
    margin: 0 0 2px;
    font-size: var(--text-sm);
  }

  .tools {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .proposals {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 10px;
    padding: 12px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-lg);
    font-size: var(--text-sm);
  }

  .proposals p {
    margin: 0;
  }

  .proposals ul {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .proposals li {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
  }

  .proposals li .btn {
    margin-left: auto;
  }

  .squads {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 8px;
  }

  .squad {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .squad header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .squad header b {
    font-weight: 650;
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 12px;
    align-items: baseline;
    margin: 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .chip {
    display: inline-flex;
    align-items: baseline;
    gap: 2px;
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--text);
    font-size: var(--text-sm);
    font-weight: 500;
    line-height: 20px;
  }

  .chip small {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .chip:hover {
    border-color: var(--border-strong);
  }

  .chip.lead {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .chip.broken {
    border-color: var(--danger);
  }

  .chip[aria-pressed="true"] {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .detail {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 8px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .detail header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
  }

  .detail header div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .detail small {
    font-size: var(--text-sm);
  }

  table {
    width: 100%;
    border-spacing: 0;
    table-layout: fixed;
  }

  th {
    height: 28px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    text-align: left;
  }

  th:first-child {
    width: 140px;
  }

  th:last-child {
    width: 180px;
  }

  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }

  tr:last-child td {
    border-bottom: 0;
  }

  .value {
    overflow-wrap: anywhere;
  }

  .from {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .from-project,
  .from-squad {
    border-color: transparent;
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .from-global,
  .from-repo {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .credits {
    margin: 0;
    font-size: var(--text-xs);
  }
</style>
