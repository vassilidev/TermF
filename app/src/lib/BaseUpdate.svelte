<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import GitMerge from "@lucide/svelte/icons/git-merge";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Banner from "$lib/ui/Banner.svelte";
  import { t } from "$lib/i18n";
  import { baseUpdate } from "$lib/run-logic";
  import type { TeamRun } from "$lib/team-run-logic";
  import { upsertRun } from "$lib/team.svelte";
  import type { Run } from "$lib/tickets.svelte";

  /** A delivered run whose pull request conflicts with its base, or is behind it, and the team's way to update it. */
  let { run, team }: { run: Run; team?: TeamRun } = $props();

  type Merge = { mergeable: string; state: string; conflict: boolean; behind: boolean };
  let merge = $state<Merge | null>(null);
  let busy = $state(false);
  let error = $state("");
  let request = 0;

  const open = $derived(run.status === "delivered" && !!run.prUrl && !run.prState);
  const said = $derived(baseUpdate(run, merge, team));
  const base = $derived((run.base ?? "").replace(/^origin\//, ""));

  // Asked again when the run or its team changes: a new delivery may have brought the branch up to date. When GitHub
  // does not answer (offline, signed out), the page says nothing of it.
  $effect(() => {
    void run.updated;
    void team?.state;
    const mine = ++request;
    if (!open) {
      merge = null;
      return;
    }
    invoke<Merge | null>("tickets_pr_merge", { id: run.id }).then(
      (m) => {
        if (mine === request) merge = m;
      },
      () => {
        if (mine === request) merge = null;
      },
    );
  });

  async function update() {
    busy = true;
    error = "";
    try {
      upsertRun(await invoke<TeamRun>("team_update_base", { run: run.id }));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if said}
  <Banner kind="warn" icon={GitMerge}>
    <b>{t(said.state === "conflict" ? "integratorConflict" : "integratorBehind", { base })}</b>
    {t(said.action ? "integratorHelp" : "integratorSolo", { base })}
    {#snippet action()}
      {#if said.action}
        <button class="btn sm" onclick={update} disabled={busy}>
          {#if busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("integratorUpdate")}
        </button>
      {/if}
    {/snippet}
  </Banner>
  {#if error}<p class="notice error" role="alert">{error}</p>{/if}
{/if}
