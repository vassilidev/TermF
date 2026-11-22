<script lang="ts">
  import Braces from "@lucide/svelte/icons/braces";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import StickyNote from "@lucide/svelte/icons/sticky-note";
  import { t } from "$lib/i18n";
  import type { Merged } from "$lib/vault-logic";

  /** What importing a TermF export does, element by element: titles, never a value. */
  let { rows, projectName }: { rows: Merged[]; projectName: (id: string) => string } = $props();

  let all = $state(false);
  const changing = $derived(rows.filter((r) => r.change !== "unchanged"));
  const same = $derived(rows.length - changing.length);
  const label = {
    added: "vault3ChangeAdded",
    changed: "vault3ChangeChanged",
    deleted: "vault3ChangeDeleted",
    conflict: "vault3ChangeConflict",
    unchanged: "vault3ChangeUnchanged",
  } as const;
</script>

{#if changing.length || all}
  <table class="rows">
    <thead>
      <tr><th>{t("vault3ColTitle")}</th><th>{t("vault3ColProject")}</th><th>{t("vault3ColChange")}</th></tr>
    </thead>
    <tbody>
      {#each all ? rows : changing as row (row.key)}
        <tr>
          <td>
            <span class="title">
              {#if row.scope}<Braces size={13} aria-hidden="true" />{:else if row.kind === "note"}<StickyNote size={13} aria-hidden="true" />{:else}<KeyRound size={13} aria-hidden="true" />{/if}
              <span class:mono={!!row.scope}>{row.title}</span>
              {#if row.scope}<span class="badge">{row.scope}</span>{/if}
            </span>
          </td>
          <td class="muted">{row.project ? projectName(row.project) : t("vaultGlobal")}</td>
          <td><span class="change {row.change}">{t(label[row.change])}</span></td>
        </tr>
      {/each}
    </tbody>
  </table>
{:else}
  <p class="notice">{t("importNothing")}</p>
{/if}
{#if same && changing.length}
  <button class="btn sm quiet more" onclick={() => (all = !all)}>{all ? t("hideUnchanged") : t("showUnchanged", { count: same })}</button>
{/if}

<style>
  .rows {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  th {
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-align: left;
    text-transform: uppercase;
  }

  td {
    padding: 7px 10px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }

  .title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 500;
  }

  .title :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .change {
    font-weight: 600;
  }

  .change.added,
  .change.changed {
    color: var(--done);
  }

  .change.deleted {
    color: var(--danger);
  }

  .change.conflict {
    color: var(--warning);
  }

  .change.unchanged {
    color: var(--muted);
  }

  .more {
    align-self: flex-start;
  }
</style>
