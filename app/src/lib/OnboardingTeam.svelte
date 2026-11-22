<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Users from "@lucide/svelte/icons/users";
  import X from "@lucide/svelte/icons/x";
  import ClaudeChange from "$lib/ClaudeChange.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { t } from "$lib/i18n";
  import {
    globList,
    members,
    replacesHand,
    teamEdit,
    teamKeys,
    toReview,
    type TeamChanges,
    type TeamDecision,
    type TeamEdit,
    type TeamSettings,
    type TeamStatus,
  } from "$lib/onboarding-logic";

  type Choices = { keep: Record<string, boolean>; split: boolean | null; size: number | null; model: string | null };
  type RoleChoice = { role: string; keep: boolean; why: string; detail: string[]; chosen: boolean };
  type Team = {
    choices: Choices;
    splittable: boolean;
    roles: RoleChoice[];
    steps: string[];
    proposal: TeamEdit;
    status: Record<string, TeamStatus>;
    path: string;
  };

  let {
    project,
    decision,
    ondecided,
  }: { project: string; decision?: TeamDecision; ondecided: (state: unknown) => void } = $props();

  let team = $state<Team | null>(null);
  /** What the card changed of the proposal's choices. */
  let choices = $state<Partial<Choices> & { keep: Record<string, boolean> }>({ keep: {} });
  let decisions = $state<Record<string, TeamDecision>>({});
  let changes = $state<TeamChanges>({ roles: {} });
  /** The part being changed, or "". */
  let editing = $state("");
  let draft = $state({ writes: "", model: "", effort: "" });
  let preview = $state<{ path: string; diff: string } | null>(null);
  let busy = $state(false);
  let error = $state("");

  const keys = $derived(team ? toReview(teamKeys(team.proposal), team.status, decisions) : []);
  const accepted = $derived(keys.filter((k) => decisions[k] === "accepted").length);
  const pending = $derived(keys.filter((k) => !decisions[k]).length);
  const squads = $derived(team?.proposal.team?.squads ?? []);
  const staff = $derived(team?.proposal.team?.staff ?? []);
  const left = $derived(team?.roles.filter((r) => !r.keep) ?? []);
  const workflow = $derived(changes.workflow ?? team?.proposal.workflow ?? "default");

  const whyKey = {
    front: "obTeamWhyFront",
    back: "obTeamWhyBack",
    tests: "obTeamWhyTests",
    "no-tests": "obTeamWhyNoTests",
    infra: "obTeamWhyInfra",
    ui: "obTeamWhyUi",
    web: "obTeamWhyWeb",
    design: "obTeamWhyDesign",
    own: "obTeamWhyOwn",
    "no-front": "obTeamWhyNoFront",
    "no-back": "obTeamWhyNoBack",
    "no-infra": "obTeamWhyNoInfra",
    "no-ui": "obTeamWhyNoUi",
    "no-web": "obTeamWhyNoWeb",
  } as const;
  const why = (r: RoleChoice) =>
    t(whyKey[r.why as keyof typeof whyKey] ?? "obTeamWhyOwn", { detail: r.detail.join(", ") });
  const models = ["", "haiku", "sonnet", "opus"];
  const efforts = ["", "low", "medium", "high", "xhigh", "max"];

  onMount(load);

  async function load() {
    try {
      team = await invoke<Team>("onboarding_team", { project, choices });
    } catch (e) {
      error = String(e);
    }
  }

  async function choose(change: Partial<Choices>) {
    choices = { ...choices, ...change };
    preview = null;
    await load();
  }

  function toggle(r: RoleChoice) {
    choose({ keep: { ...choices.keep, [r.role]: !r.keep } });
  }

  function decide(key: string, d: TeamDecision) {
    decisions[key] = d;
    preview = null;
  }

  function modify(key: string) {
    editing = editing === key ? "" : key;
    const role = key.startsWith("roles.") ? key.slice(6) : "";
    if (role && team) {
      const s: TeamSettings = changes.roles[role] ?? team.proposal.roles[role];
      draft = { writes: (s.writes ?? []).join("\n"), model: s.model ?? "", effort: s.effort ?? "" };
    }
  }

  function keepDraft(role: string) {
    const writes = globList(draft.writes);
    changes.roles[role] = {
      writes: writes.length ? writes : undefined,
      model: draft.model || undefined,
      effort: draft.effort || undefined,
    };
    decide(`roles.${role}`, "accepted");
    editing = "";
  }

  function pickWorkflow(name: string) {
    changes.workflow = name;
    decide("workflow", "accepted");
  }

  async function apply() {
    if (!team) return;
    busy = true;
    error = "";
    try {
      if (!accepted) {
        ondecided(await invoke("onboarding_decide", { project, proposal: "team", decision: "rejected", detail: "" }));
      } else {
        preview = await invoke("onboarding_team_preview", { project, edit: teamEdit(team.proposal, decisions, changes) });
      }
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function save() {
    if (!team) return;
    const edit = teamEdit(team.proposal, decisions, changes);
    busy = true;
    error = "";
    try {
      ondecided(
        await invoke("onboarding_team_save", { project, edit, replace: replacesHand(edit, team.status) }),
      );
      preview = null;
      decisions = {};
      changes = { roles: {} };
      editing = "";
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

{#snippet head(key: string, title: string)}
  <header class="part-head">
    <b>{title}</b>
    {#if team?.status[key] === "same"}<StatusPill status="done" label={t("obTeamSame")} />{/if}
    <span class="grow"></span>
    {#if keys.includes(key)}
      <button class="btn sm" aria-pressed={decisions[key] === "accepted"} disabled={busy} onclick={() => decide(key, "accepted")}>
        <Check size={14} aria-hidden="true" />{t("convAccept")}
      </button>
      <button class="btn sm" aria-pressed={decisions[key] === "rejected"} disabled={busy} onclick={() => decide(key, "rejected")}>
        <X size={14} aria-hidden="true" />{t("convReject")}
      </button>
    {/if}
    <button class="btn sm quiet" aria-expanded={editing === key} onclick={() => modify(key)}>{t("obEdit")}</button>
  </header>
  {#if team?.status[key] === "edited"}
    <p class="notice warn" role="status">{t("obTeamEdited")}</p>
  {/if}
{/snippet}

{#snippet segmented(label: string, options: [string, string][], value: string, onpick: (v: string) => void)}
  <div class="choice">
    <span class="muted">{label}</span>
    <div class="segmented" role="radiogroup" aria-label={label}>
      {#each options as [v, text] (v)}
        <button type="button" role="radio" aria-checked={value === v} disabled={busy} onclick={() => onpick(v)}>{text}</button>
      {/each}
    </div>
  </div>
{/snippet}

<article class="proposal">
  <h3><Users size={15} aria-hidden="true" />{t("obTeamTitle")} {#if decision}<StatusPill status={decision} />{/if}</h3>
  <p class="hint">{t("obTeamHint")}</p>
  {#if error}<Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>{/if}

  {#if !team}
    {#if !error}<p class="notice" role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" /></p>{/if}
  {:else}
    {#if team.proposal.team}
      <section class="part">
        {@render head("team", t("obTeamComposition"))}
        <ul class="why">
          {#each team.roles.filter((r) => r.keep) as r (r.role)}
            <li>
              <span class="badge">{r.role}</span>
              <span>{why(r)}</span>
              {#if r.chosen}<span class="muted">· {t("obTeamChosen")}</span>{/if}
            </li>
          {/each}
        </ul>
        <div class="squads">
          {#each squads as s (s.id)}
            <div class="squad">
              <div class="squad-head"><b>{t("obTeamSquad", { id: s.id })}</b><span class="muted">{t("obTeamLead", { lead: s.lead })}</span></div>
              <div class="chips">{#each members(s.roles) as m (m)}<span class="badge">{m}</span>{/each}</div>
              {#if s.repos.length || s.paths.length}
                <div class="chips">{#each [...s.repos, ...s.paths] as p (p)}<code>{p}</code>{/each}</div>
              {/if}
            </div>
          {/each}
          {#if staff.length}
            <div class="squad">
              <div class="squad-head"><b>{t("obTeamStaff")}</b></div>
              <div class="chips">{#each staff as r (r)}<span class="badge">{r}</span>{/each}</div>
            </div>
          {/if}
        </div>
        {#if left.length}
          <p class="small muted">
            {t("obTeamLeftOut", { roles: left.map((r) => `${r.role} (${why(r)})`).join(" · ") })}
          </p>
        {/if}
        {#if team.steps.length}
          <p class="small muted">{t("obTeamSteps", { roles: team.steps.join(", ") })}</p>
        {/if}
        {#if editing === "team"}
          <div class="edit">
            <div class="choice">
              <span class="muted">{t("obTeamRoles")}</span>
              <div class="chips" role="group" aria-label={t("obTeamRoles")}>
                {#each team.roles as r (r.role)}
                  <label class="chip" class:on={r.keep}>
                    <input type="checkbox" checked={r.keep} disabled={busy} onchange={() => toggle(r)} />
                    {r.role}
                  </label>
                {/each}
              </div>
            </div>
            {@render segmented(
              t("obTeamSize"),
              ["1", "2", "3"].map((n) => [n, n]),
              String(team.choices.size ?? 1),
              (v) => choose({ size: Number(v) }),
            )}
            {#if team.splittable}
              {@render segmented(
                t("obTeamSplit"),
                [
                  ["one", t("obTeamOneSquad")],
                  ["split", t("obTeamPerPart")],
                ],
                team.choices.split ? "split" : "one",
                (v) => choose({ split: v === "split" }),
              )}
            {/if}
            {@render segmented(
              t("obTeamModels"),
              [
                ["", t("obTeamEach")],
                ["sonnet", "sonnet"],
                ["opus", "opus"],
              ],
              team.choices.model ?? "",
              (v) => choose({ model: v }),
            )}
          </div>
        {/if}
      </section>
    {/if}

    {#if team.proposal.workflow}
      <section class="part">
        {@render head("workflow", t("obTeamWorkflow"))}
        <p><code>{workflow}</code> <span class="muted">{t(workflow === "squads" ? "obTeamSquads" : "obTeamDefault")}</span></p>
        {#if editing === "workflow"}
          <div class="edit">
            {@render segmented(
              t("obTeamWorkflow"),
              [
                ["default", "default"],
                ["squads", "squads"],
              ],
              workflow,
              pickWorkflow,
            )}
          </div>
        {/if}
      </section>
    {/if}

    {#each Object.entries(team.proposal.roles) as [role, proposed] (role)}
      {@const key = `roles.${role}`}
      {@const s = changes.roles[role] ?? proposed}
      <section class="part">
        {@render head(key, t("obTeamSettings", { role }))}
        <dl>
          {#if s.writes || !(s.model || s.effort)}
            <dt>{t("obTeamWrites")}</dt>
            <dd>{#each s.writes ?? [] as g (g)}<code>{g}</code>{:else}<span class="muted">—</span>{/each}</dd>
          {/if}
          {#if s.model}<dt>{t("obTeamModel")}</dt><dd>{s.model}</dd>{/if}
          {#if s.effort}<dt>{t("obTeamEffort")}</dt><dd>{s.effort}</dd>{/if}
        </dl>
        {#if editing === key}
          <div class="edit">
            <label class="choice">
              <span class="muted">{t("obTeamWrites")}</span>
              <textarea class="field mono" rows="3" bind:value={draft.writes} aria-describedby="writes-help-{role}"></textarea>
              <span class="small muted" id="writes-help-{role}">{t("obTeamWritesHelp")}</span>
            </label>
            <div class="selects">
              <label class="choice">
                <span class="muted">{t("obTeamModel")}</span>
                <select class="field" bind:value={draft.model}>
                  {#each models as m (m)}<option value={m}>{m || t("obTeamOwn")}</option>{/each}
                </select>
              </label>
              <label class="choice">
                <span class="muted">{t("obTeamEffort")}</span>
                <select class="field" bind:value={draft.effort}>
                  {#each efforts as e (e)}<option value={e}>{e || t("obTeamOwn")}</option>{/each}
                </select>
              </label>
            </div>
            <div class="row-actions">
              <span class="grow"></span>
              <button class="btn sm" onclick={() => (editing = "")}>{t("cancel")}</button>
              <button class="btn sm primary" onclick={() => keepDraft(role)}>
                <Check size={14} aria-hidden="true" />{t("convAccept")}
              </button>
            </div>
          </div>
        {/if}
      </section>
    {/each}

    {#if preview}
      <ClaudeChange preview={{ ...preview, hash: "" }} saving={busy} onsave={save} oncancel={() => (preview = null)} />
    {:else if keys.length}
      <div class="row-actions">
        <span class="muted">{t("convReviewCounts", { accepted, total: keys.length, pending })}</span>
        <span class="grow"></span>
        <button class="btn sm primary" disabled={busy || pending > 0} title={pending ? t("convDecideFirst") : undefined} onclick={apply}>
          {#if busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
          {t("convApply", { count: accepted })}
        </button>
      </div>
    {/if}
  {/if}
</article>

<style>
  .proposal {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  p {
    margin: 0;
  }

  .hint {
    color: var(--muted);
    line-height: 1.5;
  }

  .part {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }

  .part-head,
  .row-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .part-head b {
    font-weight: 600;
  }

  .part-head .btn[aria-pressed="true"] {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .grow {
    flex: 1;
  }

  .why {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .why li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    overflow-wrap: anywhere;
  }

  .squads {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 8px;
  }

  .squad {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
  }

  .squad-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 6px;
    font-size: var(--text-sm);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-pill);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .chip input {
    margin: 0;
  }

  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .small {
    font-size: var(--text-xs);
  }

  .edit {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--chrome);
  }

  .choice {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  textarea.field {
    height: auto;
    padding: 6px 8px;
  }

  .selects {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .segmented {
    flex-wrap: wrap;
    align-self: flex-start;
  }

  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 4px 10px;
    margin: 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
    overflow-wrap: anywhere;
  }

  code {
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
</style>
