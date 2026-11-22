<script lang="ts">
  import { onMount } from "svelte";
  import Plus from "@lucide/svelte/icons/plus";
  import Webhook from "@lucide/svelte/icons/webhook";
  import X from "@lucide/svelte/icons/x";
  import ClaudeChange from "$lib/ClaudeChange.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import { t } from "$lib/i18n";
  import { change, claudeView, discard, propose, scopeLabel, settingsSchema, type Overview, type Scope } from "$lib/claude.svelte";

  let { o }: { o: Overview } = $props();

  let adding = $state(false);
  let scope = $state<Scope>("user");
  let event = $state("PostToolUse");
  let matcher = $state("");
  let command = $state("");
  let events = $state<string[]>([]);

  onMount(() => {
    settingsSchema()
      .then((s) => {
        const hooks = (s.properties as Record<string, { properties?: Record<string, unknown> }>)?.hooks;
        events = Object.keys(hooks?.properties ?? {});
      })
      .catch((e) => (claudeView.error = String(e)));
  });

  async function add(e: SubmitEvent) {
    e.preventDefault();
    await propose("hooks", scope, { op: "addHook", event: event.trim(), matcher: matcher.trim() || null, command });
  }

  function close() {
    adding = false;
    discard();
    command = "";
    matcher = "";
  }

  // The form closes once the hooks are read again after a save.
  let seen: unknown = null;
  $effect(() => {
    const hooks = o.hooks;
    if (seen !== null && hooks !== seen && adding) close();
    seen = hooks;
  });
</script>

{#if o.hooks.length}
  <ul class="rows">
    {#each o.hooks as h (`${h.scope}:${h.event}:${h.group}:${h.index}`)}
      <li>
        <span class="what">
          <span class="name">{h.event}{#if h.matcher}<span class="matcher mono">{h.matcher}</span>{/if}</span>
          <span class="mono desc">{h.kind === "command" ? h.run : `${h.kind}: ${h.run}`}</span>
        </span>
        <span class="badge scope">{scopeLabel(h.scope)}</span>
        {#if h.scope !== "managed"}
          <button
            class="icon-btn sm"
            title={t("claudeMgmtRemoveItem", { item: h.event })}
            aria-label={t("claudeMgmtRemoveItem", { item: `${h.event} · ${h.run}` })}
            onclick={() => propose(`hook:${h.scope}:${h.event}:${h.group}:${h.index}`, h.scope, { op: "dropHook", event: h.event, group: h.group, index: h.index, run: h.run })}
            ><X size={13} aria-hidden="true" /></button
          >
        {:else}
          <span class="icon-space"></span>
        {/if}
      </li>
      {#if change.origin === `hook:${h.scope}:${h.event}:${h.group}:${h.index}`}<li class="pending"><ClaudeChange /></li>{/if}
    {/each}
  </ul>
{:else}
  <EmptyState icon={Webhook} text={t("claudeViewNone")} />
{/if}

{#if adding}
  <form class="form" onsubmit={add} oninput={() => change.origin === "hooks" && discard()}>
    <h3>{t("claudeMgmtAddHook")}</h3>
    <Banner kind="warn">{t("claudeMgmtHookWarning")}</Banner>
    <div class="grid">
      <label class="labelled">
        <span>{t("claudeViewScope")}</span>
        <select class="field" bind:value={scope}>
          <option value="user">{scopeLabel("user")}</option>
          <option value="project" disabled={!claudeView.repo}>{t("claudeMgmtScopeShared")}</option>
          <option value="local" disabled={!claudeView.repo}>{scopeLabel("local")}</option>
        </select>
      </label>
      <label class="labelled">
        <span>{t("claudeViewEvent")}</span>
        <input class="field mono" bind:value={event} list="claude-hook-events" spellcheck="false" autocomplete="off" required />
        <datalist id="claude-hook-events">{#each events as ev (ev)}<option value={ev}></option>{/each}</datalist>
      </label>
      <label class="labelled">
        <span>{t("claudeMgmtMatcher")}</span>
        <input class="field mono" bind:value={matcher} placeholder="Edit|Write" spellcheck="false" autocomplete="off" />
      </label>
    </div>
    <label class="labelled">
      <span>{t("claudeMgmtCommand")}</span>
      <input class="field mono" bind:value={command} placeholder="npx prettier --write" spellcheck="false" autocomplete="off" required />
    </label>
    <p class="hint">{t("claudeMgmtMatcherHint")}</p>
    {#if change.origin === "hooks"}
      <ClaudeChange oncancel={close} />
    {:else}
      <div class="buttons">
        <button type="button" class="btn sm" onclick={close}>{t("cancel")}</button>
        <button class="btn sm primary" disabled={!command.trim() || !event.trim()}>{t("claudeViewPreview")}</button>
      </div>
    {/if}
  </form>
{:else}
  <button class="btn sm add" onclick={() => (adding = true)}><Plus size={13} aria-hidden="true" />{t("claudeMgmtAddHook")}</button>
{/if}

<style>
  .rows {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    list-style: none;
  }

  .rows li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 6px 8px 6px 12px;
  }

  .rows li + li {
    border-top: 1px solid var(--border);
  }

  .rows li.pending {
    display: block;
    padding: 0 12px;
  }

  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    font-weight: 500;
  }

  .matcher {
    margin-left: 8px;
    color: var(--muted);
    font-weight: 400;
  }

  .desc {
    overflow: hidden;
    color: var(--muted);
    line-height: 1.5;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge.scope {
    min-width: 64px;
    justify-content: center;
  }

  .icon-space {
    width: 22px;
    flex-shrink: 0;
  }

  .add {
    margin-top: 12px;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 16px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--chrome);
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .grid {
    display: grid;
    grid-template-columns: 180px 220px minmax(0, 1fr);
    gap: 10px 14px;
  }

  .hint {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
