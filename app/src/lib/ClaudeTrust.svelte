<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { t } from "$lib/i18n";
  import { library } from "$lib/library.svelte";
  import { trustState, type RepoTrust } from "$lib/onboarding-logic";
  import { shortPath } from "$lib/state.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";

  /** Claude Code's trust in the project's repositories: only ever given by ticking, never by TermF alone. */
  let { project }: { project: string } = $props();

  let repos = $state<RepoTrust[] | null>(null);
  let agreed = $state(false);
  let busy = $state(false);
  let error = $state("");
  const status = $derived(repos ? trustState(repos) : "off");

  onMount(() => {
    invoke<RepoTrust[]>("claude_trust", { project }).then(
      (r) => (repos = r),
      (e) => (error = String(e)),
    );
  });

  async function set(on: boolean) {
    busy = true;
    error = "";
    try {
      repos = await invoke<RepoTrust[]>("claude_trust_set", { project, on });
      agreed = false;
      // The skills the repositories' packages ship start on or off with the trust.
      library.version++;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="trust">
  <p class="intro">{t("obTrustIntro")}</p>
  {#if error}
    <p class="notice error" role="alert">{error}</p>
  {/if}
  {#if repos}
    <ul class="repos">
      {#each repos as r (r.path)}
        <li>
          <b>{r.name}</b>
          <span class="mono" title={r.root}>{shortPath(r.root)}</span>
          {#if r.allow}<span class="loads">{t("obTrustRules", { count: r.allow })}</span>{/if}
          {#if r.mcp}<span class="loads">{t("obTrustMcp", { count: r.mcp })}</span>{/if}
          <StatusPill status={r.trusted ? "accepted" : "off"} label={t(r.trusted ? "obTrusted" : "obUntrusted")} />
        </li>
      {/each}
    </ul>
    {#if status !== "on"}
      <label class="agree">
        <input type="checkbox" bind:checked={agreed} disabled={busy} />
        {t("obTrustConfirm")}
      </label>
    {/if}
    <div class="actions">
      <span class="note">{t("obTrustNote")}</span>
      {#if status !== "off"}
        <button type="button" class="btn sm" disabled={busy} onclick={() => set(false)}>{t("obTrustRemove")}</button>
      {/if}
      {#if status !== "on"}
        <button type="button" class="btn sm" disabled={!agreed || busy} onclick={() => set(true)}>
          {#if busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
          {t("obTrustApply")}
        </button>
      {/if}
    </div>
  {:else if !error}
    <Skeleton shape="list" rows={2} />
  {/if}
</div>

<style>
  .trust {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .intro {
    margin: 0;
    color: var(--muted);
    line-height: 1.5;
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
    padding: 0 12px 0 14px;
  }

  .repos li + li {
    border-top: 1px solid var(--border);
  }

  .repos b {
    font-weight: 500;
  }

  .repos .mono {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .loads {
    color: var(--warning);
    font-size: var(--text-sm);
  }

  .agree {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 500;
    cursor: pointer;
  }

  .agree input {
    margin: 0;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .note {
    flex: 1;
    min-width: 200px;
    color: var(--muted);
    font-size: var(--text-sm);
  }
</style>
