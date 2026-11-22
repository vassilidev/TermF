<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Copy from "@lucide/svelte/icons/copy";
  import Eye from "@lucide/svelte/icons/eye";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import FileJson from "@lucide/svelte/icons/file-json";
  import LogIn from "@lucide/svelte/icons/log-in";
  import LogOut from "@lucide/svelte/icons/log-out";
  import Plus from "@lucide/svelte/icons/plus";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import X from "@lucide/svelte/icons/x";
  import ClaudeChange from "$lib/ClaudeChange.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { t } from "$lib/i18n";
  import {
    blockMarkdown,
    blockTitle,
    claudeView,
    copyText,
    healthLabel,
    scopeLabel,
    show,
    type Block,
    type McpServer,
    type Overview,
    type Plugins,
    type Preview,
    type Shown,
  } from "$lib/claude.svelte";

  let { o }: { o: Overview } = $props();

  const repo = () => claudeView.repo || null;
  let busy = $state("");

  /** Through `claude mcp …`; the servers are listed again after. */
  async function mcp(action: Record<string, unknown>, key: string, done: string): Promise<boolean> {
    busy = key;
    try {
      const r = await invoke<{ message: string; mcp: Overview["mcp"] }>("claude_mcp", { repo: repo(), action });
      o.mcp = r.mcp;
      claudeView.done = done;
      return true;
    } catch (e) {
      claudeView.error = String(e);
      return false;
    } finally {
      busy = "";
    }
  }

  /** Through `claude plugin …`; a command the marketplace would run comes back to be shown, not run. */
  async function plugin(action: Record<string, unknown>, key: string, done: string): Promise<Shown | null | false> {
    busy = key;
    try {
      const r = await invoke<{ message: string; shown: Shown | null; plugins: Plugins }>("claude_plugin", { repo: repo(), action });
      o.plugins = r.plugins;
      if (r.shown) return r.shown;
      claudeView.done = done;
      return null;
    } catch (e) {
      claudeView.error = String(e);
      return false;
    } finally {
      busy = "";
    }
  }

  const kind = (s: McpServer) => String(s.details?.Type ?? "").toLowerCase();
  const remote = (s: McpServer) => ["http", "sse"].includes(kind(s)) || s.health === "needsauth";

  // Adding a server.
  type Pair = { key: string; value: string; shown: boolean };
  let adding = $state(false);
  let form = $state({ name: "", transport: "stdio", scope: "local", command: "", args: "", url: "" });
  let pairs = $state<Pair[]>([{ key: "", value: "", shown: false }]);
  const transports = ["stdio", "http", "sse"];

  function resetForm() {
    adding = false;
    form = { name: "", transport: "stdio", scope: "local", command: "", args: "", url: "" };
    pairs = [{ key: "", value: "", shown: false }];
  }

  async function addServer(e: SubmitEvent) {
    e.preventDefault();
    const map = Object.fromEntries(pairs.filter((p) => p.key.trim()).map((p) => [p.key.trim(), p.value]));
    const has = Object.keys(map).length > 0;
    const server =
      form.transport === "stdio"
        ? {
            type: "stdio",
            command: form.command.trim(),
            ...(form.args.trim() ? { args: form.args.split("\n").map((a) => a.trim()).filter(Boolean) } : {}),
            ...(has ? { env: map } : {}),
          }
        : { type: form.transport, url: form.url.trim(), ...(has ? { headers: map } : {}) };
    const name = form.name.trim();
    if (await mcp({ op: "add", name, scope: form.scope, server }, "add", t("claudeMgmtMcpAdded", { name }))) resetForm();
  }

  // The repository's .mcp.json.
  let json = $state<{ path: string; text: string } | null>(null);
  let jsonPreview = $state<Preview | null>(null);
  let jsonError = $state("");

  async function openJson() {
    try {
      const r = await invoke<{ path: string; text: string }>("claude_mcp_json", { repo: repo() });
      json = { path: r.path, text: r.text || '{\n  "mcpServers": {}\n}\n' };
      jsonPreview = null;
      jsonError = "";
    } catch (e) {
      claudeView.error = String(e);
    }
  }

  async function previewJson() {
    if (!json) return;
    jsonError = "";
    try {
      jsonPreview = await invoke<Preview>("claude_mcp_json_preview", { repo: repo(), text: json.text });
    } catch (e) {
      jsonPreview = null;
      jsonError = String(e);
    }
  }

  async function saveJson() {
    if (!json) return;
    busy = "json";
    try {
      o.mcp = await invoke<Overview["mcp"]>("claude_mcp_json_save", { repo: repo(), text: json.text });
      claudeView.done = t("claudeMgmtSaved", { path: json.path });
      json = null;
      jsonPreview = null;
    } catch (e) {
      jsonError = String(e);
    } finally {
      busy = "";
    }
  }

  // Questions asked before acting.
  type Ask =
    | { kind: "removeServer"; name: string; scope: string | null }
    | { kind: "install"; id: string; scope: string; entry: Record<string, unknown> }
    | { kind: "shown"; action: Record<string, unknown>; shown: Shown; done: string }
    | { kind: "uninstall"; id: string; scope: string }
    | { kind: "removeMarket"; name: string };
  let ask = $state<Ask | null>(null);
  let asking = $state(false);
  const question = (a: Ask) => {
    ask = a;
    asking = true;
  };

  const askTitle = (a: Ask) =>
    ({
      removeServer: () => t("claudeMgmtRemoveServerTitle", { name: (a as { name: string }).name }),
      install: () => t("claudeMgmtInstallTitle", { id: (a as { id: string }).id }),
      shown: () => t("claudeMgmtShownTitle"),
      uninstall: () => t("claudeMgmtUninstallTitle", { id: (a as { id: string }).id }),
      removeMarket: () => t("claudeMgmtRemoveMarketTitle", { name: (a as { name: string }).name }),
    })[a.kind]();

  const askConfirm = (a: Ask) =>
    ({
      removeServer: t("claudeMgmtRemove"),
      install: t("claudeMgmtInstall"),
      shown: t("claudeMgmtRunAndInstall"),
      uninstall: t("claudeMgmtUninstall"),
      removeMarket: t("claudeMgmtRemove"),
    })[a.kind];

  async function answer() {
    const a = ask;
    if (!a) return;
    if (a.kind === "removeServer") {
      await mcp({ op: "remove", name: a.name, scope: a.scope }, `remove:${a.name}`, t("claudeMgmtMcpRemoved", { name: a.name }));
    } else if (a.kind === "install") {
      const action = { op: "install", plugin: a.id, scope: a.scope };
      const done = t("claudeMgmtInstalled", { id: a.id });
      const shown = await plugin(action, "install", done);
      if (shown) question({ kind: "shown", action, shown, done });
      else if (shown === null) installing = false;
    } else if (a.kind === "shown") {
      if ((await plugin({ ...a.action, accept: a.shown.sha256 }, "install", a.done)) === null) installing = false;
    } else if (a.kind === "uninstall") {
      await plugin({ op: "uninstall", plugin: a.id, scope: a.scope }, `uninstall:${a.id}`, t("claudeMgmtUninstalled", { id: a.id }));
    } else {
      await plugin({ op: "marketplaceRemove", name: a.name }, `market:${a.name}`, t("claudeMgmtMarketRemoved", { name: a.name }));
    }
  }

  async function update(id: string, scope: string) {
    const action = { op: "update", plugin: id, scope };
    const done = t("claudeMgmtUpdated", { id });
    const shown = await plugin(action, `update:${id}`, done);
    if (shown) question({ kind: "shown", action, shown, done });
  }

  // Installing a plugin a marketplace offers.
  let installing = $state(false);
  let pick = $state("");
  let pluginScope = $state("user");
  const picked = $derived(o.plugins.available.find((p) => p.pluginId === pick));

  /** The marketplace a plugin comes from, with where that marketplace is fetched from. */
  function market(entry: Record<string, unknown>): string {
    const m = o.plugins.marketplaces.find((m) => m.name === entry.marketplaceName);
    const from = m ? show(m.repo ?? m.url ?? m.path ?? m.source ?? "") : "";
    return [show(entry.marketplaceName ?? ""), from].filter(Boolean).join(" · ");
  }

  /** Where the plugin's own code comes from: a folder of its marketplace, or its own source (npm, git, URL…). */
  function source(entry: Record<string, unknown>): string {
    const own = entry.source;
    if (own && typeof own === "object") {
      const s = own as Record<string, unknown>;
      const where = s.repo ?? s.url ?? s.package ?? s.path;
      return where === undefined ? JSON.stringify(own) : `${show(s.source ?? "")} ${show(where)}`.trim();
    }
    return show(own ?? "");
  }

  let marketSource = $state("");
  let marketScope = $state("user");
  async function addMarket(e: SubmitEvent) {
    e.preventDefault();
    const src = marketSource.trim();
    if ((await plugin({ op: "marketplaceAdd", source: src, scope: marketScope }, "market", t("claudeMgmtMarketAdded", { source: src }))) === null)
      marketSource = "";
  }

  const installable = (scope: unknown) => scope === "user" || scope === "project" || scope === "local";
</script>

{#snippet copy(block: Block)}
  <button class="btn sm quiet" onclick={() => copyText(blockMarkdown(o, block), t("claudeViewCopied", { block: blockTitle(block) }))} title={t("claudeViewCopyTitle")}>
    <Copy size={13} aria-hidden="true" />{t("claudeViewCopy")}
  </button>
{/snippet}

{#snippet scopeSelect(value: string, set: (v: string) => void, order: string[])}
  <select class="field" {value} onchange={(e) => set(e.currentTarget.value)}>
    {#each order as s (s)}
      <option value={s} disabled={s !== "user" && !claudeView.repo}>{s === "project" ? t("claudeMgmtScopeShared") : scopeLabel(s)}</option>
    {/each}
  </select>
{/snippet}

<section class="sub first">
  <div class="subhead">
    <h3>{blockTitle("mcp")}</h3>
    <span class="sub-lead">{t("claudeViewMcpHelp")}</span>
    {@render copy("mcp")}
  </div>
  {#if o.mcp.error}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{o.mcp.error}</p>{/if}
  {#if o.mcp.servers.length}
    <ul class="rows panel">
      {#each o.mcp.servers as s (s.name)}
        {@const details = Object.entries(s.details ?? {})}
        <li>
          <details class="row-fold">
            <summary>
              <span class="what">
                <span class="name">{s.name}</span>
                <span class="mono desc">{s.target}</span>
              </span>
              {#if s.scope}<span class="badge scope">{scopeLabel(s.scope)}</span>{/if}
              <span class="badge health-{s.health}" title={s.status}>{healthLabel(s.health)}</span>
              <ChevronDown size={14} aria-hidden="true" class="chevron" />
            </summary>
            {#if details.length}
              <dl class="pairs">
                {#each details as [k, v] (k)}
                  <dt>{k}</dt>
                  <dd class="mono">{typeof v === "object" && v ? Object.entries(v).map(([a, b]) => `${a}=${b}`).join("  ") : show(v)}</dd>
                {/each}
              </dl>
            {/if}
          </details>
          <span class="actions">
            {#if remote(s)}
              <button
                class="icon-btn"
                title={t("claudeMgmtLogin")}
                aria-label={`${t("claudeMgmtLogin")} · ${s.name}`}
                disabled={!!busy}
                onclick={() => mcp({ op: "login", name: s.name }, `login:${s.name}`, t("claudeMgmtLoggedIn", { name: s.name }))}
              >
                {#if busy === `login:${s.name}`}<RefreshCw size={14} aria-hidden="true" class="spin" />{:else}<LogIn size={14} aria-hidden="true" />{/if}
              </button>
              <button
                class="icon-btn"
                title={t("claudeMgmtLogout")}
                aria-label={`${t("claudeMgmtLogout")} · ${s.name}`}
                disabled={!!busy}
                onclick={() => mcp({ op: "logout", name: s.name }, `logout:${s.name}`, t("claudeMgmtLoggedOut", { name: s.name }))}
                ><LogOut size={14} aria-hidden="true" /></button
              >
            {/if}
            {#if s.scope && s.scope !== "managed" && !s.name.startsWith("plugin:") && !s.name.startsWith("claude.ai ")}
              <button
                class="icon-btn"
                title={t("claudeMgmtRemoveServer")}
                aria-label={`${t("claudeMgmtRemoveServer")} · ${s.name}`}
                disabled={!!busy}
                onclick={() => question({ kind: "removeServer", name: s.name, scope: s.scope })}><Trash2 size={14} aria-hidden="true" /></button
              >
            {/if}
          </span>
        </li>
      {/each}
    </ul>
  {:else if !o.mcp.error}
    <p class="empty-line">{t("claudeViewNoMcp")}</p>
  {/if}
  {#if busy.startsWith("login:")}<p class="notice"><RefreshCw size={14} aria-hidden="true" class="spin" />{t("claudeMgmtLoginWait")}</p>{/if}

  {#if adding}
    <form class="form" onsubmit={addServer}>
      <h4>{t("claudeMgmtAddServer")}</h4>
      <div class="grid">
        <label class="labelled">
          <span>{t("claudeViewName")}</span>
          <input class="field mono" bind:value={form.name} placeholder="github" required spellcheck="false" autocomplete="off" />
        </label>
        <div class="labelled">
          <span>{t("claudeMgmtTransport")}</span>
          <div class="segmented" role="radiogroup" aria-label={t("claudeMgmtTransport")}>
            {#each transports as tr (tr)}
              <button type="button" role="radio" aria-checked={form.transport === tr} onclick={() => (form.transport = tr)}>{tr}</button>
            {/each}
          </div>
        </div>
        <label class="labelled">
          <span>{t("claudeViewScope")}</span>
          {@render scopeSelect(form.scope, (v) => (form.scope = v), ["local", "project", "user"])}
        </label>
      </div>
      {#if form.transport === "stdio"}
        <label class="labelled">
          <span>{t("claudeMgmtCommand")}</span>
          <input class="field mono" bind:value={form.command} placeholder="npx" required spellcheck="false" autocomplete="off" />
        </label>
        <label class="labelled">
          <span>{t("claudeMgmtArgs")}</span>
          <textarea class="field mono area" bind:value={form.args} rows="2" placeholder={"-y\n@modelcontextprotocol/server-github"} spellcheck="false"></textarea>
        </label>
      {:else}
        <label class="labelled">
          <span>URL</span>
          <input class="field mono" bind:value={form.url} placeholder="https://mcp.example.com/mcp" required spellcheck="false" autocomplete="off" />
        </label>
      {/if}
      <div class="labelled">
        <span>{form.transport === "stdio" ? t("claudeMgmtEnv") : t("claudeMgmtHeaders")}</span>
        {#each pairs as p, i (i)}
          <div class="pair-row">
            <input
              class="field mono key"
              bind:value={p.key}
              placeholder={form.transport === "stdio" ? "GITHUB_TOKEN" : "Authorization"}
              aria-label={t("claudeViewName")}
              spellcheck="false"
              autocomplete="off"
            />
            <span class="secret">
              <input class="field mono" type={p.shown ? "text" : "password"} bind:value={p.value} aria-label={t("claudeViewValue")} spellcheck="false" autocomplete="off" />
              <button
                type="button"
                class="icon-btn sm"
                aria-pressed={p.shown}
                title={p.shown ? t("claudeMgmtHide") : t("claudeMgmtReveal")}
                aria-label={p.shown ? t("claudeMgmtHide") : t("claudeMgmtReveal")}
                onclick={() => (p.shown = !p.shown)}
              >
                {#if p.shown}<EyeOff size={13} aria-hidden="true" />{:else}<Eye size={13} aria-hidden="true" />{/if}
              </button>
            </span>
            <button type="button" class="icon-btn sm" aria-label={t("claudeMgmtRemoveRow")} title={t("claudeMgmtRemoveRow")} onclick={() => pairs.splice(i, 1)}
              ><X size={13} aria-hidden="true" /></button
            >
          </div>
        {/each}
        <button type="button" class="btn sm quiet more" onclick={() => pairs.push({ key: "", value: "", shown: false })}
          ><Plus size={13} aria-hidden="true" />{t("claudeMgmtAddRow")}</button
        >
      </div>
      {#if form.scope === "project"}<Banner kind="warn">{t("claudeMgmtSharedMcp", { reference: "${VARIABLE}" })}</Banner>{/if}
      <div class="buttons">
        <span class="muted hint">{t("claudeMgmtMcpHint")}</span>
        <button type="button" class="btn sm" onclick={resetForm}>{t("cancel")}</button>
        <button class="btn sm primary" disabled={!!busy || !form.name.trim()}>
          {#if busy === "add"}<RefreshCw size={13} aria-hidden="true" class="spin" />{/if}{t("claudeMgmtAdd")}
        </button>
      </div>
    </form>
  {/if}

  {#if json}
    <div class="form">
      <h4>{t("claudeMgmtEditMcpJson")}</h4>
      <p class="mono muted path">{json.path}</p>
      <textarea
        class="field mono area json"
        bind:value={json.text}
        rows="10"
        spellcheck="false"
        aria-label=".mcp.json"
        oninput={() => {
          jsonPreview = null;
          jsonError = "";
        }}
      ></textarea>
      <p class="muted hint">{t("claudeMgmtMcpJsonHint", { reference: "${VARIABLE}" })}</p>
      {#if jsonPreview || jsonError}
        <ClaudeChange
          preview={jsonPreview}
          error={jsonError}
          saving={busy === "json"}
          onsave={saveJson}
          oncancel={() => {
            jsonPreview = null;
            jsonError = "";
          }}
        />
      {:else}
        <div class="buttons">
          <button class="btn sm" onclick={() => (json = null)}>{t("cancel")}</button>
          <button class="btn sm primary" onclick={previewJson}>{t("claudeViewPreview")}</button>
        </div>
      {/if}
    </div>
  {/if}

  {#if !adding && !json}
    <div class="toolbar">
      <button class="btn sm" onclick={() => (adding = true)}><Plus size={13} aria-hidden="true" />{t("claudeMgmtAddServer")}</button>
      <button class="btn sm quiet" onclick={openJson} disabled={!claudeView.repo} title={claudeView.repo ? "" : t("claudeMgmtNeedRepo")}>
        <FileJson size={13} aria-hidden="true" />{t("claudeMgmtEditMcpJson")}
      </button>
      <button
        class="btn sm quiet"
        disabled={!claudeView.repo || !!busy}
        title={t("claudeMgmtResetHelp")}
        onclick={() => mcp({ op: "reset" }, "reset", t("claudeMgmtResetDone"))}
      >
        <RotateCcw size={13} aria-hidden="true" />{t("claudeMgmtReset")}
      </button>
    </div>
  {/if}
</section>

<section class="sub">
  <div class="subhead">
    <h3>{blockTitle("plugins")}</h3>
    <span class="sub-lead">{t("claudeViewPluginsHelp")}</span>
    {@render copy("plugins")}
  </div>
  {#if o.plugins.error}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{o.plugins.error}</p>{/if}
  <h4>{t("claudeViewPluginsInstalled")} <span class="count">{o.plugins.plugins.length}</span></h4>
  {#if o.plugins.plugins.length}
    <ul class="rows panel">
      {#each o.plugins.plugins as p, i (i)}
        {@const id = show(p.id)}
        <li>
          <div class="row-line">
            <span class="what"><span class="name mono">{id}</span></span>
            {#if p.version}<span class="muted small mono">{show(p.version)}</span>{/if}
            {#if typeof p.scope === "string"}<span class="badge scope">{scopeLabel(p.scope)}</span>{/if}
            {#if installable(p.scope)}
              <input
                type="checkbox"
                class="switch"
                checked={!!p.enabled}
                disabled={!!busy}
                aria-label={`${t("claudeViewEnabled")} · ${id}`}
                title={p.enabled ? t("claudeViewOn") : t("claudeViewOff")}
                onchange={(e) => {
                  const on = e.currentTarget.checked;
                  plugin({ op: on ? "enable" : "disable", plugin: id, scope: p.scope }, `toggle:${id}`, t(on ? "claudeMgmtEnabled" : "claudeMgmtDisabled", { id }));
                }}
              />
            {:else}
              <span class="badge {p.enabled ? 'state-ok' : ''}">{p.enabled ? t("claudeViewOn") : t("claudeViewOff")}</span>
            {/if}
            <span class="actions">
              {#if installable(p.scope) || p.scope === "managed"}
                <button class="icon-btn" title={t("claudeMgmtUpdate")} aria-label={`${t("claudeMgmtUpdate")} · ${id}`} disabled={!!busy} onclick={() => update(id, String(p.scope))}>
                  <RefreshCw size={14} aria-hidden="true" class={busy === `update:${id}` ? "spin" : ""} />
                </button>
              {/if}
              {#if installable(p.scope)}
                <button
                  class="icon-btn"
                  title={t("claudeMgmtUninstall")}
                  aria-label={`${t("claudeMgmtUninstall")} · ${id}`}
                  disabled={!!busy}
                  onclick={() => question({ kind: "uninstall", id, scope: String(p.scope) })}><Trash2 size={14} aria-hidden="true" /></button
                >
              {/if}
            </span>
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="empty-line">{t("claudeViewNone")}</p>
  {/if}

  {#if installing}
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        if (picked) question({ kind: "install", id: pick, scope: pluginScope, entry: picked });
      }}
    >
      <h4>{t("claudeMgmtInstallPlugin")}</h4>
      {#if o.plugins.available.length}
        <div class="grid two">
          <label class="labelled">
            <span>{t("claudeMgmtPlugin")}</span>
            <select class="field mono" bind:value={pick} required>
              <option value="" disabled>{t("claudeMgmtPickPlugin")}</option>
              {#each o.plugins.available as a (a.pluginId)}<option value={a.pluginId}>{show(a.pluginId)}</option>{/each}
            </select>
          </label>
          <label class="labelled">
            <span>{t("claudeViewScope")}</span>
            {@render scopeSelect(pluginScope, (v) => (pluginScope = v), ["user", "project", "local"])}
          </label>
        </div>
        {#if picked}
          <dl class="pairs">
            {#if picked.description}<dt>{t("claudeViewDescription")}</dt><dd>{show(picked.description)}</dd>{/if}
            <dt>{t("claudeViewMarketplaces")}</dt>
            <dd class="mono">{market(picked)}</dd>
            <dt>{t("claudeViewSource")}</dt>
            <dd class="mono">{source(picked)}</dd>
          </dl>
        {/if}
      {:else}
        <p class="empty-line">{t("claudeMgmtNoAvailable")}</p>
      {/if}
      <div class="buttons">
        <button type="button" class="btn sm" onclick={() => (installing = false)}>{t("cancel")}</button>
        <button class="btn sm primary" disabled={!picked || !!busy}>
          {#if busy === "install"}<RefreshCw size={13} aria-hidden="true" class="spin" />{/if}{t("claudeMgmtInstall")}…
        </button>
      </div>
    </form>
  {:else}
    <div class="toolbar">
      <button class="btn sm" onclick={() => (installing = true)}><Plus size={13} aria-hidden="true" />{t("claudeMgmtInstallPlugin")}</button>
    </div>
  {/if}

  <h4>{t("claudeViewMarketplaces")} <span class="count">{o.plugins.marketplaces.length}</span></h4>
  {#if o.plugins.marketplaces.length}
    <ul class="rows panel">
      {#each o.plugins.marketplaces as m, i (i)}
        {@const name = show(m.name)}
        <li>
          <div class="row-line">
            <span class="what">
              <span class="name">{name}</span>
              <span class="mono desc">{show(m.repo ?? m.url ?? m.path ?? m.source ?? "")}</span>
            </span>
            <span class="actions">
              <button
                class="icon-btn"
                title={t("claudeMgmtUpdate")}
                aria-label={`${t("claudeMgmtUpdate")} · ${name}`}
                disabled={!!busy}
                onclick={() => plugin({ op: "marketplaceUpdate", name }, `market:${name}`, t("claudeMgmtMarketUpdated", { name }))}
              >
                <RefreshCw size={14} aria-hidden="true" class={busy === `market:${name}` ? "spin" : ""} />
              </button>
              <button
                class="icon-btn"
                title={t("claudeMgmtRemove")}
                aria-label={`${t("claudeMgmtRemove")} · ${name}`}
                disabled={!!busy}
                onclick={() => question({ kind: "removeMarket", name })}><Trash2 size={14} aria-hidden="true" /></button
              >
            </span>
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="empty-line">{t("claudeViewNone")}</p>
  {/if}
  <form class="add" onsubmit={addMarket}>
    <input
      class="field mono"
      bind:value={marketSource}
      placeholder="owner/repo, https://…, ./path"
      aria-label={t("claudeMgmtMarketSource")}
      spellcheck="false"
      autocomplete="off"
    />
    <span class="scope-pick">{@render scopeSelect(marketScope, (v) => (marketScope = v), ["user", "project", "local"])}</span>
    <button class="btn sm" disabled={!marketSource.trim() || !!busy}>
      {#if busy === "market"}<RefreshCw size={13} aria-hidden="true" class="spin" />{:else}<Plus size={13} aria-hidden="true" />{/if}{t("claudeMgmtAddMarket")}
    </button>
  </form>
</section>

{#if ask}
  <ConfirmDialog
    bind:open={asking}
    title={askTitle(ask)}
    confirm={askConfirm(ask)}
    danger={ask.kind !== "install"}
    onconfirm={answer}
  >
    {#if ask.kind === "removeServer"}
      <p>{t("claudeMgmtRemoveServerText")}</p>
    {:else if ask.kind === "install"}
      <dl class="pairs">
        <dt>{t("claudeViewMarketplaces")}</dt>
        <dd class="mono">{market(ask.entry)}</dd>
        <dt>{t("claudeViewSource")}</dt>
        <dd class="mono">{source(ask.entry)}</dd>
        <dt>{t("claudeViewScope")}</dt>
        <dd>{ask.scope === "project" ? t("claudeMgmtScopeShared") : scopeLabel(ask.scope)}</dd>
      </dl>
      <Banner kind="warn">{t("claudeMgmtPluginRuns")}</Banner>
    {:else if ask.kind === "shown"}
      <p>{t("claudeMgmtShownText")}</p>
      <pre class="command">{ask.shown.command ?? JSON.stringify(ask.shown, null, 2)}</pre>
      <p class="mono small">sha256 {ask.shown.sha256}</p>
      <Banner kind="warn">{t("claudeMgmtShownWarning")}</Banner>
    {:else if ask.kind === "uninstall"}
      <p>{t("claudeMgmtUninstallText")}</p>
    {:else}
      <p>{t("claudeMgmtRemoveMarketText")}</p>
    {/if}
  </ConfirmDialog>
{/if}

<style>
  .sub {
    margin-top: 32px;
  }

  .sub.first {
    margin-top: 0;
  }

  .subhead {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 30px;
    margin-bottom: 8px;
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .sub-lead {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  h4 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 16px 0 6px;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .form h4 {
    margin: 0;
    color: var(--text);
    font-size: var(--text-md);
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

  .small {
    font-size: var(--text-xs);
  }

  .empty-line {
    margin: 4px 0 0;
    color: var(--muted);
  }

  .panel {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .rows {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows > li {
    display: flex;
    align-items: flex-start;
  }

  .rows > li + li {
    border-top: 1px solid var(--border);
  }

  .row-fold {
    flex: 1;
    min-width: 0;
  }

  .row-line,
  .row-fold > summary {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 10px;
    min-width: 0;
    min-height: 44px;
    padding: 6px 12px;
  }

  .row-fold > summary {
    cursor: pointer;
    list-style: none;
  }

  .row-fold > summary::-webkit-details-marker {
    display: none;
  }

  .row-fold > summary:hover {
    background: var(--hover);
  }

  .row-fold :global(.chevron) {
    flex-shrink: 0;
    color: var(--muted);
    transition: transform var(--dur-fast) var(--ease-out);
  }

  .row-fold[open] :global(.chevron) {
    transform: rotate(180deg);
  }

  .actions {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 2px;
    min-height: 44px;
    padding-right: 8px;
  }

  .row-line .actions {
    min-height: 0;
    padding-right: 0;
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

  .state-ok,
  .health-connected {
    border-color: transparent;
    background: var(--done-soft);
    color: var(--done);
  }

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

  .pairs {
    display: grid;
    grid-template-columns: 140px minmax(0, 1fr);
    gap: 6px 16px;
    margin: 0;
  }

  .row-fold .pairs {
    padding: 4px 12px 12px 29px;
  }

  .pairs dt {
    color: var(--muted);
  }

  .pairs dd {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 12px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--chrome);
  }

  .grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto 180px;
    align-items: end;
    gap: 10px 14px;
  }

  .grid.two {
    grid-template-columns: minmax(0, 1fr) 180px;
  }

  .area {
    height: auto;
    padding: 6px 8px;
    resize: vertical;
  }

  .json {
    min-height: 180px;
  }

  .path {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .pair-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .pair-row .key {
    width: 220px;
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

  .more {
    align-self: flex-start;
  }

  .hint {
    flex: 1;
    margin: 0;
    font-size: var(--text-xs);
  }

  .buttons {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  .add {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }

  .scope-pick {
    width: 160px;
    flex-shrink: 0;
  }

  .command {
    max-height: 200px;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--text);
    font: var(--text-xs) / 1.6 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
