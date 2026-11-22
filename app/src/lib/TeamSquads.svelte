<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import X from "@lucide/svelte/icons/x";
  import Banner from "$lib/ui/Banner.svelte";
  import { t } from "$lib/i18n";
  import { library, type Layer } from "$lib/library.svelte";
  import { app, shortPath, ui } from "$lib/state.svelte";
  import { shown, source } from "$lib/team-logic";

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

  const project = $derived(app.project);
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

  async function load(asked: string) {
    try {
      const got = await invoke<View>("team_squads", { project: asked });
      if (asked !== project) return;
      view = got;
      error = "";
    } catch (e) {
      if (asked !== project) return;
      view = null;
      error = String(e);
    }
  }

  // Again on coming back: the project's file or the library may have changed meanwhile.
  $effect(() => {
    void library.version;
    if (ui.view !== "team") return;
    if (!project) {
      view = null;
      return;
    }
    load(project);
  });

  $effect(() => {
    void project;
    picked = null;
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
    <h2 id="team-squads">{t("rsSquads")} {#if view?.declared}<small>{view.squads.length}</small>{/if}</h2>
    <p class="where muted">{view && !view.declared ? `${t("rsImplicit")} ` : ""}{t("rsWhere", { file })}</p>
    {#if error}
      <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
    {/if}
    {#if view}
      <div class="squads">
        {#each view.squads as squad (squad.id)}
          {@const lead = squad.members.find((m) => m.lead)}
          <article class="squad" aria-label={squad.id}>
            <header>
              <b>{squad.id}</b>
              {#if squad.maxParallel}<span class="badge">{t("rsMaxParallel", { n: squad.maxParallel })}</span>{/if}
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
              {#if squad.repos.length}
                <dt>{t("rsRepos")}</dt>
                <dd class="mono">{squad.repos.join(", ")}</dd>
              {/if}
              {#if squad.paths.length}
                <dt>{t("rsPaths")}</dt>
                <dd class="mono">{squad.paths.join(", ")}</dd>
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
