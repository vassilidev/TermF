<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pin from "@lucide/svelte/icons/pin";
  import Plus from "@lucide/svelte/icons/plus";
  import UserRound from "@lucide/svelte/icons/user-round";
  import Conversation from "$lib/Conversation.svelte";
  import Square from "@lucide/svelte/icons/square";
  import Banner from "$lib/ui/Banner.svelte";
  import SelectionBar from "$lib/ui/SelectionBar.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import {
    agents,
    conversation,
    ctoOf,
    expertOf,
    listed,
    runtimes,
    showCto,
    showExpert,
    upsert,
    usd,
    type AgentSession,
  } from "$lib/agents.svelte";
  import { currentProject, isMac, scopeRepos } from "$lib/files.svelte";
  import { all, none, pick, settle, within, type Outcome } from "$lib/select-logic";
  import { pinned, pinnedFirst } from "$lib/conversation-logic";
  import { ago, t } from "$lib/i18n";
  import type { Item } from "$lib/library.svelte";
  import { mostRecent } from "$lib/nav-logic";
  import { app, shortPath } from "$lib/state.svelte";

  // Pinned for display only: without a pick, the view and ⌘J still start from the most recent.
  const sessions = $derived(pinnedFirst(listed()));
  const current = $derived(conversation());
  const creating = $derived(agents.creating || !current);
  const repos = $derived([...scopeRepos()].sort((a, b) => a.name.localeCompare(b.name)));

  const efforts = [
    ["low", "schedulerEffortLow"],
    ["medium", "schedulerEffortMedium"],
    ["high", "schedulerEffortHigh"],
    ["xhigh", "schedulerEffortXhigh"],
    ["max", "schedulerEffortMax"],
  ] as const;
  const modes = [
    ["manual", "schedulerPermManual"],
    ["acceptEdits", "schedulerPermAcceptEdits"],
    ["plan", "schedulerPermPlan"],
    ["auto", "schedulerPermAuto"],
    ["dontAsk", "schedulerPermDontAsk"],
    ["bypassPermissions", "schedulerPermBypass"],
  ] as const;

  let dir = $state("");
  let role = $state("");
  let prompt = $state("");
  let options = $state({ agent: "", model: "", effort: "", permissionMode: "", budget: null as number | null });
  let roles = $state<Item[]>([]);
  let starting = $state(false);
  let error = $state("");

  // The conversation shown stays: a newer session (a team step, the CLI) must not take its place.
  $effect(() => {
    if (current && agents.selected !== current.id) agents.selected = current.id;
  });

  // The folder follows the current project: its most recent repository until one is picked.
  $effect(() => {
    if (!repos.some((r) => r.path === dir)) dir = mostRecent(repos)?.path ?? "";
  });

  // The current project's active roles; a stale answer is dropped.
  $effect(() => {
    const project = app.project;
    if (!creating) return;
    invoke<Item[]>("library_list", { kind: "role", project }).then(
      (items) => {
        if (project !== app.project) return;
        roles = items.filter((i) => i.active);
        if (!roles.some((r) => r.name === role)) role = "";
      },
      () => {
        if (project !== app.project) return;
        roles = [];
        role = "";
      },
    );
  });

  let picked = $state(none<number>());
  let asking = $state(false);
  /** Kept apart from `error`, which belongs to the new session form. */
  let stopError = $state("");
  const ids = $derived(sessions.map((s) => s.id));
  const chosen = $derived(sessions.filter((s) => within(picked.keys, ids).includes(s.id)));
  const stoppable = $derived(chosen.filter((s) => s.state === "working" || s.state === "waiting" || s.state === "idle"));

  function pickOne(e: MouseEvent, id: number) {
    e.preventDefault();
    asking = false;
    stopError = "";
    picked = pick(picked, ids, id, e.shiftKey);
  }

  /** Each one as its own Stop button would; one that fails stays picked. */
  async function stopPicked() {
    asking = false;
    const results: [number, Outcome][] = [];
    for (const s of stoppable) {
      try {
        upsert(await invoke<AgentSession>("agent_stop", { id: s.id }));
        results.push([s.id, "done"]);
      } catch (e) {
        results.push([s.id, { error: String(e) }]);
      }
    }
    const out = settle(results);
    stopError = out.errors.join(" · ");
    picked = { keys: out.failed, anchor: null };
  }

  function select(id: number) {
    agents.selected = id;
    agents.creating = false;
  }

  async function start(e: SubmitEvent) {
    e.preventDefault();
    starting = true;
    error = "";
    try {
      const session = await invoke<AgentSession>("agent_start", {
        spec: {
          cwd: dir,
          project: app.project ?? undefined,
          role: role || undefined,
          agent: options.agent || undefined,
          model: options.model.trim() || undefined,
          effort: options.effort || undefined,
          permissionMode: options.permissionMode || undefined,
          budget: options.budget || undefined,
        },
        prompt,
      });
      upsert(session);
      select(session.id);
      prompt = "";
    } catch (e) {
      error = String(e);
    } finally {
      starting = false;
    }
  }
</script>

<div class="conversations">
  <ViewHeader title={t("chatView")} context={currentProject()?.name ?? t("ticketsAll")}>
    {#snippet actions()}
      {#if !creating}
        <button class="btn sm primary" onclick={() => (agents.creating = true)}>
          <Plus size={14} aria-hidden="true" />{t("chatNew")}
        </button>
      {/if}
    {/snippet}
  </ViewHeader>

  {#if chosen.length}
    <SelectionBar
      count={chosen.length}
      total={sessions.length}
      label={t("chatList")}
      onall={() => ((asking = false), (stopError = ""), (picked = all(picked, ids)))}
      onclear={() => ((asking = false), (stopError = ""), (picked = none()))}
    >
      {#if asking}
        <span class="ask" role="alert">{t("t6bStopAsk", { count: stoppable.length })}</span>
        <button class="btn sm" onclick={() => (asking = false)}>{t("cancel")}</button>
        <button class="btn sm danger" onclick={stopPicked}>{t("chatStop")} · {stoppable.length}</button>
      {:else}
        <button class="btn sm" disabled={!stoppable.length} onclick={() => (asking = true)}>
          <Square size={13} aria-hidden="true" />{t("chatStop")} · {stoppable.length}
        </button>
      {/if}
    </SelectionBar>
  {/if}
  {#if stopError}
    <div class="banners">
      <Banner kind="error" ondismiss={() => (stopError = "")}>{stopError}</Banner>
    </div>
  {/if}
  <div class="split">
    <ul class="list" class:picking={chosen.length > 0} aria-label={t("chatList")}>
      {#if app.project !== null && !ctoOf(app.project)}
        <li class="row">
          <span class="slot" aria-hidden="true"></span>
          <button class="pick" onclick={showCto}>
            <span class="name">
              <span class="title">{t("ctoTitle", { project: currentProject()?.name ?? app.project })}</span>
              <Pin size={12} class="pin" aria-label={t("ctoPinned")} />
            </span>
            <span class="desc">{t("ctoStart")}</span>
          </button>
        </li>
      {/if}
      {#if app.project !== null && !expertOf(app.project)}
        <li class="row">
          <span class="slot" aria-hidden="true"></span>
          <button class="pick" onclick={showExpert}>
            <span class="name">
              <span class="title">{t("expertTitle", { project: currentProject()?.name ?? app.project })}</span>
              <Pin size={12} class="pin" aria-label={t("ctoPinned")} />
            </span>
            <span class="desc">{t("expertStart")}</span>
          </button>
        </li>
      {/if}
      {#each sessions as session (session.id)}
        <li class="row" class:current={session === current && !creating} class:picked={chosen.includes(session)}>
          <input
            class="box"
            type="checkbox"
            checked={chosen.includes(session)}
            aria-label={t("t6bPick", { name: session.title })}
            onclick={(e) => pickOne(e, session.id)}
          />
          <button
            class="pick"
            aria-current={session === current && !creating}
            onclick={(e) => ((isMac ? e.metaKey : e.ctrlKey) || e.shiftKey ? pickOne(e, session.id) : select(session.id))}
          >
            <span class="name">
              <span class="title">{session.title}</span>
              {#if pinned(session)}<Pin size={12} class="pin" aria-label={t("ctoPinned")} />{/if}
              {#if session.role && !pinned(session)}<span class="role"><UserRound size={11} aria-hidden="true" />{session.role}</span>{/if}
            </span>
            <span class="desc"
              >{runtimes[session.runtime] ? t(runtimes[session.runtime]) : usd(session.cost)} · {ago(session.updated)}</span
            >
          </button>
          <StatusPill status={session.state} />
        </li>
      {:else}
        <li class="none muted">{t("chatEmpty")}</li>
      {/each}
    </ul>

    <section class="detail" class:talk={!creating}>
      {#if creating}
        <form class="form" onsubmit={start}>
          <h2>{t("chatNew")}</h2>
          <p class="help">{t("chatNewHelp")}</p>
          <div class="grid">
            <label class="labelled">
              <span>{t("chatFolder")}</span>
              <select class="field" required bind:value={dir}>
                <option value="" disabled>{t("schedulerDirPick")}</option>
                {#each repos as repo (repo.path)}
                  <option value={repo.path}>{repo.name} · {shortPath(repo.path)}</option>
                {/each}
              </select>
            </label>
            {#if roles.length}
              <label class="labelled">
                <span>{t("kindRole")}</span>
                <select class="field" bind:value={role}>
                  <option value="">{t("chatNoRole")}</option>
                  {#each roles as r (r.name)}<option value={r.name}>{r.name}</option>{/each}
                </select>
              </label>
            {/if}
          </div>
          <label class="labelled">
            <span>{t("chatFirstMessage")}</span>
            <textarea class="field prompt" required rows="5" placeholder={t("chatFirstMessageHint")} bind:value={prompt}></textarea>
          </label>
          <details class="options">
            <summary>{t("chatOptions")}</summary>
            <div class="grid">
              <label class="labelled">
                <span>{t("agent")}</span>
                <select class="field" bind:value={options.agent}>
                  <option value="">{t("schedulerDefault")}</option>
                  {#each app.workspace?.agents ?? [] as agent (agent)}<option value={agent}>{agent}</option>{/each}
                </select>
              </label>
              <label class="labelled">
                <span>{t("metaModel")}</span>
                <input
                  class="field"
                  list="chat-models"
                  placeholder={t("schedulerDefault")}
                  spellcheck="false"
                  autocomplete="off"
                  bind:value={options.model}
                />
                <datalist id="chat-models">
                  <option value="opus"></option>
                  <option value="sonnet"></option>
                  <option value="haiku"></option>
                </datalist>
              </label>
              <label class="labelled">
                <span>{t("metaEffort")}</span>
                <select class="field" bind:value={options.effort}>
                  <option value="">{t("schedulerDefault")}</option>
                  {#each efforts as [value, label] (value)}<option {value}>{t(label)}</option>{/each}
                </select>
              </label>
              <label class="labelled">
                <span>{t("chatMode")}</span>
                <select class="field" bind:value={options.permissionMode}>
                  <option value="">{t("schedulerDefault")}</option>
                  {#each modes as [value, label] (value)}<option {value}>{t(label)}</option>{/each}
                </select>
              </label>
              <label class="labelled">
                <span>{t("schedulerBudget")}</span>
                <input
                  class="field"
                  type="number"
                  min="0.01"
                  step="0.01"
                  inputmode="decimal"
                  placeholder="—"
                  bind:value={() => options.budget ?? "", (v) => (options.budget = v === "" || v === null ? null : Number(v))}
                />
              </label>
            </div>
          </details>
          {#if error}
            <p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>
          {/if}
          <div class="buttons">
            {#if current}
              <button type="button" class="btn sm" onclick={() => (agents.creating = false)}>{t("cancel")}</button>
            {/if}
            <button type="submit" class="btn sm primary" disabled={starting || !dir || !prompt.trim()}>
              {#if starting}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
              {t("chatStart")}
            </button>
          </div>
        </form>
      {:else if current}
        {#key current.id}
          <Conversation session={current} />
        {/key}
      {/if}
    </section>
  </div>
</div>

<style>
  .conversations {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .name {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .role {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    gap: 3px;
    height: 18px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .role :global(svg) {
    color: var(--accent-ink);
  }

  .name :global(.pin) {
    flex-shrink: 0;
    color: var(--accent-ink);
  }

  /* Where the other rows have their checkbox. */
  .slot {
    flex-shrink: 0;
    width: 14px;
  }

  .none {
    padding: 8px 10px;
    font-size: var(--text-sm);
  }

  /* A conversation scrolls its thread between a fixed header and the message box. */
  .detail.talk {
    display: flex;
    flex-direction: column;
    padding: 0;
    overflow: hidden;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .help {
    margin: -6px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 10px 12px;
  }

  .labelled {
    min-width: 0;
  }

  .prompt {
    height: auto;
    min-height: 96px;
    padding: 6px 8px;
    line-height: 1.5;
    resize: vertical;
  }

  .options summary {
    width: max-content;
    margin-bottom: 10px;
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
