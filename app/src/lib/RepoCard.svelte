<script lang="ts">
  import type { Snippet } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import ForgeIcon from "$lib/ui/ForgeIcon.svelte";
  import { ago, t } from "$lib/i18n";
  import type { Card } from "$lib/setup-logic";
  import { shortPath } from "$lib/state.svelte";

  /** A repository with what tells it apart: its forge, path, branch, changes, activity and stack. */
  let {
    card,
    selected,
    elsewhere = [],
    trailing,
  }: {
    card: Card;
    /** Shows the check box; left out where the card is not picked. */
    selected?: boolean;
    /** The other projects that list it. */
    elsewhere?: string[];
    trailing?: Snippet;
  } = $props();

  const MAX_TAGS = 3;
  const read = $derived(card.stack !== undefined);
  const tags = $derived(card.stack ?? []);
  // The usual branch says nothing: only another one is worth a look.
  const usual = (branch: string) => branch === "main" || branch === "master";
</script>

<div class="repo" class:selected>
  {#if selected !== undefined}
    <span class="box" aria-hidden="true">{#if selected}<Check size={12} strokeWidth={3} />{/if}</span>
  {/if}
  <span class="forge" title={card.remote?.host ?? t("setupNoRemote")}><ForgeIcon host={card.remote?.host} /></span>
  <span class="main">
    <span class="line">
      <b class="name" title={card.path}>{card.name}</b>
      {#each elsewhere as name (name)}
        <span class="badge in">{t("setupInProject", { name })}</span>
      {/each}
    </span>
    <span class="line meta">
      {#if card.remote}
        <span class="slug" title={card.path}>{card.remote.slug}</span>
      {:else}
        <span class="slug path mono" title={card.path}>{shortPath(card.path)}</span>
      {/if}
      {#if card.branch && !usual(card.branch)}
        <span class="branch"><GitBranch size={12} aria-hidden="true" /><span class="mono">{card.branch}</span></span>
      {/if}
      {#if card.changes}
        <span class="changes">{t("setupChanges", { count: card.changes })}</span>
      {/if}
      {#if card.modified}<span class="when">{ago(card.modified)}</span>{/if}
    </span>
  </span>
  <span class="tags" aria-label={read ? t("setupStackOf", { list: tags.join(", ") || "—" }) : undefined}>
    {#if !read}
      <span class="skel"></span><span class="skel short"></span>
    {:else}
      {#each tags.slice(0, MAX_TAGS) as tag (tag)}<span class="badge">{tag}</span>{/each}
      {#if tags.length > MAX_TAGS}<span class="more" title={tags.slice(MAX_TAGS).join(", ")}>+{tags.length - MAX_TAGS}</span>{/if}
    {/if}
  </span>
  {@render trailing?.()}
</div>

<style>
  .repo {
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 0 12px;
    min-height: 58px;
    padding: 9px 14px;
  }

  /* Without check box, the forge's mark starts the row. */
  .repo:not(:has(.box)) {
    grid-template-columns: auto minmax(0, 1fr) auto auto;
  }

  .box {
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border: 1.5px solid var(--field-border);
    border-radius: var(--radius-sm);
    background: var(--panel);
    color: var(--accent-text);
    transition:
      background-color var(--dur-fast) ease-out,
      border-color var(--dur-fast) ease-out;
  }

  .selected .box {
    border-color: var(--accent);
    background: var(--accent);
  }

  .forge {
    display: inline-grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--chrome);
    color: var(--text);
  }

  .main {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .name {
    overflow: hidden;
    font-size: var(--text-md);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .line .path {
    flex-shrink: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .slug {
    flex-shrink: 0;
    max-width: 50%;
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .in {
    align-self: center;
    border-color: transparent;
    background: var(--hover);
  }

  .meta {
    align-items: center;
    gap: 6px 12px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .branch {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    max-width: 180px;
  }

  .branch .mono {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .changes {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
    color: var(--warning);
    white-space: nowrap;
  }

  .changes::before {
    content: "";
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--waiting);
  }

  .when {
    flex-shrink: 0;
    white-space: nowrap;
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 4px;
    max-width: 280px;
  }

  .more {
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
    line-height: 18px;
  }

  .skel {
    width: 44px;
    height: 18px;
    border-radius: var(--radius-pill);
    background: var(--hover);
  }

  .skel.short {
    width: 30px;
  }
</style>
