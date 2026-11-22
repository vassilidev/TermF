<script lang="ts">
  import { tick } from "svelte";
  import Info from "@lucide/svelte/icons/info";
  import Search from "@lucide/svelte/icons/search";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import { isMac } from "$lib/files.svelte";
  import { t } from "$lib/i18n";
  import {
    action,
    actions,
    conflict,
    display,
    format,
    groups,
    parse,
    pressedCombo,
    refusal,
    setBinding,
    shellKeys,
    tokens,
    type KeyAction,
    type Combo,
  } from "$lib/keymap-logic";
  import { current, keymap, placeLabel, saveKeys } from "$lib/keymap.svelte";

  const DIGITS = "1…9";

  /** The search of the Settings view. */
  let { filter = "" }: { filter?: string } = $props();

  let query = $state("");
  let confirming = $state(false);
  let error = $state("");
  /** A combination another action already has, waiting for Replace or Cancel. */
  let clash = $state<{ id: string; combo: string; other: string } | null>(null);
  /** Why the last press was refused, or why saving failed, under the action's row. */
  let notes = $state<Record<string, string>>({});

  const bound = $derived(current());
  const fallback = (a: KeyAction) => (isMac ? a.defaults.mac : a.defaults.other);
  const changed = (a: KeyAction) => a.id in keymap.overrides && keymap.overrides[a.id] !== fallback(a);
  // A system-wide shortcut is a key place, named after what it types on this keyboard when the webview says.
  const named = (a: KeyAction, c: Combo | null) => (c && a.scope === "global" ? { ...c, key: placeLabel(c.key) } : c);
  const shown = (a: KeyAction, combo: string) => display(named(a, parse(combo)), isMac, a.digits ? DIGITS : "");
  const fold = (s: string) =>
    s
      .normalize("NFD")
      .replace(/\p{Diacritic}/gu, "")
      .toLowerCase();

  function matching(list: KeyAction[], search: string): KeyAction[] {
    const words = fold(search).split(/\s+/).filter(Boolean);
    return list.filter((a) => {
      const text = fold([t(a.label), a.help ? t(a.help) : "", shown(a, bound[a.id]), bound[a.id]].join(" "));
      return words.every((w) => text.includes(w));
    });
  }

  // The settings' search narrows the list when it names actions, not when it only named the section.
  const visible = $derived.by(() => {
    const outer = matching(actions, filter);
    return matching(outer.length ? outer : actions, query);
  });

  function keysOf(a: KeyAction): string[] {
    const c = named(a, parse(clash?.id === a.id ? clash.combo : bound[a.id]));
    return c ? tokens(c, isMac, a.digits ? DIGITS : "") : [];
  }

  /** Linux and Windows: the reminder that, in a terminal, this combination needs Shift. */
  function shellNote(a: KeyAction): string {
    const c = parse(bound[a.id]);
    if (!c || a.scope === "files" || a.scope === "global" || !shellKeys(a.digits ? { ...c, key: "1" } : c, isMac)) {
      return "";
    }
    const digit = a.digits ? DIGITS : "";
    return t("settingsKeysShell", {
      keys: display(c, isMac, digit),
      shifted: display({ ...c, shift: true }, isMac, digit),
    });
  }

  function note(a: KeyAction): string {
    if (notes[a.id]) return notes[a.id];
    return a.scope === "global" && keymap.globalError ? t("settingsKeysGlobalError", { error: keymap.globalError }) : "";
  }

  // WebKit does not focus a clicked button: the focus is given here, and the keys are read on the window.
  function record(a: KeyAction, button: HTMLElement) {
    clash = null;
    delete notes[a.id];
    keymap.recording = a.id;
    button.focus();
  }

  // Leaving the section must not leave every shortcut off.
  $effect(() => () => {
    keymap.recording = null;
  });

  function stop() {
    if (keymap.recording) delete notes[keymap.recording];
    keymap.recording = null;
  }

  async function refocus(id: string) {
    await tick();
    document.getElementById(`key-${id}`)?.focus();
  }

  /** In the capture phase, before any field or terminal; the app's shortcuts are off meanwhile. */
  function capture(e: KeyboardEvent) {
    const a = keymap.recording ? action(keymap.recording) : null;
    if (!a) return;
    e.preventDefault();
    e.stopPropagation();
    const plain = !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey;
    if (plain && e.key === "Escape") return stop();
    if (plain && e.key === "Backspace") return (stop(), choose(a, ""));
    const c = pressedCombo(e, a.scope === "global");
    if (!c) return;
    const why = refusal(a, c, isMac);
    if (why) {
      // Still recording: the next press may do.
      notes[a.id] = t(why, { keys: display(named(a, c), isMac), mods: isMac ? "⌘ / ⌃ / ⌥" : "Ctrl / Alt" });
      return;
    }
    stop();
    choose(a, format(a.digits ? { ...c, key: "" } : c));
  }

  function choose(a: KeyAction, combo: string) {
    const other = combo ? conflict(a.id, combo, bound, isMac) : null;
    if (other) clash = { id: a.id, combo, other };
    else save(setBinding(keymap.overrides, a.id, combo, isMac), a.id);
  }

  function replace() {
    if (!clash) return;
    const { id, combo, other } = clash;
    clash = null;
    save(setBinding(setBinding(keymap.overrides, other, "", isMac), id, combo, isMac), id);
    refocus(id);
  }

  function cancelClash() {
    const id = clash?.id;
    clash = null;
    if (id) refocus(id);
  }

  function resetAll() {
    confirming = false;
    clash = null;
    const known = new Set(actions.map((a) => a.id));
    save(Object.fromEntries(Object.entries(keymap.overrides).filter(([id]) => !known.has(id))), null);
  }

  async function save(next: Record<string, string>, id: string | null) {
    error = "";
    try {
      await saveKeys(next);
      if (id) delete notes[id];
    } catch (e) {
      const message = String(e);
      if (!id) error = message;
      else notes[id] = action(id)?.scope === "global" ? t("settingsKeysGlobalError", { error: message }) : message;
    }
  }
</script>

<svelte:window onkeydowncapture={capture} />

<section class="keys" aria-labelledby="keys-title">
  <h2 id="keys-title">{t("shortcuts")}</h2>
  <p class="lead">{t("settingsKeysHelp")}</p>

  <div class="bar">
    <div class="search">
      <Search size={14} aria-hidden="true" />
      <input
        class="field"
        type="search"
        placeholder={t("settingsKeysSearch")}
        aria-label={t("settingsKeysSearch")}
        bind:value={query}
      />
    </div>
    {#if confirming}
      <span class="ask" role="alert">{t("settingsKeysResetAllAsk")}</span>
      <button class="btn sm" onclick={() => (confirming = false)} {@attach (el) => el.focus()}>{t("cancel")}</button>
      <button class="btn sm primary" onclick={resetAll}>{t("settingsKeysResetAll")}</button>
    {:else}
      <button class="btn sm" onclick={() => (confirming = true)} disabled={!actions.some(changed)}>
        <Undo2 size={14} aria-hidden="true" />{t("settingsKeysResetAll")}
      </button>
    {/if}
  </div>
  {#if error}
    <p class="notice error" role="alert">{error}</p>
  {/if}

  {#each groups as [group, title] (group)}
    {@const rows = visible.filter((a) => a.group === group)}
    {#if rows.length}
      <h3>{t(title)}</h3>
      <ul>
        {#each rows as a (a.id)}
          {@const recording = keymap.recording === a.id}
          {@const reminder = shellNote(a)}
          {@const line = note(a)}
          {@const keys = keysOf(a)}
          <li class:conflict={clash?.id === a.id} class:recording>
            <span class="what" id="key-label-{a.id}">
              {t(a.label)}
              {#if a.help}<small>{t(a.help)}</small>{/if}
              {#if reminder}<small>{reminder}</small>{/if}
            </span>
            <button
              id="key-{a.id}"
              class="combo"
              class:rec={recording}
              aria-describedby="key-label-{a.id}"
              onclick={(e) => (recording ? stop() : record(a, e.currentTarget))}
              onblur={() => recording && stop()}
            >
              {#if recording}
                {t("settingsKeysRecording")}
              {:else if keys.length}
                {#each keys as key, i (i)}<kbd>{key}</kbd>{/each}
              {:else}
                <span class="none">{t("settingsKeysNone")}</span>
              {/if}
              {#if changed(a) && !recording}<span class="dot" aria-hidden="true"></span>{/if}
            </button>
            <span class="acts">
              {#if recording}
                <!-- Keeps the focus on the combination, whose blur would also cancel. -->
                <button class="btn sm quiet" onpointerdown={(e) => e.preventDefault()} onclick={stop}>
                  {t("cancel")}<kbd>{t("settingsKeysEsc")}</kbd>
                </button>
              {:else if changed(a)}
                {@const label = t("settingsKeysRevert", {
                  keys: shown(a, fallback(a)) || t("settingsKeysNoShortcut"),
                })}
                <button class="icon-btn sm" title={label} aria-label={label} onclick={() => choose(a, fallback(a))}>
                  <Undo2 size={14} aria-hidden="true" />
                </button>
              {/if}
            </span>
            {#if clash?.id === a.id}
              {@const other = action(clash.other)}
              <div class="clash" role="alert">
                <Info size={14} aria-hidden="true" />
                <span class="text">
                  {t("settingsKeysConflict", {
                    keys: shown(a, clash.combo),
                    action: other ? t(other.label) : clash.other,
                  })}
                </span>
                <button class="btn sm" onclick={cancelClash}>{t("cancel")}</button>
                <button class="btn sm primary" onclick={replace}>{t("replace")}</button>
              </div>
            {:else if line}
              <p class="note" role="alert">{line}</p>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/each}
  {#if !visible.length}
    <p class="none-found muted">{t("filesNoResults")}</p>
  {/if}
</section>

<style>
  .keys {
    max-width: 760px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .lead {
    margin: 6px 0 16px;
    color: var(--muted);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .search {
    position: relative;
    flex: 1;
    min-width: 0;
    color: var(--muted);
  }

  .search :global(svg) {
    position: absolute;
    top: 8px;
    left: 10px;
    pointer-events: none;
  }

  .search .field {
    padding-left: 30px;
  }

  .ask {
    color: var(--text);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .bar + .notice {
    margin-top: 8px;
  }

  h3 {
    margin: 20px 0 8px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  ul {
    margin: 0;
    padding: 0;
    overflow: hidden;
    list-style: none;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 200px 96px;
    align-items: center;
    gap: 6px 12px;
    min-height: 42px;
    padding: 6px 10px 6px 14px;
    border-top: 1px solid var(--border);
  }

  li:first-child {
    border-top: 0;
  }

  li.conflict {
    background: var(--waiting-soft);
  }

  li.recording {
    background: var(--accent-soft);
  }

  .what {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .what small {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .combo {
    display: inline-flex;
    align-items: center;
    justify-self: start;
    gap: 3px;
    min-height: 28px;
    padding: 0 6px;
    border: 1px solid transparent;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    transition: background-color var(--dur-fast) ease-out;
  }

  .combo:hover {
    background: var(--hover);
  }

  .combo kbd {
    min-width: 22px;
    font-size: var(--text-sm);
    line-height: 20px;
  }

  .combo.rec {
    border-color: var(--accent);
    background: var(--panel);
    color: var(--accent-ink);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .none {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .dot {
    width: 6px;
    height: 6px;
    margin-left: 4px;
    border-radius: 50%;
    background: var(--accent);
  }

  .acts {
    display: flex;
    justify-content: flex-end;
  }

  .acts kbd {
    margin-left: 4px;
  }

  .clash,
  .note {
    grid-column: 1 / -1;
  }

  .clash {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0 2px;
    color: var(--text);
    font-size: var(--text-sm);
  }

  .clash :global(svg) {
    flex-shrink: 0;
    color: var(--warning);
  }

  .clash .text {
    flex: 1;
    min-width: 0;
  }

  .note {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }

  .none-found {
    margin-top: 16px;
  }
</style>
