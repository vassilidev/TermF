<script lang="ts">
  import SessionTile from "$lib/SessionTile.svelte";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";

  const cols = $derived(
    Math.max(1, Math.ceil(Math.sqrt(app.focused === null ? app.sessions.length : 1))),
  );
</script>

<main class="grid" style:--cols={cols}>
  {#each app.sessions as session (session.id)}
    <SessionTile {session} />
  {:else}
    <p class="empty muted">{t("empty")}</p>
  {/each}
</main>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    grid-auto-rows: minmax(0, 1fr);
    gap: 6px;
    padding: 6px;
    min-height: 0;
    overflow: hidden;
  }

  .empty {
    grid-column: 1 / -1;
    place-self: center;
  }
</style>
