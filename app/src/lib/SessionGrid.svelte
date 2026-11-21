<script lang="ts">
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import BroadcastBar from "$lib/BroadcastBar.svelte";
  import QuickTerminal from "$lib/QuickTerminal.svelte";
  import SessionTile from "$lib/SessionTile.svelte";
  import { keys } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { app, openLayout, ticked } from "$lib/state.svelte";

  const cols = $derived(
    Math.max(1, Math.ceil(Math.sqrt(app.focused === null ? app.sessions.length : 1))),
  );

  const shortcuts = $derived([
    [keys.palette, t("palette")],
    [keys.next, t("nextWaiting")],
    [`${keys.tile(1)}…9`, t("focusTiles")],
    [keys.maximize, t("maximizeActive")],
    [keys.quick, t("quickTerminal")],
  ]);
</script>

<main class="main">
  <QuickTerminal />
  {#if app.sessions.length}
    <div class="grid" class:broadcasting={ticked.size > 0} style:--cols={cols}>
      {#each app.sessions as session, index (session.id)}
        <SessionTile {session} {index} />
      {/each}
    </div>
    <BroadcastBar />
  {:else}
    <div class="empty">
      <h1>{t("emptyTitle")}</h1>
      <ol class="steps">
        <li><span class="n" aria-hidden="true">1</span>{t("emptyStep1")}</li>
        <li><span class="n" aria-hidden="true">2</span>{t("emptyStep2")}</li>
      </ol>
      {#if app.layouts.length}
        <div class="layouts">
          <p>{t("emptyLayouts")}</p>
          <div class="chips">
            {#each app.layouts as layout (layout.name)}
              <button class="btn sm" onclick={() => openLayout(layout)}>
                <LayoutGrid size={14} aria-hidden="true" />{layout.name}
              </button>
            {/each}
          </div>
        </div>
      {/if}
      <section class="keys" aria-labelledby="shortcuts-title">
        <h2 id="shortcuts-title">{t("shortcuts")}</h2>
        <dl>
          {#each shortcuts as [key, label] (label)}
            <dt><kbd>{key}</kbd></dt>
            <dd>{label}</dd>
          {/each}
        </dl>
      </section>
    </div>
  {/if}
</main>

<style>
  .main {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .grid {
    flex: 1;
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    grid-auto-rows: minmax(0, 1fr);
    gap: 8px;
    padding: 8px;
    min-height: 0;
    overflow: hidden;
  }

  .empty {
    display: flex;
    flex-direction: column;
    gap: 20px;
    width: min(440px, calc(100% - 48px));
    margin: auto;
    padding-bottom: 6vh;
  }

  h1 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .steps {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .steps li {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .n {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: var(--text-sm);
    font-weight: 650;
  }

  .layouts p {
    margin: 0 0 8px;
    color: var(--muted);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .keys {
    padding-top: 20px;
    border-top: 1px solid var(--border);
  }

  h2 {
    margin: 0 0 10px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 14px;
    align-items: center;
    margin: 0;
  }

  dt {
    justify-self: start;
  }

  dd {
    margin: 0;
    color: var(--muted);
  }
</style>
