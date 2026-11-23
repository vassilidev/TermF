<script lang="ts">
  import Columns2 from "@lucide/svelte/icons/columns-2";
  import Copy from "@lucide/svelte/icons/copy";
  import Equal from "@lucide/svelte/icons/equal";
  import SquarePlus from "@lucide/svelte/icons/square-plus";
  import Maximize2 from "@lucide/svelte/icons/maximize-2";
  import Minimize2 from "@lucide/svelte/icons/minimize-2";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import Rows2 from "@lucide/svelte/icons/rows-2";
  import Smartphone from "@lucide/svelte/icons/smartphone";
  import UserRound from "@lucide/svelte/icons/user-round";
  import X from "@lucide/svelte/icons/x";
  import Action from "$lib/Action.svelte";
  import ContextGauge from "$lib/ContextGauge.svelte";
  import RemoteSwitch from "$lib/RemoteSwitch.svelte";
  import SessionLoops from "$lib/SessionLoops.svelte";
  import Terminal, { selectionOf } from "$lib/Terminal.svelte";
  import ContextMenu, { menuKey, menuPoint, type ContextItem } from "$lib/ui/ContextMenu.svelte";
  import { phase } from "$lib/remote-logic";
  import { setRemote, turnOn } from "$lib/remote.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { closeSplit, focusSession, keys, openLauncher, splitActive, toggleMaximize } from "$lib/commands";
  import { isMac } from "$lib/files.svelte";
  import { t } from "$lib/i18n";
  import { display } from "$lib/keymap.svelte";
  import type { Rect, Side } from "$lib/split-logic";
  import { activate, equalize, tabOf, terminals } from "$lib/tabs.svelte";
  import { dropping } from "$lib/workspace.svelte";
  import { liveLoops, tileName } from "$lib/tile-logic";
  import { grid, loopClock, onScreen, resetContext, ticked, type Session } from "$lib/state.svelte";

  /**
   * `rect`: where the split sits in its tab, as fractions of the tab; none while the tab is not shown. `alone`: the
   * only split of its tab, which has nothing to zoom over.
   */
  let { session, rect, zoomed, alone }: { session: Session; rect?: Rect; zoomed: boolean; alone: boolean } = $props();

  const failed = $derived(!!session.exitCode);
  const name = $derived(tileName(session.title, session.agent ?? "shell"));
  const asking = $derived(terminals.asking === session.id);
  const remote = $derived(session.running ? (session.remote ?? null) : null);
  const loops = $derived(session.running ? liveLoops(session.loops, loopClock.now) : []);
  const pct = (n: number) => `${n * 100}%`;

  const remoteOn = () => turnOn(session.id);
  let confirm = $state<HTMLButtonElement>();

  // ⌘W on a running session: Enter, or ⌘W again, closes it.
  $effect(() => {
    if (asking) confirm?.focus();
  });

  let menuOpen = $state(false);
  let menuAt = $state<{ x: number; y: number } | null>(null);
  let menuTrigger = $state<HTMLElement>();
  let head = $state<HTMLElement>();
  let menuItems = $state<ContextItem[]>([]);

  /** A split's actions at the pointer: the same tool beside it in one gesture, or another session from the launcher. */
  function openMenu(e: MouseEvent | KeyboardEvent) {
    e.preventDefault();
    const selection = selectionOf(session.id);
    menuTrigger = (document.activeElement as HTMLElement | null) ?? undefined;
    // From the keyboard, under the header rather than at the foot of the tile.
    const top = e instanceof KeyboardEvent ? head?.getBoundingClientRect() : undefined;
    menuAt = top ? { x: top.left + 8, y: top.bottom } : menuPoint(e);
    menuItems = [
      ...(selection ? [{ label: t("wsCopy"), icon: Copy, kbd: isMac ? "⌘C" : "", run: () => void navigator.clipboard.writeText(selection) }] : []),
      { label: t("t6aSplitRight"), icon: Columns2, kbd: display("split_right"), divider: true, run: () => splitActive("right", session.id) },
      { label: t("t6aSplitDown"), icon: Rows2, kbd: display("split_down"), run: () => splitActive("down", session.id) },
      { label: t("wsSplitWith"), icon: SquarePlus, run: () => split("right") },
      ...(alone
        ? []
        : [
            { label: zoomed ? t("unfocus") : t("focus"), icon: zoomed ? Minimize2 : Maximize2, kbd: keys.maximize, divider: true, run: () => toggleMaximize(session.id) },
            { label: t("t6aEqualize"), icon: Equal, kbd: display("equalize_splits"), run: () => equalize(tabOf(session.id)) },
          ]),
      { label: t("close"), icon: X, kbd: display("close_split"), divider: true, run: () => closeSplit(session.id) },
    ];
    menuOpen = true;
  }

  /** From the launcher: another tool or folder beside this split. */
  function split(side: Side) {
    activate(session.id);
    openLauncher(side);
  }

  function cancel() {
    terminals.asking = null;
    focusSession(session.id);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions (the context-menu key and Shift+F10 of its terminal) -->
<section
  class="tile"
  class:waiting={session.running && session.status === "waiting"}
  class:ticked={ticked.has(session.id)}
  class:maximized={zoomed}
  class:dropping={dropping.on === session.id}
  data-session={session.id}
  hidden={!rect}
  style:left="calc({pct(rect?.x ?? 0)} + 4px)"
  style:top="calc({pct(rect?.y ?? 0)} + 4px)"
  style:width="calc({pct(rect?.w ?? 1)} - 8px)"
  style:height="calc({pct(rect?.h ?? 1)} - 8px)"
  aria-label={session.title}
  onfocusin={() => activate(session.id)}
  oncontextmenu={openMenu}
  onkeydowncapture={(e) => menuKey(e) && (e.stopPropagation(), openMenu(e))}
>
  <!-- svelte-ignore a11y_no_static_element_interactions (a shortcut for the zoom button) -->
  <header
    bind:this={head}
    class:remote={remote && phase(remote) !== "off"}
    ondblclick={(e) => !alone && !(e.target as Element).closest("button, input") && toggleMaximize(session.id)}
  >
    {#if grid.broadcast}
      <input
        type="checkbox"
        class="tick"
        checked={ticked.has(session.id)}
        disabled={!session.running}
        onchange={(e) => (e.currentTarget.checked ? ticked.add(session.id) : ticked.delete(session.id))}
        aria-label={t("broadcastTo")}
        title={t("broadcastTo")}
      />
    {/if}
    {#if !session.running}
      <StatusPill
        status={failed ? "failed" : "exited"}
        label={failed ? `${t("exitedLabel")} (${session.exitCode})` : undefined}
      />
    {:else if session.status}
      <StatusPill status={session.status} />
    {/if}
    {#if session.role}
      <span class="role"><UserRound size={12} aria-hidden="true" />{session.role}</span>
    {/if}
    <span class="title" title={session.cwd}
      ><b>{name}</b><span class="tool">{session.agent ?? t("shell")}</span>{#if session.action && session.running}<span class="last"
          ><Action act={{ kind: "tool", ...session.action }} /></span
        >{/if}</span
    >
    {#if session.context && session.running}<ContextGauge gauge={session.context} />{/if}
    {#if loops.length}<SessionLoops {session} {loops} />{/if}
    {#if remote && phase(remote) !== "off"}
      <RemoteSwitch id={session.id} rc={remote} />
    {:else if session.remoteControl && !session.remote}
      <span class="rc" role="img" aria-label={t("remoteControlOn")} title={t("remoteControlOn")}>
        <Smartphone size={14} aria-hidden="true" />
      </span>
    {/if}
    <!-- Over the end of the header, so a narrow tile keeps its name and gauge readable until hovered. -->
    <div class="actions">
      {#if remote && phase(remote) === "off"}
        <button class="icon-btn sm" onclick={remoteOn} aria-label={t("remoteTurnOn")} title="{t('remoteTurnOn')} · {t('uxfT3RemoteHint')}">
          <Smartphone size={14} aria-hidden="true" />
        </button>
      {/if}
      {#if session.context && session.running}
        <button
          class="icon-btn sm"
          onclick={() => resetContext(session.id)}
          disabled={session.context.phase !== "idle"}
          aria-label={t("contextReset")}
          title={t("contextReset")}
        >
          <RotateCcw size={14} aria-hidden="true" />
        </button>
      {/if}
      <button
        class="icon-btn sm"
        onclick={() => splitActive("right", session.id)}
        aria-label={t("t6aSplitRight")}
        title="{t('t6aSplitRight')} ({display('split_right')})"
      >
        <Columns2 size={14} aria-hidden="true" />
      </button>
      <button
        class="icon-btn sm"
        onclick={() => splitActive("down", session.id)}
        aria-label={t("t6aSplitDown")}
        title="{t('t6aSplitDown')} ({display('split_down')})"
      >
        <Rows2 size={14} aria-hidden="true" />
      </button>
      {#if !alone}
        <button
          class="icon-btn sm"
          onclick={() => toggleMaximize(session.id)}
          aria-label={zoomed ? t("unfocus") : t("focus")}
          title="{zoomed ? t('unfocus') : t('focus')} ({keys.maximize})"
        >
          {#if zoomed}<Minimize2 size={14} aria-hidden="true" />{:else}<Maximize2 size={14} aria-hidden="true" />{/if}
        </button>
      {/if}
      <button
        class="icon-btn sm"
        onclick={() => closeSplit(session.id)}
        aria-label={t("close")}
        title="{t('close')} ({display('close_split')})"
      >
        <X size={14} aria-hidden="true" />
      </button>
    </div>
  </header>
  {#if asking}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions (Escape answers the question) -->
    <div class="ask" role="alertdialog" aria-label={t("t6aCloseAsk")} tabindex="-1" onkeydown={(e) => e.key === "Escape" && cancel()}>
      <span>{t("t6aCloseAsk")}</span>
      <button class="btn sm" onclick={cancel}>{t("cancel")}</button>
      <button bind:this={confirm} class="btn sm primary" onclick={() => closeSplit(session.id, true)}>{t("close")}</button>
    </div>
  {:else if remote?.ask}
    <div class="ask" role="group" aria-label={t("remoteAsk")}>
      <Smartphone size={14} aria-hidden="true" />
      <span>{t("remoteAsk")}</span>
      <button class="btn sm" onclick={() => setRemote(session.id, false)}>{t("remoteNo")}</button>
      <button class="btn sm primary" onclick={remoteOn}>{t("remoteYes")}</button>
    </div>
  {/if}
  <Terminal id={session.id} visible={!!rect && onScreen("terminals")} />
  {#if dropping.on === session.id}<div class="drop-note" aria-hidden="true">{t("wsDropPath")}</div>{/if}
</section>

<ContextMenu bind:open={menuOpen} point={menuAt} trigger={menuTrigger} label={session.title} items={menuItems} />

<style>
  .tile {
    position: absolute;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--term-bg);
    box-shadow: var(--shadow-1);
    transition: box-shadow var(--dur) var(--ease-out);
  }

  .tile[hidden] {
    display: none;
  }

  .tile:focus-within {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }

  .tile.ticked {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  /* A file from the Finder over it: its path goes here once let go. */
  .tile.dropping {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent);
  }

  .drop-note {
    position: absolute;
    right: 12px;
    bottom: 12px;
    padding: 4px 10px;
    border-radius: var(--radius-pill);
    background: var(--accent);
    color: var(--accent-text);
    font-size: var(--text-sm);
    font-weight: 600;
    pointer-events: none;
  }

  .tile.waiting {
    border-color: var(--waiting-ring);
    box-shadow: 0 0 0 1px var(--waiting-ring);
  }

  header {
    --header-bg: var(--panel);
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 8px 0 10px;
    border-bottom: 1px solid var(--border);
    background: var(--header-bg);
    user-select: none;
  }

  .waiting header {
    --header-bg: var(--waiting-soft);
  }

  .title {
    display: flex;
    flex: 1;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }

  .title b,
  .tool {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* A narrow tile gives up the tool before the folder's name. */
  .title b {
    flex-shrink: 0;
    max-width: 100%;
    font-weight: 600;
  }

  .tool {
    color: var(--muted);
    font: var(--text-sm) var(--font-mono);
  }

  /* The agent's last action, the first to give way in a narrow tile. */
  .last {
    display: flex;
    flex-shrink: 20;
    min-width: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .last::before {
    content: "·";
    margin-right: 6px;
  }

  .role {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    gap: 4px;
    height: 20px;
    padding: 0 7px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .role :global(svg) {
    color: var(--accent-ink);
  }

  .rc {
    display: grid;
    place-items: center;
    color: var(--accent-ink);
  }

  .actions {
    position: absolute;
    inset: 0 6px 0 auto;
    display: flex;
    align-items: center;
    gap: 2px;
    padding-left: 20px;
    background: linear-gradient(to right, transparent, var(--header-bg) 16px);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--dur-fast) ease-out;
  }

  /* Shown on hover or keyboard focus; a maximized tile keeps them in line, as they carry its state. */
  .tile:hover .actions,
  .tile:focus-within .actions,
  .tile.maximized .actions {
    opacity: 1;
    pointer-events: auto;
  }

  .tile.maximized .actions {
    position: static;
    padding-left: 0;
    background: none;
  }

  /* Remote Control stays in sight and in reach while the actions show. */
  header.remote .actions {
    right: 36px;
  }

  .tick {
    flex-shrink: 0;
    margin: 0 2px 0 0;
  }

  .ask {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    color: var(--text);
    font-size: var(--text-sm);
  }

  .ask span {
    flex: 1;
    min-width: 12em;
  }
</style>
