<script lang="ts">
  import { onMount } from "svelte";
  import ArrowRightLeft from "@lucide/svelte/icons/arrow-right-left";
  import Eye from "@lucide/svelte/icons/eye";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import ClaudeChange from "$lib/ClaudeChange.svelte";
  import { t } from "$lib/i18n";
  import { field } from "$lib/claude-logic";
  import {
    change,
    claudeView,
    effective,
    fileValue,
    propose,
    reveal,
    scopeLabel,
    settingsSchema,
    show,
    type Edit,
    type Overview,
    type Scope,
  } from "$lib/claude.svelte";

  type Key = Parameters<typeof t>[0];

  let { o }: { o: Overview } = $props();

  /** The file edits go to; managed settings are only read. */
  let scope = $state<Scope>("user");
  const writable: { id: Scope; label: Key; help: Key }[] = [
    { id: "user", label: "claudeViewScopeUser", help: "claudeMgmtScopeUserHelp" },
    { id: "project", label: "claudeMgmtScopeShared", help: "claudeMgmtScopeSharedHelp" },
    { id: "local", label: "claudeViewScopeLocal", help: "claudeMgmtScopeLocalHelp" },
  ];
  // Strongest first: the order Claude Code reads them in.
  const order: Scope[] = ["managed", "local", "project", "user"];
  const file = $derived(o.settings.files.find((f) => f.scope === scope));

  $effect(() => {
    if (!claudeView.repo && scope !== "user") scope = "user";
  });

  let schema = $state<Record<string, unknown> | null>(null);
  onMount(() => {
    settingsSchema()
      .then((s) => (schema = s))
      .catch((e) => (claudeView.error = String(e)));
  });
  const efforts = $derived(schema ? field(schema, "effortLevel").values : []);
  const modes = $derived(schema ? field(schema, "permissions.defaultMode").values : []);

  const lists: { id: "deny" | "ask" | "allow"; label: Key }[] = [
    { id: "deny", label: "claudeMgmtDeny" },
    { id: "ask", label: "claudeMgmtAsk" },
    { id: "allow", label: "claudeMgmtAllow" },
  ];

  /** Every scope's items of the list at `key`, strongest scope first, with their place in their file. */
  function items(key: string) {
    return order.flatMap((s) => {
      const v = fileValue(o, s, key);
      return Array.isArray(v) ? v.map((item, index) => ({ scope: s, item: show(item), index })) : [];
    });
  }

  const vars = $derived(
    order.flatMap((s) =>
      Object.entries((fileValue(o, s, "env") ?? {}) as Record<string, unknown>).map(([name, v]) => ({
        scope: s,
        name,
        value: show(v),
      })),
    ),
  );

  let drafts = $state<Record<string, string>>({});
  let model = $state("");
  let varName = $state("");
  let varValue = $state("");
  let showValue = $state(false);
  let revealed = $state<Record<string, string>>({});

  // A value shown stays only until the settings are read again.
  $effect(() => {
    void o.settings;
    revealed = {};
  });

  // The fields show what the chosen scope's file says.
  $effect(() => {
    const m = fileValue(o, scope, "model");
    model = typeof m === "string" ? m : "";
  });

  async function edit(origin: string, e: Edit, clear?: () => void) {
    await propose(origin, scope, e);
    if (!change.error) clear?.();
  }

  const value = (key: string) => {
    const v = fileValue(o, scope, key);
    return typeof v === "string" ? v : "";
  };

  async function toggle(s: Scope, name: string) {
    const id = `${s}:${name}`;
    if (id in revealed) {
      delete revealed[id];
      return;
    }
    const v = await reveal(s, `env.${name}`);
    if (v !== null) revealed[id] = show(v);
  }

  function decided(key: string): string {
    const e = effective(o, key);
    return e ? t("claudeMgmtDecidedBy", { scope: scopeLabel(e.scope) }) : t("claudeMgmtNotSet");
  }
</script>

{#snippet remove(s: Scope, label: string, e: Edit, origin: string)}
  {#if s === scope}
    <button
      class="icon-btn sm"
      title={t("claudeMgmtRemoveItem", { item: label })}
      aria-label={t("claudeMgmtRemoveItem", { item: label })}
      onclick={() => edit(origin, e)}><X size={13} aria-hidden="true" /></button
    >
  {/if}
{/snippet}

{#snippet list(key: string, title: string, placeholder: string, move: boolean)}
  {@const rows = items(key)}
  <div class="list">
    <h4>{title} <span class="count">{rows.length}</span></h4>
    {#if rows.length}
      <ul class="rows">
        {#each rows as r (`${r.scope}:${r.index}`)}
          <li class:mine={r.scope === scope}>
            <span class="mono rule" title={r.item}>{r.item}</span>
            <span class="badge scope">{scopeLabel(r.scope)}</span>
            {#if move && r.scope === scope}
              <label class="move" title={t("claudeMgmtMoveRule", { rule: r.item })}>
                <ArrowRightLeft size={12} aria-hidden="true" />
                <span class="sr-only">{t("claudeMgmtMoveRule", { rule: r.item })}</span>
                <select
                  class="field"
                  value=""
                  onchange={(e) => {
                    const to = e.currentTarget.value;
                    e.currentTarget.value = "";
                    if (to) edit("permissions", { op: "move", key, index: r.index, to: `permissions.${to}` });
                  }}
                >
                  <option value="" disabled>{t("claudeMgmtMoveTo")}</option>
                  {#each lists.filter((l) => `permissions.${l.id}` !== key) as l (l.id)}<option value={l.id}>{t(l.label)}</option>{/each}
                </select>
              </label>
            {/if}
            {@render remove(r.scope, r.item, { op: "drop", key, index: r.index }, "permissions")}
          </li>
        {/each}
      </ul>
    {/if}
    <form
      class="add"
      onsubmit={(e) => {
        e.preventDefault();
        const v = (drafts[key] ?? "").trim();
        if (v) edit("permissions", { op: "add", key, value: v }, () => (drafts[key] = ""));
      }}
    >
      <input
        class="field mono"
        bind:value={drafts[key]}
        {placeholder}
        aria-label={`${title} · ${t("claudeMgmtAdd")}`}
        spellcheck="false"
        autocomplete="off"
      />
      <button class="btn sm" disabled={!(drafts[key] ?? "").trim()}><Plus size={13} aria-hidden="true" />{t("claudeMgmtAdd")}</button>
    </form>
  </div>
{/snippet}

<div class="scopes">
  <span class="label">{t("claudeMgmtWriteTo")}</span>
  <div class="segmented" role="radiogroup" aria-label={t("claudeMgmtWriteTo")}>
    {#each writable as w (w.id)}
      <button
        role="radio"
        aria-checked={scope === w.id}
        disabled={w.id !== "user" && !claudeView.repo}
        onclick={() => (scope = w.id)}>{t(w.label)}</button
      >
    {/each}
  </div>
  <span class="mono muted path" title={file?.path}>{file?.path ?? ""}</span>
</div>
<p class="scope-help">
  {t(writable.find((w) => w.id === scope)?.help ?? "claudeMgmtScopeUserHelp")}
  {#if !claudeView.repo}{t("claudeMgmtNeedRepo")}{/if}
</p>

<section class="block" aria-labelledby="claude-model">
  <h3 id="claude-model">{t("claudeMgmtModelEffort")}</h3>
  <div class="pairs">
    <form
      class="pair"
      onsubmit={(e) => {
        e.preventDefault();
        edit("model", { op: "set", key: "model", value: model.trim() || null });
      }}
    >
      <label class="labelled">
        <span>{t("claudeRedesignModel")}</span>
        <input class="field mono" bind:value={model} placeholder="opus, sonnet, haiku…" spellcheck="false" autocomplete="off" />
      </label>
      <button class="btn sm">{t("claudeViewPreview")}</button>
      <span class="muted small">{decided("model")}</span>
    </form>
    <div class="pair">
      <label class="labelled">
        <span>{t("claudeMgmtEffort")}</span>
        <select
          class="field mono"
          value={value("effortLevel")}
          onchange={(e) => edit("model", { op: "set", key: "effortLevel", value: e.currentTarget.value || null })}
        >
          <option value="">{t("claudeMgmtNotSet")}</option>
          {#each efforts as v (v)}<option value={v}>{v}</option>{/each}
        </select>
      </label>
      <span class="muted small">{decided("effortLevel")}</span>
    </div>
  </div>
  {#if change.origin === "model"}<ClaudeChange />{/if}
</section>

<section class="block" aria-labelledby="claude-permissions">
  <div class="block-head">
    <h3 id="claude-permissions">{t("claudeMgmtPermissions")}</h3>
    <label class="mode">
      <span>{t("claudeMgmtDefaultMode")}</span>
      <select
        class="field mono"
        value={value("permissions.defaultMode")}
        onchange={(e) =>
          edit("permissions", { op: "set", key: "permissions.defaultMode", value: e.currentTarget.value || null })}
      >
        <option value="">{t("claudeMgmtNotSet")}</option>
        {#each modes as v (v)}<option value={v}>{v}</option>{/each}
      </select>
      <span class="muted small">{decided("permissions.defaultMode")}</span>
    </label>
  </div>
  <p class="help">{t("claudeMgmtPermissionsHelp")}</p>
  {#if change.origin === "permissions"}<ClaudeChange />{/if}
  {#each lists as l (l.id)}
    {@render list(`permissions.${l.id}`, t(l.label), "Bash(npm run *), Read(./.env), mcp__github__*", true)}
  {/each}
  {@render list("permissions.additionalDirectories", t("claudeMgmtDirs"), "~/code/shared", false)}
</section>

<section class="block" aria-labelledby="claude-env">
  <h3 id="claude-env">{t("claudeMgmtEnv")}</h3>
  <p class="help">{t("claudeMgmtEnvHelp")}</p>
  {#if change.origin === "env"}<ClaudeChange />{/if}
  {#if vars.length}
    <ul class="rows">
      {#each vars as v (`${v.scope}:${v.name}`)}
        {@const id = `${v.scope}:${v.name}`}
        <li class:mine={v.scope === scope}>
          <span class="mono var">{v.name}</span>
          <span class="mono rule" class:hidden={!(id in revealed) && v.value.includes("••••••")}>{revealed[id] ?? v.value}</span>
          {#if v.value.includes("••••••")}
            <button
              class="icon-btn sm"
              aria-pressed={id in revealed}
              title={id in revealed ? t("claudeMgmtHide") : t("claudeMgmtReveal")}
              aria-label={`${id in revealed ? t("claudeMgmtHide") : t("claudeMgmtReveal")} · ${v.name}`}
              onclick={() => toggle(v.scope, v.name)}
            >
              {#if id in revealed}<EyeOff size={13} aria-hidden="true" />{:else}<Eye size={13} aria-hidden="true" />{/if}
            </button>
          {/if}
          <span class="badge scope">{scopeLabel(v.scope)}</span>
          {@render remove(v.scope, v.name, { op: "set", key: `env.${v.name}`, value: null }, "env")}
        </li>
      {/each}
    </ul>
  {/if}
  <form
    class="add"
    onsubmit={(e) => {
      e.preventDefault();
      const name = varName.trim();
      if (name)
        edit("env", { op: "set", key: `env.${name}`, value: varValue }, () => {
          varName = "";
          varValue = "";
        });
    }}
  >
    <input class="field mono name" bind:value={varName} placeholder="NODE_ENV" aria-label={t("claudeMgmtEnvName")} spellcheck="false" autocomplete="off" />
    <span class="secret">
      <input
        class="field mono"
        type={showValue ? "text" : "password"}
        bind:value={varValue}
        placeholder={t("claudeMgmtEnvValue")}
        aria-label={t("claudeMgmtEnvValue")}
        spellcheck="false"
        autocomplete="off"
      />
      <button
        type="button"
        class="icon-btn sm"
        aria-pressed={showValue}
        title={showValue ? t("claudeMgmtHide") : t("claudeMgmtReveal")}
        aria-label={showValue ? t("claudeMgmtHide") : t("claudeMgmtReveal")}
        onclick={() => (showValue = !showValue)}
      >
        {#if showValue}<EyeOff size={13} aria-hidden="true" />{:else}<Eye size={13} aria-hidden="true" />{/if}
      </button>
    </span>
    <button class="btn sm" disabled={!varName.trim()}><Plus size={13} aria-hidden="true" />{t("claudeMgmtAdd")}</button>
  </form>
</section>

<style>
  .scopes {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin-top: 8px;
  }

  .label {
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .segmented button:disabled {
    opacity: 0.5;
  }

  .scope-help,
  .help {
    max-width: 72ch;
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .block {
    margin-top: 24px;
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .block-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
  }

  .mode {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--text-sm);
  }

  .mode select {
    width: 170px;
  }

  .pairs {
    display: flex;
    flex-wrap: wrap;
    gap: 12px 32px;
    margin-top: 10px;
  }

  .pair {
    display: flex;
    align-items: flex-end;
    gap: 8px;
  }

  .pair .labelled {
    width: 200px;
  }

  .pair .muted {
    padding-bottom: 6px;
  }

  .small {
    font-size: var(--text-xs);
  }

  h4 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 14px 0 6px;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    font-size: var(--text-xs);
    font-weight: 500;
    line-height: 16px;
    text-align: center;
  }

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
    gap: 8px;
    min-height: 34px;
    padding: 4px 8px 4px 12px;
  }

  .rows li + li {
    border-top: 1px solid var(--border);
  }

  .rows li:not(.mine) {
    color: var(--muted);
  }

  .rule {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rule.hidden {
    color: var(--muted);
    letter-spacing: 0.1em;
  }

  .var {
    width: 200px;
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .badge.scope {
    min-width: 64px;
    justify-content: center;
  }

  .move {
    position: relative;
    display: inline-grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: var(--radius);
    color: var(--muted);
  }

  .move:hover {
    background: var(--hover);
    color: var(--text);
  }

  /* The select covers the icon: one click opens the lists to move to. */
  .move select {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    opacity: 0;
    cursor: pointer;
  }

  .move:focus-within {
    outline: 1px solid var(--focus);
  }

  .add {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }

  .add .name {
    width: 200px;
    flex-shrink: 0;
  }

  .secret {
    position: relative;
    display: flex;
    flex: 1;
    align-items: center;
  }

  .secret input {
    padding-right: 30px;
  }

  .secret .icon-btn {
    position: absolute;
    right: 4px;
  }
</style>
