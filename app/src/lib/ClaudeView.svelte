<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Copy from "@lucide/svelte/icons/copy";
  import FileDown from "@lucide/svelte/icons/file-down";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import X from "@lucide/svelte/icons/x";
  import ClaudeArtifacts from "$lib/ClaudeArtifacts.svelte";
  import { t } from "$lib/i18n";
  import { allRepos } from "$lib/files.svelte";
  import {
    BLOCKS,
    EDITABLE,
    blockMarkdown,
    blockTitle,
    claudeView,
    copyText,
    effective,
    exportOverview,
    healthLabel,
    kindLabel,
    loadOverview,
    scopeLabel,
    show,
    size,
    stateLabel,
    type Artifacts,
    type Block,
    type Preview,
    type Scope,
    type Settings,
  } from "$lib/claude.svelte";

  const repos = $derived(allRepos().sort((a, b) => a.name.localeCompare(b.name)));
  const o = $derived(claudeView.overview);
  const status = $derived(o && effective(o, "statusLine.command"));
  const theme = $derived(o && effective(o, "theme"));
  const channel = $derived(o && effective(o, "autoUpdatesChannel"));

  /** The setting being edited, and its preview once asked. */
  let edit = $state<{ key: string; scope: Scope; value: string; remove: boolean } | null>(null);
  let preview = $state<Preview | null>(null);
  let editError = $state("");
  let saving = $state(false);

  onMount(() => {
    const unlisten = listen<Artifacts>("artifacts", ({ payload }) => (claudeView.artifacts = payload));
    return () => unlisten.then((stop) => stop());
  });

  function counts(block: Block): string {
    if (!o) return "";
    const n = {
      settings: o.settings.effective.length,
      mcp: o.mcp.servers.length,
      plugins: o.plugins.plugins.length,
      extensions: o.extensions.reduce((s, f) => s + f.items.length, 0),
      hooks: o.hooks.length,
      memory: o.memory.files.filter((f) => f.state === "ok").length + o.memory.auto.length,
      interface: o.interface.themes.length + (o.interface.keybindings.state === "ok" ? 1 : 0),
      account: Object.keys(o.account.auth ?? {}).length,
      tools: o.tools.filter((x) => x.version).length,
    }[block];
    return String(n);
  }

  function startEdit(key: string) {
    const current = o && effective(o, key);
    const scope = (current && current.scope !== "managed" ? current.scope : "user") as Scope;
    edit = { key, scope, value: current ? show(current.value) : "", remove: false };
    preview = null;
    editError = "";
  }

  function value(): unknown {
    if (!edit || edit.remove) return null;
    const kind = EDITABLE[edit.key].kind;
    if (kind === "bool") return edit.value === "true";
    if (kind === "number") return Number(edit.value);
    return edit.value.trim();
  }

  async function showPreview() {
    if (!edit) return;
    editError = "";
    try {
      preview = await invoke<Preview>("claude_setting_preview", {
        scope: edit.scope,
        key: edit.key,
        value: value(),
        repo: claudeView.repo || null,
      });
    } catch (e) {
      preview = null;
      editError = String(e);
    }
  }

  async function saveSetting() {
    if (!edit || !o) return;
    saving = true;
    try {
      o.settings = await invoke<Settings>("claude_setting_save", {
        scope: edit.scope,
        key: edit.key,
        value: value(),
        repo: claudeView.repo || null,
      });
      claudeView.done = t("claudeViewSaved", { key: edit.key, path: preview?.path ?? "" });
      edit = null;
      preview = null;
    } catch (e) {
      editError = String(e);
    } finally {
      saving = false;
    }
  }

  const diffLines = (diff: string) => diff.replace(/\n$/, "").split("\n");
</script>

{#snippet copyButton(block: Block)}
  {#if o}
    <button
      class="btn sm quiet"
      onclick={() => o && copyText(blockMarkdown(o, block), t("claudeViewCopied", { block: blockTitle(block) }))}
      title={t("claudeViewCopyTitle")}
    >
      <Copy size={13} aria-hidden="true" />{t("claudeViewCopy")}
    </button>
  {/if}
{/snippet}

{#snippet stateBadge(s: "absent" | "invalid" | "ok")}
  <span class="badge state-{s}">{stateLabel(s)}</span>
{/snippet}

{#snippet scopeBadge(scope: string)}
  <span class="badge scope">{scopeLabel(scope)}</span>
{/snippet}

{#snippet editor()}
  {#if edit}
    {@const spec = EDITABLE[edit.key]}
    <form
      class="editor"
      onsubmit={(e) => {
        e.preventDefault();
        showPreview();
      }}
    >
      <div class="editor-row">
        <label class="labelled">
          <span>{t("claudeViewKey")}</span>
          <select class="field mono" bind:value={edit.key} onchange={() => edit && startEdit(edit.key)}>
            {#each Object.keys(EDITABLE) as key (key)}<option value={key}>{key}</option>{/each}
          </select>
        </label>
        <label class="labelled">
          <span>{t("claudeViewScope")}</span>
          <select class="field" bind:value={edit.scope} onchange={() => (preview = null)}>
            <option value="user">{scopeLabel("user")}</option>
            <option value="project" disabled={!claudeView.repo}>{scopeLabel("project")}</option>
            <option value="local" disabled={!claudeView.repo}>{scopeLabel("local")}</option>
          </select>
        </label>
        <label class="labelled value">
          <span>{t("claudeViewValue")}</span>
          {#if spec.kind === "enum"}
            <select class="field mono" bind:value={edit.value} disabled={edit.remove} onchange={() => (preview = null)}>
              {#each spec.values ?? [] as v (v)}<option value={v}>{v}</option>{/each}
            </select>
          {:else if spec.kind === "bool"}
            <select class="field mono" bind:value={edit.value} disabled={edit.remove} onchange={() => (preview = null)}>
              <option value="true">true</option>
              <option value="false">false</option>
            </select>
          {:else}
            <input
              class="field mono"
              type={spec.kind === "number" ? "number" : "text"}
              min={spec.kind === "number" ? 1 : undefined}
              bind:value={edit.value}
              disabled={edit.remove}
              oninput={() => (preview = null)}
              spellcheck="false"
              autocomplete="off"
            />
          {/if}
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={edit.remove} onchange={() => (preview = null)} />
          {t("claudeViewRemove")}
        </label>
      </div>
      {#if editError}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{editError}</p>{/if}
      {#if preview}
        <p class="file">{t("claudeViewPreviewOf", { path: preview.path })}</p>
        {#if preview.diff}
          <pre class="code diff">{#each diffLines(preview.diff) as line, i (i)}<span
                class="l{line[0] === '+' ? ' add' : line[0] === '-' ? ' del' : ''}">{line}
</span>{/each}</pre>
        {:else}
          <p class="notice"><Check size={14} aria-hidden="true" />{t("claudeViewNoChange")}</p>
        {/if}
      {/if}
      <div class="buttons">
        <span class="muted hint">{t("claudeViewEditHint")}</span>
        <button type="button" class="btn sm" onclick={() => ((edit = null), (preview = null))}>{t("cancel")}</button>
        <button type="submit" class="btn sm">{t("claudeViewPreview")}</button>
        <button type="button" class="btn sm primary" disabled={!preview?.diff || saving} onclick={saveSetting}>
          {t("save")}
        </button>
      </div>
    </form>
  {/if}
{/snippet}

<div class="claude">
  <div class="tabs" role="tablist" aria-label={t("claudeViewTitle")}>
    <button role="tab" aria-selected={claudeView.tab === "config"} onclick={() => (claudeView.tab = "config")}>
      {t("claudeViewTabConfig")}
    </button>
    <button role="tab" aria-selected={claudeView.tab === "artifacts"} onclick={() => (claudeView.tab = "artifacts")}>
      {t("claudeViewTabArtifacts")}{#if claudeView.artifacts}<span class="count">{claudeView.artifacts.artifacts.length}</span>{/if}
    </button>
    {#if claudeView.tab === "config"}
      <div class="spacer"></div>
      <label class="scope">
        <span>{t("claudeViewRepo")}</span>
        <select class="field" bind:value={claudeView.repo} onchange={loadOverview}>
          <option value="">{t("claudeViewNoRepo")}</option>
          {#each repos as repo (repo.path)}<option value={repo.path}>{repo.name}</option>{/each}
        </select>
      </label>
      <button class="btn sm" onclick={loadOverview} disabled={claudeView.loading}>
        <RefreshCw size={14} aria-hidden="true" class={claudeView.loading ? "spin" : ""} />{t("claudeViewRefresh")}
      </button>
      <button class="btn sm" onclick={() => exportOverview("md")} disabled={!o}>
        <FileDown size={14} aria-hidden="true" />{t("claudeViewExportMd")}
      </button>
      <button class="btn sm" onclick={() => exportOverview("json")} disabled={!o}>
        <FileDown size={14} aria-hidden="true" />{t("claudeViewExportJson")}
      </button>
    {/if}
  </div>

  {#if claudeView.error || claudeView.done}
    <div class="notices">
      {#if claudeView.error}
        <div class="notice error banner" role="alert">
          <CircleAlert size={14} aria-hidden="true" /><span>{claudeView.error}</span>
          <button class="icon-btn sm" onclick={() => (claudeView.error = "")} aria-label={t("dismiss")} title={t("dismiss")}>
            <X size={14} aria-hidden="true" />
          </button>
        </div>
      {/if}
      {#if claudeView.done}
        <div class="notice ok banner" role="status">
          <Check size={14} aria-hidden="true" /><span>{claudeView.done}</span>
          <button class="icon-btn sm" onclick={() => (claudeView.done = "")} aria-label={t("dismiss")} title={t("dismiss")}>
            <X size={14} aria-hidden="true" />
          </button>
        </div>
      {/if}
    </div>
  {/if}

  <div class="pane">
    {#if claudeView.tab === "artifacts"}
      <ClaudeArtifacts />
    {:else if !o}
      <p class="loading muted" role="status">
        {#if claudeView.loading}<LoaderCircle size={16} aria-hidden="true" class="spin" />{t("claudeViewLoading")}{/if}
      </p>
    {:else}
      <div class="intro">
        <p class="muted">
          {t("claudeViewIntro", { dir: o.claudeDir })}
        </p>
        <nav class="jump" aria-label={t("claudeViewSections")}>
          {#each BLOCKS as block (block)}
            <a href="#claude-{block}">{blockTitle(block)} <span class="count">{counts(block)}</span></a>
          {/each}
        </nav>
      </div>

      <section class="card" id="claude-settings">
        <header>
          <h2>{blockTitle("settings")}</h2>
          <span class="muted desc">{t("claudeViewSettingsHelp")}</span>
          <button class="btn sm quiet" onclick={() => startEdit("model")}>
            <Pencil size={13} aria-hidden="true" />{t("claudeViewEdit")}
          </button>
          {@render copyButton("settings")}
        </header>
        {@render editor()}
        <ul class="files">
          {#each o.settings.files as f (f.scope)}
            <li>
              {@render scopeBadge(f.scope)}
              <span class="mono path">{f.path}</span>
              {@render stateBadge(f.state)}
              {#if f.scope === "managed"}<span class="muted small">{t("claudeViewReadOnly")}</span>{/if}
              {#if f.error}<span class="err small">{f.error}</span>{/if}
            </li>
          {/each}
          {#if !claudeView.repo}<li class="muted small">{t("claudeViewPickRepo")}</li>{/if}
        </ul>
        {#if o.settings.effective.length}
          <table>
            <thead>
              <tr>
                <th>{t("claudeViewKey")}</th>
                <th>{t("claudeViewValue")}</th>
                <th>{t("claudeViewDecidedBy")}</th>
                <th><span class="sr-only">{t("claudeViewEdit")}</span></th>
              </tr>
            </thead>
            <tbody>
              {#each o.settings.effective as e (e.key)}
                <tr>
                  <td class="mono key">{e.key}</td>
                  <td class="mono val">{show(e.value)}</td>
                  <td>
                    <div class="decided">
                      {@render scopeBadge(e.scope)}
                      {#if e.others.length}
                        <span class="muted small">
                          {e.merged ? t("claudeViewMergedWith") : t("claudeViewOverrides")}
                          {e.others.map(scopeLabel).join(", ")}
                        </span>
                      {/if}
                    </div>
                  </td>
                  <td class="act">
                    {#if e.key in EDITABLE}
                      <button class="icon-btn sm" onclick={() => startEdit(e.key)} aria-label={t("claudeViewEditKey", { key: e.key })} title={t("claudeViewEditKey", { key: e.key })}>
                        <Pencil size={13} aria-hidden="true" />
                      </button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <p class="empty-line muted">{t("claudeViewNoSettings")}</p>
        {/if}
      </section>

      <section class="card" id="claude-mcp">
        <header>
          <h2>{blockTitle("mcp")}</h2>
          <span class="muted desc">{t("claudeViewMcpHelp")}</span>
          {@render copyButton("mcp")}
        </header>
        {#if o.mcp.error}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{o.mcp.error}</p>{/if}
        {#if o.mcp.servers.length}
          <ul class="rows">
            {#each o.mcp.servers as s (s.name)}
              <li>
                <div class="what">
                  <span class="name">{s.name}</span>
                  <span class="mono desc">{s.target}</span>
                  {#each Object.entries(s.details ?? {}).filter(([, v]) => typeof v === "object" && v) as [k, v] (k)}
                    <span class="mono desc">{k}: {Object.entries(v as object).map(([a, b]) => `${a}=${b}`).join("  ")}</span>
                  {/each}
                </div>
                {#if s.scope}{@render scopeBadge(s.scope)}{/if}
                <span class="badge health-{s.health}" title={s.status}>{healthLabel(s.health)}</span>
              </li>
            {/each}
          </ul>
        {:else if !o.mcp.error}
          <p class="empty-line muted">{t("claudeViewNoMcp")}</p>
        {/if}
      </section>

      <section class="card" id="claude-plugins">
        <header>
          <h2>{blockTitle("plugins")}</h2>
          <span class="muted desc">{t("claudeViewPluginsHelp")}</span>
          {@render copyButton("plugins")}
        </header>
        {#if o.plugins.error}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{o.plugins.error}</p>{/if}
        <h3>{t("claudeViewPluginsInstalled")} <span class="count">{o.plugins.plugins.length}</span></h3>
        {#if o.plugins.plugins.length}
          <ul class="rows">
            {#each o.plugins.plugins as p, i (i)}
              <li>
                <div class="what"><span class="name mono">{show(p.id)}</span></div>
                {#if p.version}<span class="muted small mono">{show(p.version)}</span>{/if}
                {#if typeof p.scope === "string"}{@render scopeBadge(p.scope)}{/if}
                <span class="badge {p.enabled ? 'state-ok' : ''}">{p.enabled ? t("claudeViewOn") : t("claudeViewOff")}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="empty-line muted">{t("claudeViewNone")}</p>
        {/if}
        <h3>{t("claudeViewMarketplaces")} <span class="count">{o.plugins.marketplaces.length}</span></h3>
        {#if o.plugins.marketplaces.length}
          <ul class="rows">
            {#each o.plugins.marketplaces as m, i (i)}
              <li>
                <div class="what">
                  <span class="name">{show(m.name)}</span>
                  <span class="mono desc">{show(m.repo ?? m.url ?? m.path ?? m.source ?? "")}</span>
                </div>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="empty-line muted">{t("claudeViewNone")}</p>
        {/if}
      </section>

      <section class="card" id="claude-extensions">
        <header>
          <h2>{blockTitle("extensions")}</h2>
          <span class="muted desc">{t("claudeViewExtensionsHelp")}</span>
          {@render copyButton("extensions")}
        </header>
        {#each o.extensions as f (f.path)}
          <h3>{kindLabel(f.kind)} <span class="count">{f.items.length}</span> {@render scopeBadge(f.scope)} <span class="mono small muted">{f.path}</span></h3>
          {#if f.items.length}
            <ul class="rows compact">
              {#each f.items as item (item.path)}
                <li>
                  <span class="name mono">{item.name}</span>
                  <span class="muted desc grow">{item.description ?? ""}</span>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="empty-line muted">{t("claudeViewEmptyFolder")}</p>
          {/if}
        {:else}
          <p class="empty-line muted">{t("claudeViewNone")}</p>
        {/each}
      </section>

      <section class="card" id="claude-hooks">
        <header>
          <h2>{blockTitle("hooks")}</h2>
          <span class="muted desc">{t("claudeViewHooksHelp")}</span>
          {@render copyButton("hooks")}
        </header>
        {#if o.hooks.length}
          <ul class="rows compact">
            {#each o.hooks as h, i (i)}
              <li>
                <span class="name mono">{h.event}{h.matcher ? ` · ${h.matcher}` : ""}</span>
                <span class="mono desc grow">{h.kind === "command" ? h.run : `${h.kind}: ${h.run}`}</span>
                {@render scopeBadge(h.scope)}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="empty-line muted">{t("claudeViewNone")}</p>
        {/if}
      </section>

      <section class="card" id="claude-memory">
        <header>
          <h2>{blockTitle("memory")}</h2>
          <span class="muted desc">{t("claudeViewMemoryHelp")}</span>
          {@render copyButton("memory")}
        </header>
        <ul class="files">
          {#each o.memory.files as f (f.path)}
            <li>
              {#if f.scope}{@render scopeBadge(f.scope)}{/if}
              <span class="mono path">{f.path}</span>
              {@render stateBadge(f.state)}
              {#if f.size !== null}<span class="muted small">{size(f.size)}</span>{/if}
            </li>
          {/each}
        </ul>
        <h3>{t("claudeViewAutoMemory")} <span class="count">{o.memory.auto.length}</span></h3>
        {#if o.memory.auto.length}
          <ul class="rows compact">
            {#each o.memory.auto as m (m.path)}
              <li>
                <span class="mono name">{m.project || m.path}</span>
                <span class="muted desc grow">{m.files.map(([n]) => n).join(", ")}</span>
                <span class="muted small">{t("claudeViewFileCount", { count: m.files.length })} · {size(m.files.reduce((s, [, b]) => s + b, 0))}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="empty-line muted">{t("claudeViewNone")}</p>
        {/if}
      </section>

      <section class="card" id="claude-interface">
        <header>
          <h2>{blockTitle("interface")}</h2>
          {@render copyButton("interface")}
        </header>
        <dl>
          <dt>{t("claudeViewStatusLine")}</dt>
          <dd>{#if status}<span class="mono">{show(status.value)}</span> {@render scopeBadge(status.scope)}{:else}<span class="muted">{t("claudeViewDefault")}</span>{/if}</dd>
          <dt>{t("claudeViewTheme")}</dt>
          <dd>{#if theme}<span class="mono">{show(theme.value)}</span> {@render scopeBadge(theme.scope)}{:else}<span class="muted">{t("claudeViewDefault")}</span>{/if}</dd>
          <dt>{t("claudeViewCustomThemes")}</dt>
          <dd class="mono">{o.interface.themes.join(", ") || "—"}</dd>
          <dt>{t("claudeViewKeybindings")}</dt>
          <dd><span class="mono">{o.interface.keybindings.path}</span> {@render stateBadge(o.interface.keybindings.state)}</dd>
        </dl>
      </section>

      <section class="card" id="claude-account">
        <header>
          <h2>{blockTitle("account")}</h2>
          {@render copyButton("account")}
        </header>
        {#if o.account.authError}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{o.account.authError}</p>{/if}
        <dl>
          {#if o.account.auth}
            <dt>{t("claudeViewLoggedIn")}</dt>
            <dd>
              <span class="badge {o.account.auth.loggedIn ? 'state-ok' : 'state-absent'}">
                {o.account.auth.loggedIn ? t("claudeViewYes") : t("claudeViewNo")}
              </span>
            </dd>
            {#each Object.entries(o.account.auth).filter(([k, v]) => k !== "loggedIn" && (v === null || typeof v !== "object")) as [k, v] (k)}
              <dt class="mono">{k}</dt>
              <dd class="mono">{show(v)}</dd>
            {/each}
          {/if}
          <dt>{t("claudeViewVersion")}</dt>
          <dd class="mono">{o.account.version ?? t("claudeViewUnknown")}</dd>
          <dt>{t("claudeViewChannel")}</dt>
          <dd>
            {#if channel}<span class="mono">{show(channel.value)}</span> {@render scopeBadge(channel.scope)}{:else}<span class="mono">latest</span> <span class="muted">({t("claudeViewDefault")})</span>{/if}
            <button class="icon-btn sm" onclick={() => startEdit("autoUpdatesChannel")} aria-label={t("claudeViewEditKey", { key: "autoUpdatesChannel" })} title={t("claudeViewEditKey", { key: "autoUpdatesChannel" })}>
              <Pencil size={13} aria-hidden="true" />
            </button>
          </dd>
          <dt>{t("claudeViewInstall")}</dt>
          <dd>{o.account.install ?? t("claudeViewUnknown")} {#if o.account.path}<span class="mono muted small">{o.account.path}</span>{/if}</dd>
        </dl>
      </section>

      <section class="card" id="claude-tools">
        <header>
          <h2>{blockTitle("tools")}</h2>
          <span class="muted desc">{t("claudeViewToolsHelp")}</span>
          {@render copyButton("tools")}
        </header>
        <ul class="rows compact">
          {#each o.tools as tool (tool.name)}
            <li>
              <span class="name mono tool">{tool.name}</span>
              <span class="tool-version">{#if tool.version}<span class="mono">{tool.version}</span>{:else}<span class="badge state-absent">{t("claudeViewMissing")}</span>{/if}</span>
              <span class="mono muted desc grow">{tool.path ?? ""}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>
</div>

<style>
  .claude {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .scope {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .tabs .btn {
    margin-left: 4px;
  }

  .scope select {
    height: 26px;
    width: 240px;
  }

  .spacer {
    flex: 1;
  }

  .tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  [role="tab"] {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 10px;
    border: 0;
    background: none;
    color: var(--muted);
    font-weight: 500;
    box-shadow: inset 0 -2px 0 transparent;
  }

  [role="tab"]:hover {
    color: var(--text);
  }

  [role="tab"][aria-selected="true"] {
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    line-height: 16px;
    text-align: center;
  }

  .notices {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .banner {
    align-items: center;
  }

  .banner > span {
    flex: 1;
  }

  .banner .icon-btn {
    color: inherit;
  }

  .notice.ok {
    background: var(--done-soft);
    color: var(--done);
  }

  .pane {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 16px 40px;
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin: 40px auto;
  }

  .intro {
    max-width: 1100px;
    margin: 0 auto 12px;
  }

  .intro p {
    margin: 0 0 8px;
    font-size: var(--text-sm);
  }

  .jump {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .jump a {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    color: var(--text);
    font-size: var(--text-sm);
    text-decoration: none;
  }

  .jump a:hover {
    background: var(--hover);
  }

  .card {
    max-width: 1100px;
    margin: 0 auto 12px;
    padding: 4px 14px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    scroll-margin-top: 8px;
  }

  .card header {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 40px;
  }

  .card h2 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .card header .desc {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .card header:not(:has(.desc)) h2 {
    flex: 1;
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 12px 0 4px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .mono {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .small {
    font-size: var(--text-xs);
  }

  .err {
    color: var(--danger);
  }

  .files {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0 0 10px;
    padding: 0;
    list-style: none;
  }

  .files li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge.scope {
    min-width: 64px;
    justify-content: center;
  }

  .state-ok,
  .health-connected {
    border-color: transparent;
    background: var(--done-soft);
    color: var(--done);
  }

  .state-invalid,
  .health-failed {
    border-color: transparent;
    background: var(--danger-soft);
    color: var(--danger);
  }

  .health-pending,
  .health-needsauth {
    border-color: transparent;
    background: var(--waiting-soft);
    color: var(--warning);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  th {
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    text-align: left;
  }

  td {
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }

  tr:last-child td {
    border-bottom: 0;
  }

  td.key {
    width: 32%;
    word-break: break-all;
  }

  td.val {
    max-width: 380px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  td.act {
    width: 30px;
  }

  .tool {
    width: 56px;
  }

  .tool-version {
    width: 80px;
  }

  .decided {
    display: flex;
    align-items: center;
    gap: 8px;
    white-space: nowrap;
  }

  .rows {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 4px;
    border-bottom: 1px solid var(--border);
  }

  .rows.compact li {
    padding: 4px;
  }

  .rows li:last-child {
    border-bottom: 0;
  }

  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    flex-shrink: 0;
    font-weight: 500;
  }

  .desc {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-xs);
    line-height: 1.5;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .grow {
    flex: 1;
    min-width: 0;
  }

  .empty-line {
    margin: 4px 0 0;
    font-size: var(--text-sm);
  }

  dl {
    display: grid;
    grid-template-columns: 180px 1fr;
    gap: 6px 12px;
    margin: 4px 0 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .editor {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0 0 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--chrome);
  }

  .editor-row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px 14px;
  }

  .labelled {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 240px;
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .labelled.value {
    width: 260px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    font-size: var(--text-sm);
  }

  .buttons {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .buttons .hint {
    flex: 1;
    font-size: var(--text-xs);
  }

  .file {
    margin: 0;
    color: var(--muted);
    font: var(--text-xs) var(--font-mono);
  }

  .code {
    max-height: 280px;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    font: var(--text-xs) / 1.6 var(--font-mono);
    white-space: pre;
  }

  .diff .l {
    display: block;
  }

  .diff .add {
    background: var(--done-soft);
    color: var(--done);
  }

  .diff .del {
    background: var(--danger-soft);
    color: var(--danger);
  }
</style>
