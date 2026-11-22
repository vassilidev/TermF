<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import LibraryBig from "@lucide/svelte/icons/library-big";
  import ContextGauge from "$lib/ContextGauge.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import TeamSquads from "$lib/TeamSquads.svelte";
  import { focusSession } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { currentProject } from "$lib/files.svelte";
  import { library, showLibrary, type Item, type Layer } from "$lib/library.svelte";
  import { projectsOf } from "$lib/nav-logic";
  import { sessionProjects } from "$lib/project.svelte";
  import { app, shortPath, ui, type Gauge } from "$lib/state.svelte";
  import { access, initials } from "$lib/team-logic";
  import { team } from "$lib/team.svelte";
  import { tickets } from "$lib/tickets.svelte";

  /** What the view reads of a headless session (`agent_sessions`). */
  type AgentSession = {
    id: number;
    title: string;
    cwd: string;
    project: string | null;
    state: "working" | "waiting" | "idle" | "interactive" | "stopped" | "error" | "budget";
    context: Gauge | null;
  };

  let agents = $state<AgentSession[]>([]);
  let busy = $state(false);
  let error = $state("");

  const project = $derived(currentProject());
  const scope = $derived(app.project ?? "");
  const here = $derived<Layer>(scope ? "project" : "global");
  const visible = $derived(ui.view === "team");

  // A session in a tile is listed once, as its tile; a headless one only while its process lives.
  // The current project's own, whatever the Terminals view shows (its "every project" toggle).
  const sessions = $derived(
    app.sessions.filter(
      (s) => s.running && s.agent !== null && (app.project === null || sessionProjects(s).includes(app.project)),
    ),
  );
  const headless = $derived(
    agents.filter(
      (a) =>
        (a.state === "working" || a.state === "waiting" || a.state === "idle") &&
        (app.project === null ||
          a.project === app.project ||
          projectsOf(a.cwd, app.workspace?.projects ?? [], tickets.runs).includes(app.project)),
    ),
  );

  const layerLabel: Record<Layer, Parameters<typeof t>[0]> = {
    shipped: "layerShipped",
    global: "layerGlobal",
    project: "layerProject",
    repo: "layerRepo",
    run: "layerRun",
  };

  /** The scope `team.roles` belongs to: another project's roles are never shown, nor counted, under this one. */
  let rolesOf: string | null = null;

  async function refresh() {
    const asked = scope;
    if (asked !== rolesOf) team.roles = [];
    try {
      const roles = await invoke<Item[]>("library_list", { kind: "role", project: asked || null });
      if (asked !== scope) return;
      team.roles = roles;
      rolesOf = asked;
    } catch (e) {
      if (asked !== scope) return;
      team.roles = [];
      rolesOf = null;
      error = String(e);
    }
  }

  // Again on coming back: the Library may have changed the roles meanwhile.
  $effect(() => {
    void library.version;
    void visible;
    refresh();
  });

  onMount(() => {
    invoke<AgentSession[]>("agent_sessions").then(
      (list) => (agents = list),
      (e) => (error = String(e)),
    );
    const off = listen<AgentSession>("agent-updated", ({ payload }) => {
      const i = agents.findIndex((a) => a.id === payload.id);
      if (i >= 0) agents[i] = payload;
      else agents.unshift(payload);
    });
    return () => off.then((stop) => stop());
  });

  /** As in the Library: `enabled` null inherits; a refused toggle puts the switch back. */
  async function toggle(role: Item, enabled: boolean | null, input?: HTMLInputElement) {
    busy = true;
    error = "";
    try {
      await invoke("library_toggle", { kind: "role", name: role.name, project: scope || null, enabled });
    } catch (e) {
      if (input) input.checked = role.active;
      error = String(e);
    } finally {
      library.version++;
      busy = false;
    }
  }

  function editRoles() {
    Object.assign(library, { global: false, tab: "role", panel: null });
    showLibrary();
  }

  const text = (value: unknown) => (typeof value === "string" && value ? value : t("t5None"));
</script>

<div class="team">
  <ViewHeader title={t("t5Team")} context={project?.name ?? t("libraryGlobal")}>
    {#snippet actions()}
      <button class="btn sm" onclick={editRoles}>
        <LibraryBig size={14} aria-hidden="true" />{t("t5EditRoles")}
      </button>
    {/snippet}
  </ViewHeader>

  {#if error}
    <div class="banners">
      <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
    </div>
  {/if}

  <div class="content">
    {#if sessions.length || headless.length}
      <section aria-labelledby="team-live">
        <h2 id="team-live">{t("t5InService")} <small>{sessions.length + headless.length}</small></h2>
        <ul class="live">
          <!-- The role an agent works as goes in each card once runs launch roles (phase 4). -->
          {#each sessions as session (session.id)}
            <li>
              <button class="agent" class:waiting={session.status === "waiting"} onclick={() => focusSession(session.id)}>
                <span class="top">
                  <b>{session.title}</b>
                  {#if session.status}<StatusPill status={session.status} />{/if}
                </span>
                <span class="foot">
                  <span class="mono">{shortPath(session.cwd)}</span>
                  {#if session.context}<ContextGauge gauge={session.context} />{/if}
                </span>
              </button>
            </li>
          {/each}
          {#each headless as agent (agent.id)}
            <li>
              <div class="agent" class:waiting={agent.state === "waiting"}>
                <span class="top">
                  <b>{agent.title}</b>
                  <StatusPill status={agent.state as "working" | "waiting" | "idle"} />
                </span>
                <span class="foot">
                  <span class="mono">{shortPath(agent.cwd)}</span>
                  {#if agent.context}<ContextGauge gauge={agent.context} />{/if}
                </span>
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <TeamSquads />

    <section aria-labelledby="team-roles">
      <h2 id="team-roles">{t("metaRoles")}</h2>
      {#if team.roles.length}
        <table class="roster">
          <colgroup>
            <col />
            <col class="model" />
            <col class="effort" />
            <col class="access" />
            <col class="layer" />
            <col class="on" />
          </colgroup>
          <thead>
            <tr>
              <th scope="col">{t("kindRole")}</th>
              <th scope="col">{t("metaModel")}</th>
              <th scope="col">{t("metaEffort")}</th>
              <th scope="col">{t("t5Access")}</th>
              <th scope="col">{t("t5Layer")}</th>
              <th scope="col">{t("itemOn")}</th>
            </tr>
          </thead>
          <tbody>
            {#each team.roles as role (role.name)}
              {@const writes = access(role.meta)}
              <tr>
                <td>
                  <div class="who">
                    <span class="mark" aria-hidden="true">{initials(role.name)}</span>
                    <div>
                      <b>{role.name}</b>
                      {#if role.description}<small title={role.description}>{role.description}</small>{/if}
                    </div>
                  </div>
                </td>
                <td><span class="tag">{text(role.meta.model)}</span></td>
                <td class="muted">{text(role.meta.effort)}</td>
                <td class="muted" class:mono={Array.isArray(writes)}>
                  {Array.isArray(writes) ? writes.join(", ") : writes ? t("t5ReadOnly") : t("t5None")}
                </td>
                <td><span class="badge layer-{role.layer}">{t(layerLabel[role.layer])}</span></td>
                <td>
                  <div class="active">
                    <input
                      class="switch"
                      type="checkbox"
                      role="switch"
                      checked={role.active}
                      disabled={busy}
                      aria-label={t("itemActive", { name: role.name })}
                      onchange={(e) => toggle(role, e.currentTarget.checked, e.currentTarget)}
                    />
                    {#if role.toggles[here] !== undefined}
                      <button class="btn sm quiet" disabled={busy} onclick={() => toggle(role, null)} title={t("inheritTitle")}>
                        {t("inherit")}
                      </button>
                    {/if}
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="muted">{t("libraryEmpty")}</p>
      {/if}
    </section>
  </div>
</div>

<style>
  .team {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }

  .content {
    display: flex;
    flex-direction: column;
    gap: 28px;
    flex: 1;
    min-height: 0;
    padding: 20px 24px;
    overflow-y: auto;
  }

  h2 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0 0 8px;
    font-size: var(--text-md);
    font-weight: 650;
  }

  h2 small {
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 400;
  }

  .live {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .agent {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 100%;
    height: 100%;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    color: var(--text);
    text-align: left;
  }

  button.agent:hover {
    border-color: var(--border-strong);
  }

  .agent.waiting {
    border-color: var(--waiting-ring);
    background: var(--waiting-soft);
  }

  .top,
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .top b {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }

  .foot {
    justify-content: space-between;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .foot .mono {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Fixed columns: the description, not the switches, gives way in a narrow window. */
  .roster {
    width: 100%;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    border-spacing: 0;
    background: var(--panel);
    table-layout: fixed;
  }

  .model {
    width: 96px;
  }

  .effort {
    width: 88px;
  }

  .access {
    width: 200px;
  }

  .layer {
    width: 96px;
  }

  .on {
    width: 132px;
  }

  th {
    height: 34px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    text-align: left;
    white-space: nowrap;
  }

  td {
    height: 54px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
    overflow-wrap: anywhere;
  }

  td.muted {
    font-size: var(--text-sm);
  }

  tr:last-child td {
    border-bottom: 0;
  }

  .who {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .who div {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    line-height: 1.35;
  }

  .who b {
    font-weight: 600;
  }

  .who small {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mark {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--hover);
    color: var(--text);
    font-size: var(--text-xs);
    font-weight: 700;
  }

  .tag {
    display: inline-flex;
    align-items: center;
    height: 20px;
    padding: 0 7px;
    border-radius: var(--radius-sm);
    background: var(--hover);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .layer-project {
    border-color: transparent;
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .layer-global {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .active {
    display: flex;
    align-items: center;
    gap: 6px;
  }
</style>
