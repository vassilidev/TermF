<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import X from "@lucide/svelte/icons/x";
  import Banner from "$lib/ui/Banner.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { t } from "$lib/i18n";
  import { squadIssues, type SquadForm, type SquadIssue } from "$lib/team-logic";

  /** One squad of the project, new when `from` is null: only its table of the project's file changes. */
  let {
    project,
    from,
    form,
    others,
    roles,
    assignable,
    repos,
    onsaved,
    oncancel,
  }: {
    project: string;
    from: string | null;
    form: SquadForm;
    /** The project's other squads, for the names taken and the perimeters that meet. */
    others: SquadForm[];
    /** Every role the library knows, and those a squad's member may be. */
    roles: string[];
    assignable: string[];
    /** The project's repositories, by name. */
    repos: string[];
    onsaved: (view: unknown) => void;
    oncancel: () => void;
  } = $props();

  // The editor is mounted per squad: `form` only seeds it.
  // svelte-ignore state_referenced_locally
  let id = $state(form.id);
  // svelte-ignore state_referenced_locally
  let lead = $state(form.lead);
  // svelte-ignore state_referenced_locally
  let members = $state(Object.entries(form.roles).map(([role, count]) => ({ role, count })));
  // svelte-ignore state_referenced_locally
  let picked = $state([...form.repos]);
  // svelte-ignore state_referenced_locally
  let paths = $state(form.paths.join("\n"));
  // svelte-ignore state_referenced_locally
  let domains = $state(form.domains.join(", "));
  let busy = $state(false);
  let error = $state("");
  let confirming = $state(false);

  const current = $derived<SquadForm>({
    id: id.trim(),
    lead: lead.trim(),
    roles: Object.fromEntries(members.map((m) => [m.role, m.count])),
    repos: picked,
    paths: paths.split("\n").map((p) => p.trim()).filter(Boolean),
    domains: domains.split(",").map((d) => d.trim()).filter(Boolean),
  });
  const issues = $derived(squadIssues(current, others, roles, repos));
  const blocked = $derived(issues.some((i) => i.level === "error"));
  // A repository the file names but the project has not stays in the list, to be unticked.
  const shownRepos = $derived([...repos, ...picked.filter((r) => !repos.includes(r))]);
  const addable = $derived(assignable.filter((r) => !members.some((m) => m.role === r)));
  const leads = $derived(lead && !roles.includes(lead) ? [lead, ...roles] : roles);

  const said: Record<SquadIssue["kind"], Parameters<typeof t>[0]> = {
    id: "teams2SqIssueId",
    taken: "teams2SqIssueTaken",
    role: "teams2SqIssueRole",
    count: "teams2SqIssueCount",
    path: "teams2SqIssuePath",
    repo: "teams2SqIssueRepo",
    overlap: "teams2SqIssueOverlap",
  };

  function add(e: Event & { currentTarget: HTMLSelectElement }) {
    const role = e.currentTarget.value;
    if (role) members.push({ role, count: 1 });
    e.currentTarget.value = "";
  }

  async function act(command: string, args: Record<string, unknown>) {
    busy = true;
    error = "";
    try {
      onsaved(await invoke(command, { project, ...args }));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function save(e: SubmitEvent) {
    e.preventDefault();
    if (!blocked) act("team_squad_save", { from, squad: current });
  }
</script>

<form class="editor" onsubmit={save} aria-labelledby="squad-editor-title">
  <header>
    <b id="squad-editor-title">{from ? t("teams2SqEdit", { id: from }) : t("teams2SqNew")}</b>
    <button type="button" class="icon-btn sm" onclick={oncancel} aria-label={t("close")} title={t("close")}>
      <X size={14} aria-hidden="true" />
    </button>
  </header>

  <div class="fields">
    <label for="squad-id">{t("teams2SqId")}</label>
    <input
      id="squad-id"
      class="field"
      bind:value={id}
      autocomplete="off"
      spellcheck="false"
      {@attach (el) => el.focus()}
    />

    <label for="squad-lead">{t("rsLead")}</label>
    <select id="squad-lead" class="field" bind:value={lead}>
      <option value="">{t("teams2SqLeadDefault")}</option>
      {#each leads.filter((r) => r !== "lead") as role (role)}<option value={role}>{role}</option>{/each}
    </select>

    <span class="label">{t("rsMembers")}</span>
    <div class="stack">
      {#each members as member, i (member.role)}
        <div class="member">
          <span class="mono">{member.role}</span>
          <input
            class="field count"
            type="number"
            min="1"
            step="1"
            bind:value={member.count}
            aria-label={t("teams2SqCount", { role: member.role })}
          />
          <button
            type="button"
            class="icon-btn sm"
            onclick={() => members.splice(i, 1)}
            aria-label={t("teams2SqRemoveRole", { role: member.role })}
            title={t("teams2SqRemoveRole", { role: member.role })}
          >
            <X size={14} aria-hidden="true" />
          </button>
        </div>
      {/each}
      {#if !members.length}<small class="muted">{t("teams2SqAllRoles")}</small>{/if}
      {#if addable.length}
        <select class="field add" onchange={add} aria-label={t("teams2SqAddRole")}>
          <option value="">{t("teams2SqAddRole")}</option>
          {#each addable as role (role)}<option value={role}>{role}</option>{/each}
        </select>
      {/if}
    </div>

    <span class="label">{t("rsRepos")}</span>
    <div class="stack">
      <div class="repos">
        {#each shownRepos as repo (repo)}
          <label class="check">
            <input
              type="checkbox"
              checked={picked.includes(repo)}
              onchange={(e) => (picked = e.currentTarget.checked ? [...picked, repo] : picked.filter((r) => r !== repo))}
            />
            <span class="mono">{repo}</span>
          </label>
        {/each}
      </div>
      {#if !picked.length}<small class="muted">{t("teams2SqAllRepos")}</small>{/if}
    </div>

    <label for="squad-paths">{t("rsPaths")}</label>
    <div class="stack">
      <textarea id="squad-paths" class="field mono" rows="3" spellcheck="false" placeholder="apps/web/**" bind:value={paths}
      ></textarea>
      <small class="muted">{t("teams2SqPathsHint")}</small>
    </div>

    <label for="squad-domains">{t("rsDomains")}</label>
    <div class="stack">
      <input id="squad-domains" class="field" bind:value={domains} autocomplete="off" />
      <small class="muted">{t("teams2SqDomainsHint")}</small>
    </div>
  </div>

  {#if issues.length}
    <ul class="issues" aria-live="polite">
      {#each issues as issue, i (i)}
        <li class={issue.level}>{t(said[issue.kind], { subject: issue.subject })}</li>
      {/each}
    </ul>
  {/if}
  {#if error}
    <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
  {/if}

  <footer>
    {#if from}
      <button type="button" class="btn sm delete" disabled={busy} onclick={() => (confirming = true)}>{t("delete")}</button>
    {/if}
    <span class="spacer"></span>
    <button type="button" class="btn sm" onclick={oncancel}>{t("cancel")}</button>
    <button type="submit" class="btn sm primary" disabled={busy || blocked}>{t("save")}</button>
  </footer>
</form>

{#if from}
  <ConfirmDialog
    bind:open={confirming}
    title={t("teams2SqDeleteTitle", { id: from })}
    confirm={t("delete")}
    danger
    onconfirm={() => act("team_squad_delete", { id: from })}
  >
    <p>{t("teams2SqDeleteBody")}</p>
  </ConfirmDialog>
{/if}

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 8px;
    padding: 12px;
    border: 1px solid var(--accent);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  header,
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  header {
    justify-content: space-between;
  }

  header b {
    font-weight: 650;
  }

  .fields {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 10px 16px;
    align-items: baseline;
    font-size: var(--text-sm);
  }

  .fields > label,
  .label {
    color: var(--muted);
  }

  .fields .field {
    max-width: 420px;
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .member {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .member .mono {
    min-width: 96px;
  }

  .fields .count {
    width: 64px;
  }

  .fields .add {
    width: 200px;
  }

  .repos {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  textarea {
    resize: vertical;
  }

  .issues {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .issues li::before {
    margin-right: 6px;
    font-weight: 700;
  }

  .issues .error {
    color: var(--danger);
  }

  .issues .error::before {
    content: "×";
  }

  .issues .warning {
    color: var(--warning);
  }

  .issues .warning::before {
    content: "!";
  }

  .spacer {
    flex: 1;
  }

  .delete {
    border-color: var(--danger);
    color: var(--danger);
  }

  p {
    margin: 0;
  }
</style>
