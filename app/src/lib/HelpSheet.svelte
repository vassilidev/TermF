<script lang="ts">
  import { onMount } from "svelte";
  import Compass from "@lucide/svelte/icons/compass";
  import Keyboard from "@lucide/svelte/icons/keyboard";
  import Signpost from "@lucide/svelte/icons/signpost";
  import X from "@lucide/svelte/icons/x";
  import { isMac } from "$lib/files.svelte";
  import { t } from "$lib/i18n";
  import { sheet } from "$lib/keymap-logic";
  import { current } from "$lib/keymap.svelte";
  import { showSettings } from "$lib/settings.svelte";
  import { ui, type ViewId } from "$lib/state.svelte";
  import { startTour } from "$lib/tour.svelte";
  import { views } from "$lib/views";

  type Key = Parameters<typeof t>[0];

  const about: Partial<Record<ViewId, [Key, Key]>> = {
    home: ["ux3t1Home", "ux3t4HelpHome"],
    terminals: ["viewTerminals", "t6bHelpTerminals"],
    conversations: ["chatView", "t6bHelpConversations"],
    tickets: ["viewTickets", "t6bHelpTickets"],
    files: ["viewFiles", "t6bHelpFiles"],
    git: ["gitView", "gitHelp"],
    library: ["viewLibrary", "t6bHelpLibrary"],
    ci: ["viewCi", "t6bHelpCi"],
    design: ["designView", "designHelp"],
    scheduler: ["schedulerView", "t6bHelpScheduler"],
    claude: ["claudeViewTitle", "t6bHelpClaude"],
    ssh: ["viewSsh", "t6bHelpSsh"],
    settings: ["settings", "t6bHelpSettings"],
    project: ["ux3t3Config", "ux3t4HelpConfig"],
    newProject: ["newProject", "t6bHelpOnboarding"],
    vault: ["vaultTitle", "vaultHelp"],
    vaultAll: ["uxfT1VaultAll", "vaultHelp"],
    ops: ["opsView", "opsIntro"],
  };

  let dialog: HTMLDialogElement;
  const here = about[ui.view];
  const icon = views.find((v) => v.id === ui.view)?.icon;
  const linked = views.filter((v) => v.group === "linked");
  const groups = $derived(sheet(current(), isMac, ui.view));
  const mod = isMac ? "⌘" : "Ctrl";

  onMount(() => {
    dialog.showModal();
    return () => dialog.open && dialog.close();
  });

  function go(run: () => void) {
    ui.help = false;
    run();
  }
</script>

<dialog
  bind:this={dialog}
  class="help"
  aria-labelledby="help-title"
  onclose={() => (ui.help = false)}
  onclick={(e) => e.target === dialog && dialog.close()}
>
  <header>
    <h2 id="help-title">{t("t6bHelp")}</h2>
    <button class="icon-btn" aria-label={t("close")} title={t("close")} onclick={() => dialog.close()}>
      <X size={16} aria-hidden="true" />
    </button>
  </header>

  <div class="body">
    {#if here}
      {@const Icon = icon}
      <section class="here" aria-labelledby="help-here">
        <h3 id="help-here">{#if Icon}<Icon size={15} aria-hidden="true" />{/if}{t(here[0])}</h3>
        <p>{t(here[1])}</p>
      </section>
    {/if}

    <ul class="tips">
      <li>{t("ux3t4TipElsewhere")}</li>
      <li>{t("t6bTipBeside", { key: mod })}</li>
      <li>{t("t6bTipSelect", { key: mod })}</li>
    </ul>

    <section class="linked" aria-labelledby="help-linked">
      <h3 id="help-linked"><Signpost size={15} aria-hidden="true" />{t("uxfT1HelpLinked")}</h3>
      {#each linked as view (view.id)}
        <button class="open" onclick={() => go(view.show)}>
          <view.icon size={16} aria-hidden="true" />
          <span><b>{t(view.label)}</b>{#if view.hint}<span>{t(view.hint)}</span>{/if}</span>
        </button>
      {/each}
    </section>

    <section aria-labelledby="help-keys">
      <h3 id="help-keys"><Keyboard size={15} aria-hidden="true" />{t("shortcuts")}</h3>
      {#each groups as group (group.group)}
        <h4>{t(group.label)}</h4>
        <dl>
          {#each group.rows as row (row.id)}
            <dt>{t(row.label)}</dt>
            <dd>
              {#each row.keys as key (key)}<kbd>{key}</kbd>{/each}
            </dd>
          {/each}
        </dl>
      {/each}
    </section>
  </div>

  <footer>
    <button class="btn sm" onclick={() => go(startTour)}>
      <Compass size={14} aria-hidden="true" />{t("wsIntroAgain")}
    </button>
    <button class="btn sm" onclick={() => go(() => showSettings("keys"))}>
      <Keyboard size={14} aria-hidden="true" />{t("t6bEditKeys")}
    </button>
  </footer>
</dialog>

<style>
  .help {
    display: flex;
    flex-direction: column;
    width: min(420px, calc(100vw - 32px));
    max-width: none;
    height: 100vh;
    max-height: none;
    margin: 0 0 0 auto;
    padding: 0;
    border: 0;
    border-left: 1px solid var(--border);
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--shadow-3);
  }

  .help[open] {
    animation: enter var(--dur-slow) var(--ease-out);
  }

  .help::backdrop {
    background: var(--scrim, rgb(10 12 20 / 0.3));
  }

  @keyframes enter {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
  }

  header {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: space-between;
    height: 48px;
    padding: 0 10px 0 20px;
    border-bottom: 1px solid var(--border);
  }

  h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 650;
  }

  .body {
    flex: 1;
    min-height: 0;
    padding: 18px 20px 24px;
    overflow-y: auto;
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 6px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  h3 > :global(svg) {
    color: var(--accent-ink);
  }

  .here p {
    margin: 0;
    color: var(--muted);
    line-height: 1.55;
  }

  .tips {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 16px 0 22px;
    padding: 10px 12px 10px 28px;
    border-radius: var(--radius);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .linked {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 22px;
  }

  .open {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    text-align: left;
  }

  .open:hover {
    background: var(--hover);
  }

  .open > :global(svg) {
    flex-shrink: 0;
    color: var(--accent-ink);
  }

  .open > span {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .open b {
    font-weight: 500;
  }

  .open b + span {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  h4 {
    margin: 16px 0 6px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  dl {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    margin: 0;
  }

  dt,
  dd {
    padding: 5px 0;
    border-bottom: 1px solid var(--border);
  }

  dt {
    min-width: 0;
    padding-right: 12px;
  }

  dd {
    display: flex;
    justify-content: flex-end;
    gap: 4px;
    margin: 0;
  }

  footer {
    display: flex;
    flex-shrink: 0;
    gap: 8px;
    padding: 10px 20px;
    border-top: 1px solid var(--border);
  }
</style>
