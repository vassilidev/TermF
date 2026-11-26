<script lang="ts">
  import Database from "@lucide/svelte/icons/database";
  import Layers from "@lucide/svelte/icons/layers";
  import { t } from "$lib/i18n";
  import type { Connection, Kind } from "$lib/ops-logic";
  import { ops, sqlRecipe } from "$lib/ops.svelte";

  /** The project's databases and Redis at the top of the SQL studio and of the Redis browser: one opens where it reads. */
  const stores = $derived(ops.connections.filter((c) => c.kind === "redis" || sqlRecipe(c)));
  const picked = $derived(ops.tab === "redis" ? ops.redis.connection : ops.sql.connection);
  const kindLabel = (k: Kind) => t(`opsKind${k[0].toUpperCase()}${k.slice(1)}` as Parameters<typeof t>[0]);

  function pick(c: Connection) {
    if (c.kind === "redis") ops.redis.connection = c.name;
    else ops.sql.connection = c.name;
    ops.tab = c.kind === "redis" ? "redis" : "sql";
  }
</script>

{#if stores.length}
  <nav class="stores" aria-label={t("opstidyStores")}>
    {#each stores as c (c.name)}
      {@const Icon = c.kind === "redis" ? Layers : Database}
      <button class="store" aria-pressed={c.name === picked} onclick={() => pick(c)}>
        <Icon size={15} aria-hidden="true" />
        <span class="text">
          <span class="name mono">{c.name}</span>
          <span class="meta"><span class="badge env" class:prod={c.env === "prod"}>{c.env}</span>{kindLabel(c.kind)}</span>
        </span>
      </button>
    {/each}
  </nav>
{/if}

<style>
  .stores {
    display: flex;
    flex-shrink: 0;
    gap: 8px;
    padding: 12px 20px;
    overflow-x: auto;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  .store {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 10px;
    min-width: 150px;
    padding: 7px 12px 7px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
    color: var(--muted);
    text-align: left;
    cursor: pointer;
    transition:
      border-color var(--dur-fast) ease-out,
      background-color var(--dur-fast) ease-out;
  }

  .store:hover {
    border-color: var(--border-strong);
  }

  .store[aria-pressed="true"] {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .name {
    color: var(--text);
    font-size: var(--text-sm);
    font-weight: 600;
    white-space: nowrap;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .badge.prod {
    border-color: var(--danger);
    background: var(--danger);
    color: var(--danger-text);
  }
</style>
