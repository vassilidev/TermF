<script lang="ts">
  import { tick, untrack } from "svelte";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import AtSign from "@lucide/svelte/icons/at-sign";
  import FileText from "@lucide/svelte/icons/file-text";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import LayoutPanelLeft from "@lucide/svelte/icons/layout-panel-left";
  import CircleCheckBig from "@lucide/svelte/icons/circle-check-big";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MessagesSquare from "@lucide/svelte/icons/messages-square";
  import TextSelect from "@lucide/svelte/icons/text-select";
  import X from "@lucide/svelte/icons/x";
  import Conversation from "$lib/Conversation.svelte";
  import GitProposal from "$lib/GitProposal.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import SidePanel from "$lib/ui/SidePanel.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { showConversation } from "$lib/agents.svelte";
  import { ask, askedProject, chips, currentRole, loadRoles, outgoing, pick, removedChips, screenName, screenRole, send, threadOf, type Chip } from "$lib/ask.svelte";
  import { ensure } from "$lib/home.svelte";
  import { t } from "$lib/i18n";
  import { app, ui } from "$lib/state.svelte";
  import { tickets } from "$lib/tickets.svelte";
  import { files } from "$lib/files.svelte";

  /** The ⌘I panel: one way to ask an agent from any screen, docked on the right. */
  const project = $derived(askedProject());
  const role = $derived(currentRole());
  const session = $derived(threadOf(project, role));
  let conv = $state<ReturnType<typeof Conversation>>();
  const sent = $derived(chips());
  const placeholder = $derived(role === "cto" ? t("aigPlaceholderCto") : t("aigPlaceholder", { role }));

  $effect(() => {
    if (ask.open) loadRoles(project);
  });

  // The CTO's and the expert's conversations exist before the first question, as from the palette.
  $effect(() => {
    const p = project;
    const r = role;
    if (ask.open && p && (r === "cto" || r === "expert")) untrack(() => ensure(p, r));
  });

  let back: HTMLElement | null = null;
  $effect(() => {
    if (ask.open) back = untrack(() => (document.activeElement as HTMLElement | null) ?? null);
  });

  let draft = $state("");

  // Asked to focus, or a question to prefill: in the conversation's field once it is there, or the first question's.
  const standing = $derived(role === "cto" || role === "expert");
  $effect(() => {
    void ask.focus;
    void session?.id;
    untrack(async () => {
      if (standing && !session) return;
      if (!ask.open) return;
      await tick();
      const box = document.querySelector<HTMLTextAreaElement>("[data-ask] textarea");
      if (!box) return;
      if (ask.draft) {
        box.value = box.value.trim() ? `${box.value.trimEnd()}\n\n${ask.draft}` : ask.draft;
        // The binding takes it from the event, as if typed.
        box.dispatchEvent(new Event("input", { bubbles: true }));
        ask.draft = "";
      }
      box.focus();
    });
  });

  /** Escape closes; the focus goes back where it was, else (the "Ask" bar, gone while the panel was open) to the view's bar. */
  async function onkeydown(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented) return;
    e.preventDefault();
    ask.open = false;
    await tick();
    const view = document.querySelector<HTMLElement>(`[data-panel="${ui.view}"]`);
    const to = back?.isConnected ? back : (view?.querySelector<HTMLElement>(".askbar .ask") ?? view);
    to?.focus();
  }

  async function first(e: SubmitEvent) {
    e.preventDefault();
    if (ask.busy || !draft.trim()) return;
    if (await send(draft)) draft = "";
  }

  function firstKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      (e.currentTarget as HTMLTextAreaElement).form?.requestSubmit();
    }
  }

  // The roles one can ask: the screen's first, the CTO and the expert, then the project's.
  const roles = $derived([...new Set([screenRole(), "cto", "expert", ...ask.roles])]);

  // What "@ Add" offers: what was taken off, the open runs, the files open.
  let adding = $state(false);
  let addButton = $state<HTMLElement>();
  const addable = $derived.by((): Chip[] => {
    const all: Chip[] = [
      ...tickets.runs
        .filter((r) => r.project === project && r.status === "running")
        .slice(0, 5)
        .map((r) => ({ id: `run:${r.id}`, kind: "object" as const, label: `${r.ticket.key} ${r.title}`, text: `Ticket run ${r.id}: ${r.ticket.url}` })),
      ...files.tabs.slice(0, 5).map((tab) => ({ id: `file:${tab.repo}/${tab.path}`, kind: "object" as const, label: tab.path, text: `File ${tab.path} in ${tab.repo}` })),
    ];
    return all.filter((c) => !sent.some((s) => s.id === c.id) && !removed.some((r) => r.id === c.id));
  });
  const removed = $derived(removedChips());

  function add(chip: Chip) {
    const p = pick();
    if (p.removed.includes(chip.id)) p.removed = p.removed.filter((id) => id !== chip.id);
    else p.added.push(chip);
    adding = false;
  }

  const icons = { view: LayoutPanelLeft, object: FileText, selection: TextSelect };
</script>

{#snippet cleanupPlan(text: string)}<GitProposal {text} />{/snippet}

{#snippet chipRow()}
  <div class="ctx">
    <ul aria-label={t("aigContext")}>
      {#each sent as c (c.id)}
        {@const Icon = c.secret ? KeyRound : icons[c.kind]}
        <li class="chip" title={c.secret ? t("aigSecretChip") : (c.text?.slice(0, 300) ?? c.label)}>
          <Icon size={12} aria-hidden="true" />
          <span class="label">{c.label}</span>
          <button type="button" class="x" aria-label={t("aigRemoveChip", { label: c.label })} onclick={() => pick().removed.push(c.id)}>
            <X size={11} aria-hidden="true" />
          </button>
        </li>
      {/each}
    </ul>
    {#if addable.length || removed.length}
      <button type="button" class="btn sm quiet" bind:this={addButton} aria-expanded={adding} onclick={() => (adding = !adding)}>
        <AtSign size={12} aria-hidden="true" />{t("aigAdd")}
      </button>
      <Popover bind:open={adding} trigger={addButton} label={t("aigAdd")} width={280}>
        <ul class="menu">
          {#each [...removed, ...addable] as c (c.id)}
            <li><button type="button" onclick={() => add(c)}>{c.label}</button></li>
          {/each}
        </ul>
      </Popover>
    {/if}
  </div>
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions (Escape in the panel closes it) -->
<div class="dock" data-ask {onkeydown}>
  <SidePanel id="ask" title={t("aigTitle")} bind:open={ask.open} flush>
    {#snippet actions()}
      <label class="who">
        <Avatar {role} size="sm" />
        <span class="sr-only">{t("aigRole")}</span>
        <select class="role" value={role} onchange={(e) => (pick().role = e.currentTarget.value === screenRole() ? null : e.currentTarget.value)}>
          {#each roles as r (r)}
            <option value={r}>{r === screenRole() ? t("aigRoleHere", { role: r }) : r}</option>
          {/each}
        </select>
      </label>
      <!-- A run step's session is the team's, not this panel's: its topic is not the user's to close. -->
      {#if session && (standing || session.actions)}
        <button class="icon-btn" aria-label={t("topicDone")} title={t("topicDoneHelp")} onclick={() => conv?.endTopic()}>
          <CircleCheckBig size={16} aria-hidden="true" />
        </button>
      {/if}
      {#if session}
        <button class="icon-btn" aria-label={t("aigOpenConversation")} title={t("aigOpenConversation")} onclick={() => showConversation(session.id)}>
          <MessagesSquare size={16} aria-hidden="true" />
        </button>
      {/if}
    {/snippet}

    {#if ask.error}
      <div class="banner"><Banner kind="error" ondismiss={() => (ask.error = "")}>{ask.error}</Banner></div>
    {/if}

    {#if project === null}
      <div class="pad">
        <EmptyState icon={MessagesSquare} title={t("aigNoProjectTitle")} text={t("aigNoProjectBody")} />
      </div>
    {:else if session}
      {#key session.id}
        <Conversation bind:this={conv} {session} bare fresh={standing} chips={chipRow} {outgoing} {placeholder} fences={{ "termf-cleanup": cleanupPlan }} />
      {/key}
    {:else if standing}
      <!-- Its conversation is being opened. -->
      <div class="pad"><Skeleton rows={4} /></div>
    {:else}
      <div class="intro">
        <Avatar {role} size="lg" />
        <p><b>{t("aigIntroTitle", { role })}</b></p>
        <p class="muted">{t("aigIntroBody", { screen: screenName(ui.view) })}</p>
      </div>
      <form class="first" onsubmit={first}>
        {@render chipRow()}
        <div class="box">
          <textarea
            class="field"
            rows="2"
            {placeholder}
            aria-label={placeholder}
            bind:value={draft}
            onkeydown={firstKey}
          ></textarea>
          <button type="submit" class="icon-send" aria-label={t("chatSend")} title={t("chatSend")} disabled={ask.busy || !draft.trim()}>
            {#if ask.busy}<LoaderCircle size={16} aria-hidden="true" class="spin" />{:else}<ArrowUp size={16} aria-hidden="true" />{/if}
          </button>
        </div>
      </form>
    {/if}
  </SidePanel>
</div>

<style>
  .dock {
    display: contents;
  }

  .who {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .role {
    max-width: 150px;
    height: var(--h-sm);
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--panel);
    color: var(--text);
    font: inherit;
    font-size: var(--text-sm);
  }

  .role:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }

  .banner,
  .pad {
    padding: 12px;
  }

  .intro {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 24px;
    text-align: center;
  }

  .intro p {
    margin: 0;
  }

  .intro :global(.avatar) {
    margin-bottom: 8px;
  }

  .muted {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .first {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px 12px;
    border-top: 1px solid var(--border);
  }

  .box {
    position: relative;
  }

  .box textarea {
    width: 100%;
    min-height: 58px;
    padding: 8px 48px 8px 12px;
    border-radius: var(--radius-lg);
    resize: none;
  }

  .icon-send {
    position: absolute;
    right: 8px;
    bottom: 8px;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--accent-text);
  }

  .icon-send:disabled {
    background: var(--hover);
    color: var(--muted);
  }

  .ctx {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .ctx ul {
    display: contents;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    height: 24px;
    padding: 0 4px 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--well);
    color: var(--soft);
    font-size: var(--text-xs);
  }

  .chip .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .x {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
  }

  .x:hover {
    background: var(--hover);
    color: var(--text);
  }

  .x:focus-visible {
    outline: 2px solid var(--focus);
  }

  .menu {
    margin: 0;
    padding: 4px;
    list-style: none;
  }

  .menu button {
    width: 100%;
    padding: 6px 8px;
    overflow: hidden;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .menu button:hover,
  .menu button:focus-visible {
    background: var(--hover);
  }
</style>
