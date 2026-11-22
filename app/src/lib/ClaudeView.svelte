<script lang="ts">
  import { onMount, type Component, type Snippet } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import Brain from "@lucide/svelte/icons/brain";
  import Check from "@lucide/svelte/icons/check";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleUser from "@lucide/svelte/icons/circle-user";
  import Copy from "@lucide/svelte/icons/copy";
  import FileDown from "@lucide/svelte/icons/file-down";
  import Images from "@lucide/svelte/icons/images";
  import LayoutDashboard from "@lucide/svelte/icons/layout-dashboard";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plug from "@lucide/svelte/icons/plug";
  import Puzzle from "@lucide/svelte/icons/puzzle";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Eye from "@lucide/svelte/icons/eye";
  import Webhook from "@lucide/svelte/icons/webhook";
  import Wrench from "@lucide/svelte/icons/wrench";
  import ClaudeArtifacts from "$lib/ClaudeArtifacts.svelte";
  import ClaudeAccount from "$lib/ClaudeAccount.svelte";
  import ClaudeBinary from "$lib/ClaudeBinary.svelte";
  import ClaudeMemory from "$lib/ClaudeMemory.svelte";
  import ClaudeNative from "$lib/ClaudeNative.svelte";
  import ClaudeSessions from "$lib/ClaudeSessions.svelte";
  import ClaudeUsage from "$lib/ClaudeUsage.svelte";
  import Gauge from "@lucide/svelte/icons/gauge";
  import History from "@lucide/svelte/icons/history";
  import ClaudeChange from "$lib/ClaudeChange.svelte";
  import ClaudeHooks from "$lib/ClaudeHooks.svelte";
  import ClaudeMcp from "$lib/ClaudeMcp.svelte";
  import ClaudeScopeSettings from "$lib/ClaudeScopeSettings.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import SectionNav from "$lib/ui/SectionNav.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { t } from "$lib/i18n";
  import { allRepos } from "$lib/files.svelte";
  import { alerts, field, prefill, type Alert } from "$lib/claude-logic";
  import {
    blockMarkdown,
    blockTitle,
    change,
    claudeView,
    commit,
    copyText,
    discard,
    effective,
    exportOverview,
    fileValue,
    healthLabel,
    binaryOrigin,
    kindLabel,
    loadOverview,
    propose,
    reveal,
    scopeLabel,
    settingsSchema,
    show,
    size,
    stateLabel,
    type Artifacts,
    type Block,
    type Folder,
    type Health,
    type Overview,
    type Scope,
    type Section,
  } from "$lib/claude.svelte";

  type Key = Parameters<typeof t>[0];

  const repos = $derived(allRepos().sort((a, b) => a.name.localeCompare(b.name)));
  const o = $derived(claudeView.overview);
  const status = $derived(o && effective(o, "statusLine.command"));
  const theme = $derived(o && effective(o, "theme"));
  const channel = $derived(o && effective(o, "autoUpdatesChannel"));
  const model = $derived(o && effective(o, "model"));
  const mode = $derived(o && effective(o, "permissions.defaultMode"));
  const warnings = $derived(o ? alerts(o) : []);

  const sections: { id: Section; label: Key; icon: Component }[] = [
    { id: "overview", label: "claudeRedesignOverview", icon: LayoutDashboard },
    { id: "settings", label: "claudeViewSettings", icon: SlidersHorizontal },
    { id: "mcp", label: "claudeRedesignMcpPlugins", icon: Plug },
    { id: "extensions", label: "claudeRedesignExtensions", icon: Puzzle },
    { id: "hooks", label: "claudeViewHooks", icon: Webhook },
    { id: "memory", label: "claudeViewMemory", icon: Brain },
    { id: "sessions", label: "claudeDgSessions", icon: History },
    { id: "usage", label: "claudeDgUsage", icon: Gauge },
    { id: "account", label: "claudeViewAccount", icon: CircleUser },
    { id: "tools", label: "claudeViewTools", icon: Wrench },
    { id: "artifacts", label: "claudeViewTabArtifacts", icon: Images },
  ];
  const section = $derived(claudeView.section);
  const config = $derived(section !== "artifacts" && section !== "usage");
  /** Sessions have their own refresh, and nothing to export. */
  const overviewActions = $derived(config && section !== "sessions");
  const formats: ["md" | "json", Key][] = [
    ["md", "claudeViewExportMd"],
    ["json", "claudeViewExportJson"],
  ];

  /** The setting being edited; its preview is the pending change of origin "editor". */
  let edit = $state<{ key: string; scope: Scope; value: string; remove: boolean } | null>(null);
  let editError = $state("");
  let schema = $state<Record<string, unknown> | null>(null);
  const spec = $derived(edit && schema ? field(schema, edit.key) : null);
  const schemaKeys = $derived(Object.keys((schema?.properties ?? {}) as object).sort());
  const choices = $derived(spec?.kind === "bool" ? ["true", "false"] : (spec?.values ?? []));

  let exporting = $state(false);
  let exportButton = $state<HTMLButtonElement>();
  let content = $state<HTMLElement>();

  onMount(() => {
    const unlisten = listen<Artifacts>("artifacts", ({ payload }) => {
      claudeView.artifacts = payload;
    });
    return () => unlisten.then((stop) => stop());
  });

  // A section starts at its top.
  $effect(() => {
    void claudeView.section;
    if (content) content.scrollTop = 0;
  });

  function go(id: Section) {
    claudeView.section = id;
  }

  function count(id: Section): number | null {
    if (id === "artifacts") return claudeView.artifacts?.artifacts.length ?? null;
    if (!o) return null;
    switch (id) {
      case "settings":
        return o.settings.effective.length;
      case "mcp":
        return o.mcp.servers.length + o.plugins.plugins.length;
      case "extensions":
        return o.extensions.reduce((s, f) => s + f.items.length, 0);
      case "hooks":
        return o.hooks.length;
      case "memory":
        return o.memory.files.filter((f) => f.state === "ok").length + o.memory.auto.length;
      case "tools":
        return o.tools.filter((x) => x.version).length;
      default:
        return null;
    }
  }

  /** The worst thing waiting in a section, for its dot in the menu. */
  function tone(id: Section): "error" | "warn" | null {
    if (id === "artifacts") return claudeView.artifacts?.refresh.last?.failure ? "warn" : null;
    const found = warnings.filter((a) => a.section === id);
    return found.some((a) => a.tone === "error") ? "error" : found.length ? "warn" : null;
  }

  function alertText(a: Alert): string {
    if ("server" in a) return t("claudeRedesignAlertMcp", { name: a.server, state: healthLabel(a.health as Health) });
    if ("file" in a) return t("claudeRedesignAlertFile", { path: a.file }) + (a.error ? ` (${a.error})` : "");
    if ("loggedOut" in a) return t("claudeRedesignLoggedOut");
    if ("binary" in a) {
      const version = o?.account.binary.install?.version ?? "";
      return a.binary === "old"
        ? t("claudebinAlertOld", { version, minimum: o?.account.binary.minimum ?? "" })
        : t("claudebinAlertSeveral");
    }
    return a.text;
  }

  const healthTone: Record<string, string> = { connected: "ok", failed: "error", needsauth: "warn", pending: "warn" };
  const fileName = (path: string) => path.split(/[/\\]/).pop() ?? path;

  /** One line per kind of thing installed, with their names, for the overview. */
  function inventory(o: Overview): { section: Section; label: string; items: { text: string; tone?: string; off?: boolean }[] }[] {
    const kinds: Folder["kind"][] = ["skills", "agents", "commands", "output-styles", "rules"];
    const auto = o.memory.auto.length ? [{ text: `${t("claudeViewAutoMemory")} (${o.memory.auto.length})` }] : [];
    return [
      { section: "mcp", label: t("claudeViewMcp"), items: o.mcp.servers.map((s) => ({ text: s.name, tone: healthTone[s.health] })) },
      {
        section: "mcp",
        label: t("claudeViewPluginsInstalled"),
        items: o.plugins.plugins.map((p) => ({ text: show(p.id).split("@")[0], off: !p.enabled })),
      },
      ...kinds.map((kind) => ({
        section: "extensions" as const,
        label: kindLabel(kind),
        items: o.extensions
          .filter((f) => f.kind === kind)
          .flatMap((f) => f.items.map((i) => ({ text: kind === "commands" ? `/${i.name}` : i.name }))),
      })),
      { section: "hooks", label: t("claudeViewHooks"), items: o.hooks.map((h) => ({ text: h.matcher ? `${h.event} · ${h.matcher}` : h.event })) },
      {
        section: "memory",
        label: t("claudeViewMemory"),
        items: [
          ...o.memory.files.filter((f) => f.state === "ok").map((f) => ({ text: `${fileName(f.path)} · ${f.scope ? scopeLabel(f.scope) : ""}` })),
          ...auto,
        ],
      },
      { section: "tools", label: t("claudeViewTools"), items: o.tools.map((x) => ({ text: x.name, off: !x.version })) },
    ];
  }

  function startEdit(key: string) {
    const current = o && effective(o, key);
    const scope = (current && current.scope !== "managed" ? current.scope : "user") as Scope;
    edit = { key, scope, value: "", remove: false };
    load();
    go("settings");
    settingsSchema()
      .then((s) => (schema = s))
      .catch((e) => (editError = String(e)));
  }

  /** The editor shows the value the chosen scope's file has, masked like everywhere else. */
  function load() {
    if (!edit || !o) return;
    discard();
    editError = "";
    const v = prefill(fileValue(o, edit.scope, edit.key), effective(o, edit.key));
    edit.value = v === undefined ? "" : typeof v === "string" ? v : JSON.stringify(v, null, 2);
  }

  async function showReveal() {
    if (!edit) return;
    const v = await reveal(edit.scope, edit.key);
    if (v !== null) edit.value = typeof v === "string" ? v : JSON.stringify(v, null, 2);
  }

  function value(): unknown {
    if (!edit || edit.remove) return null;
    switch (spec?.kind) {
      case "bool":
        return edit.value === "true";
      case "number":
        return Number(edit.value);
      case "text":
      case "enum":
        return edit.value.trim();
      default:
        return JSON.parse(edit.value);
    }
  }

  async function showPreview() {
    if (!edit) return;
    editError = "";
    let v: unknown;
    try {
      v = value();
    } catch {
      editError = t("claudeMgmtNotJson");
      return;
    }
    await propose("editor", edit.scope, { op: "set", key: edit.key, value: v });
  }

  async function saveSetting() {
    await commit();
    if (!change.edit) edit = null;
  }
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

{#snippet head(title: string, lead: string, actions?: Snippet)}
  <header class="page-head">
    <div class="titles">
      <h2>{title}</h2>
      {#if lead}<p class="lead">{lead}</p>{/if}
    </div>
    {#if actions}<div class="head-actions">{@render actions()}</div>{/if}
  </header>
{/snippet}

{#snippet subhead(block: Block, lead: string)}
  <div class="subhead">
    <h3>{blockTitle(block)}</h3>
    {#if lead}<span class="sub-lead">{lead}</span>{/if}
    {@render copyButton(block)}
  </div>
{/snippet}

{#snippet editor()}
  {#if edit}
    {@const kind = spec?.kind ?? "json"}
    <form
      class="editor"
      onsubmit={(e) => {
        e.preventDefault();
        showPreview();
      }}
      oninput={() => change.origin === "editor" && discard()}
    >
      <div class="editor-row">
        <label class="labelled">
          <span>{t("claudeViewKey")}</span>
          <input
            class="field mono"
            list="claude-setting-keys"
            bind:value={edit.key}
            onchange={() => edit && startEdit(edit.key)}
            spellcheck="false"
            autocomplete="off"
            required
          />
          <datalist id="claude-setting-keys">{#each schemaKeys as key (key)}<option value={key}></option>{/each}</datalist>
        </label>
        <label class="labelled">
          <span>{t("claudeViewScope")}</span>
          <select class="field" bind:value={edit.scope} onchange={load}>
            <option value="user">{scopeLabel("user")}</option>
            <option value="project" disabled={!claudeView.repo}>{t("claudeMgmtScopeShared")}</option>
            <option value="local" disabled={!claudeView.repo}>{scopeLabel("local")}</option>
          </select>
        </label>
        {#if kind !== "json"}
          <label class="labelled value">
            <span>{t("claudeViewValue")}</span>
            {#if kind === "enum" || kind === "bool"}
              <select class="field mono" bind:value={edit.value} disabled={edit.remove}>
                {#each choices as v (v)}<option value={v}>{v}</option>{/each}
              </select>
            {:else}
              <input class="field mono" type={kind === "number" ? "number" : "text"} bind:value={edit.value} disabled={edit.remove} spellcheck="false" autocomplete="off" />
            {/if}
          </label>
        {/if}
        <label class="check">
          <input type="checkbox" bind:checked={edit.remove} />
          {t("claudeViewRemove")}
        </label>
      </div>
      {#if spec?.description}<p class="muted describe">{spec.description}</p>{/if}
      {#if kind === "json"}
        <label class="labelled full">
          <span>{t("claudeMgmtJsonValue")}</span>
          <textarea class="field mono json" bind:value={edit.value} disabled={edit.remove} rows="6" spellcheck="false"></textarea>
        </label>
      {/if}
      {#if edit.value.includes("••••••") && !edit.remove}
        <p class="notice warn">
          {t("claudeMgmtMaskedValue")}
          <button type="button" class="btn sm quiet" onclick={showReveal}><Eye size={13} aria-hidden="true" />{t("claudeMgmtReveal")}</button>
        </p>
      {/if}
      {#if editError}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{editError}</p>{/if}
      {#if change.origin === "editor"}
        <ClaudeChange
          onsave={saveSetting}
          oncancel={() => {
            discard();
            edit = null;
          }}
        />
      {:else}
        <div class="buttons">
          <span class="muted hint">{t("claudeViewEditHint")}</span>
          <button type="button" class="btn sm" onclick={() => (edit = null)}>{t("cancel")}</button>
          <button type="submit" class="btn sm primary">{t("claudeViewPreview")}</button>
        </div>
      {/if}
    </form>
  {/if}
{/snippet}

{#snippet fold(open: boolean, summary: Snippet, body: Snippet)}
  <details class="fold" {open}>
    <summary><ChevronRight size={14} aria-hidden="true" class="chevron" />{@render summary()}</summary>
    <div class="fold-body">{@render body()}</div>
  </details>
{/snippet}

{#snippet overviewPage(o: Overview)}
  {@render head(t("claudeRedesignOverview"), t("claudeViewIntro", { dir: o.claudeDir }))}

  <dl class="facts">
    <div class="fact">
      <dt>{t("claudeViewVersion")}</dt>
      <dd>
        <span class="fact-value mono">{o.account.version ?? t("claudeViewUnknown")}</span>
        <span class="meta">{[o.account.install, channel ? show(channel.value) : "latest"].filter(Boolean).join(" · ")}</span>
      </dd>
    </div>
    <div class="fact">
      <dt>{t("claudeViewLoggedIn")}</dt>
      <dd>
        {#if o.account.auth?.loggedIn}
          <span class="fact-value">{show(o.account.auth.email ?? t("claudeViewYes"))}</span>
          <span class="meta">{[o.account.auth.orgName, o.account.auth.subscriptionType].filter(Boolean).map(show).join(" · ")}</span>
        {:else}
          <span class="fact-value off">{o.account.auth ? t("claudeViewNo") : t("claudeViewUnknown")}</span>
        {/if}
      </dd>
    </div>
    <div class="fact">
      <dt>{t("claudeRedesignModel")}</dt>
      <dd>
        <span class="fact-value mono">{model ? show(model.value) : t("claudeViewDefault")}</span>
        {#if model}<span class="meta">{scopeLabel(model.scope)}</span>{/if}
      </dd>
    </div>
    <div class="fact">
      <dt>{t("claudeRedesignMode")}</dt>
      <dd>
        <span class="fact-value" class:mono={!!mode}>{mode ? show(mode.value) : t("claudeRedesignModeDefault")}</span>
        <span class="meta">{mode ? scopeLabel(mode.scope) : t("claudeViewDefault")}</span>
      </dd>
    </div>
  </dl>

  <section class="block" aria-labelledby="claude-attention">
    {#if warnings.length}
      <h3 id="claude-attention">{t("claudeRedesignAttention")} <span class="count">{warnings.length}</span></h3>
      <div class="alerts">
        {#each warnings as a, i (i)}
          <Banner kind={a.tone}>
            {alertText(a)}
            {#snippet action()}
              <button class="btn sm quiet see" onclick={() => go(a.section)}>{t("claudeRedesignSee")}<ChevronRight size={13} aria-hidden="true" /></button>
            {/snippet}
          </Banner>
        {/each}
      </div>
    {:else}
      <p id="claude-attention" class="notice ok"><Check size={14} aria-hidden="true" />{t("claudeRedesignAllGood")}</p>
    {/if}
  </section>

  <section class="block" aria-labelledby="claude-inventory">
    <h3 id="claude-inventory">{t("claudeRedesignInventory")}</h3>
    <ul class="inventory">
      {#each inventory(o) as line (line.label)}
        <li>
          <button class="inv" onclick={() => go(line.section)}>
            <span class="inv-label">{line.label}<span class="count">{line.items.length}</span></span>
            <span class="inv-items">
              {#each line.items.slice(0, 8) as item, i (i)}
                <span class="entry" class:off={item.off}>
                  {#if item.tone}<span class="dot {item.tone}" aria-hidden="true"></span>{/if}{item.text}
                </span>
              {:else}
                <span class="entry off">{t("claudeViewNone")}</span>
              {/each}
              {#if line.items.length > 8}<span class="entry more">+{line.items.length - 8}</span>{/if}
            </span>
            <ChevronRight size={14} aria-hidden="true" class="go" />
          </button>
        </li>
      {/each}
    </ul>
  </section>
{/snippet}

{#snippet settingsPage(o: Overview)}
  {@const invalid = o.settings.files.some((f) => f.state === "invalid")}
  {#snippet actions()}
    <button class="btn sm" onclick={() => startEdit("model")}>
      <Pencil size={13} aria-hidden="true" />{t("claudeViewEdit")}
    </button>
    {@render copyButton("settings")}
  {/snippet}
  {@render head(blockTitle("settings"), t("claudeViewSettingsHelp"), actions)}
  {@render editor()}
  <ClaudeScopeSettings {o} />
  <h3 class="all-keys">{t("claudeMgmtAllKeys")}</h3>
  {#if o.settings.effective.length}
    <div class="panel">
      <table class="settings">
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
              <td class="mono val" title={show(e.value)}>{show(e.value)}</td>
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
                <button class="icon-btn sm" onclick={() => startEdit(e.key)} aria-label={t("claudeViewEditKey", { key: e.key })} title={t("claudeViewEditKey", { key: e.key })}>
                  <Pencil size={13} aria-hidden="true" />
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <p class="empty-line">{t("claudeViewNoSettings")}</p>
  {/if}

  {#snippet filesSummary()}
    <span class="fold-title">{t("claudeRedesignSettingsFiles")}</span>
    <span class="count">{o.settings.files.length}</span>
    {#if invalid}{@render stateBadge("invalid")}{/if}
  {/snippet}
  {#snippet filesBody()}
    <ul class="files">
      {#each o.settings.files as f (f.scope)}
        <li>
          {@render scopeBadge(f.scope)}
          <span class="mono path" title={f.path}>{f.path}</span>
          {@render stateBadge(f.state)}
          {#if f.scope === "managed"}<span class="muted small">{t("claudeViewReadOnly")}</span>{/if}
          {#if f.error}<span class="err small">{f.error}</span>{/if}
        </li>
      {/each}
      {#if !claudeView.repo}<li class="muted small">{t("claudeViewPickRepo")}</li>{/if}
    </ul>
  {/snippet}
  {@render fold(invalid, filesSummary, filesBody)}

  <section class="sub">
    {@render subhead("interface", "")}
    <dl class="pairs">
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
{/snippet}

{#snippet mcpPage(o: Overview)}
  {@render head(t("claudeRedesignMcpPlugins"), "")}
  <ClaudeMcp {o} />
{/snippet}

{#snippet extensionsPage(o: Overview)}
  {#snippet actions()}{@render copyButton("extensions")}{/snippet}
  {@render head(blockTitle("extensions"), t("claudeViewExtensionsHelp"), actions)}
  <ClaudeNative {o} />
{/snippet}

{#snippet hooksPage(o: Overview)}
  {#snippet actions()}{@render copyButton("hooks")}{/snippet}
  {@render head(blockTitle("hooks"), t("claudeViewHooksHelp"), actions)}
  <ClaudeHooks {o} />
{/snippet}

{#snippet memoryPage(o: Overview)}
  {#snippet actions()}{@render copyButton("memory")}{/snippet}
  {@render head(blockTitle("memory"), t("claudeViewMemoryHelp"), actions)}
  <ClaudeMemory {o} />
{/snippet}

{#snippet accountPage(o: Overview)}
  {#snippet actions()}{@render copyButton("account")}{/snippet}
  {@render head(blockTitle("account"), "", actions)}
  {#if o.account.authError}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{o.account.authError}</p>{/if}
  <ClaudeBinary b={o.account.binary} />
  <dl class="pairs panel padded">
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
    <dt>{t("claudebinOrigin")}</dt>
    <dd>{binaryOrigin(o.account.binary.install)}</dd>
  </dl>
  <ClaudeAccount {o} />
{/snippet}

{#snippet toolsPage(o: Overview)}
  {#snippet actions()}{@render copyButton("tools")}{/snippet}
  {@render head(blockTitle("tools"), t("claudeViewToolsHelp"), actions)}
  <ul class="rows panel">
    {#each o.tools as tool (tool.name)}
      <li>
        <div class="row-line">
          <span class="name mono tool">{tool.name}</span>
          <span class="tool-version">{#if tool.version}<span class="mono">{tool.version}</span>{:else}<span class="badge state-absent">{t("claudeViewMissing")}</span>{/if}</span>
          <span class="mono muted desc grow">{tool.path ?? ""}</span>
        </div>
      </li>
    {/each}
  </ul>
{/snippet}

<div class="claude">
  <ViewHeader title={t("claudeViewTitle")}>
    {#if config}
      <label class="scope">
        <span>{t("claudeViewRepo")}</span>
        <select class="field" bind:value={claudeView.repo} onchange={loadOverview}>
          <option value="">{t("claudeViewNoRepo")}</option>
          {#each repos as repo (repo.path)}<option value={repo.path}>{repo.name}</option>{/each}
        </select>
      </label>
    {/if}
    {#snippet actions()}
      {#if overviewActions}
        <button class="btn sm quiet" onclick={loadOverview} disabled={claudeView.loading}>
          <RefreshCw size={14} aria-hidden="true" class={claudeView.loading ? "spin" : ""} />{t("claudeViewRefresh")}
        </button>
        <button
          bind:this={exportButton}
          class="btn sm quiet"
          aria-haspopup="dialog"
          aria-expanded={exporting}
          onclick={() => (exporting = !exporting)}
          disabled={!o}
        >
          <FileDown size={14} aria-hidden="true" />{t("claudeRedesignExport")}<ChevronDown size={14} aria-hidden="true" />
        </button>
      {/if}
    {/snippet}
  </ViewHeader>

  <Popover bind:open={exporting} trigger={exportButton} label={t("claudeRedesignExport")} align="end" width={240}>
    <ul class="menu">
      {#each formats as [format, label] (format)}
        <li>
          <button
            class="menu-item"
            onclick={() => {
              exporting = false;
              exportOverview(format);
            }}
          >
            <FileDown size={14} aria-hidden="true" />{t(label)}
          </button>
        </li>
      {/each}
    </ul>
  </Popover>

  <div class="body">
    <SectionNav
      items={sections.map((s) => ({
        id: s.id,
        label: t(s.label),
        icon: s.icon,
        dot: tone(s.id),
        count: count(s.id),
        apart: s.id === "artifacts",
      }))}
      current={section}
      label={t("claudeViewSections")}
      onselect={go}
    />

    <div class="content" bind:this={content}>
      {#if claudeView.error || claudeView.done}
        <div class="banners">
          {#if claudeView.error}
            <Banner kind="error" ondismiss={() => (claudeView.error = "")}>{claudeView.error}</Banner>
          {/if}
          {#if claudeView.done}
            <Banner kind="ok" ondismiss={() => (claudeView.done = "")}>{claudeView.done}</Banner>
          {/if}
        </div>
      {/if}

      <div class="page">
        {#if section === "artifacts"}
          <ClaudeArtifacts />
        {:else if section === "sessions"}
          <ClaudeSessions />
        {:else if section === "usage"}
          <ClaudeUsage />
        {:else if !o && claudeView.loading}
          <Skeleton shape={section === "overview" ? "cards" : "lines"} rows={section === "overview" ? 4 : 6} label={t("claudeViewLoading")} />
        {:else if !o}
          <EmptyState icon={CircleAlert}>
            <button class="btn sm" onclick={loadOverview}><RefreshCw size={14} aria-hidden="true" />{t("claudeViewRefresh")}</button>
          </EmptyState>
        {:else if section === "overview"}
          {@render overviewPage(o)}
        {:else if section === "settings"}
          {@render settingsPage(o)}
        {:else if section === "mcp"}
          {@render mcpPage(o)}
        {:else if section === "extensions"}
          {@render extensionsPage(o)}
        {:else if section === "hooks"}
          {@render hooksPage(o)}
        {:else if section === "memory"}
          {@render memoryPage(o)}
        {:else if section === "account"}
          {@render accountPage(o)}
        {:else if section === "tools"}
          {@render toolsPage(o)}
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .claude {
    display: flex;
    flex-direction: column;
    height: 100%;
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

  .scope select {
    width: 220px;
  }

  .menu {
    margin: 0;
    padding: 6px;
    list-style: none;
  }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
  }

  .menu-item:hover {
    background: var(--hover);
  }

  .menu-item :global(svg) {
    color: var(--muted);
  }

  /* Same frame as the Settings view's sections. */
  .body {
    display: grid;
    flex: 1;
    grid-template: minmax(0, 1fr) / 224px minmax(0, 1fr);
    min-height: 0;
  }

  .dot {
    flex-shrink: 0;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--border-strong);
  }

  .dot.ok {
    background: var(--done);
  }

  .dot.warn {
    background: var(--waiting);
  }

  .dot.error {
    background: var(--danger);
  }

  .content {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
  }

  .page {
    width: 100%;
    max-width: 960px;
    padding: 20px 28px 40px;
  }

  .page-head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 12px 24px;
    margin-bottom: 16px;
  }

  .titles {
    flex: 1 1 360px;
    min-width: 0;
  }

  h2 {
    margin: 0 0 4px;
    font-size: var(--text-xl);
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .lead {
    max-width: 68ch;
    margin: 0;
    color: var(--muted);
  }

  .head-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
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

  .block {
    margin-top: 28px;
  }

  .block > h3 {
    margin-bottom: 10px;
  }

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

  .sub-lead {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .subhead:not(:has(.sub-lead)) h3 {
    flex: 1;
  }

  .small {
    font-size: var(--text-xs);
  }

  .err {
    color: var(--danger);
  }

  .empty-line {
    margin: 4px 0 0;
    color: var(--muted);
  }

  /* Overview: the four things to know, one row. */
  .facts {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    margin: 4px 0 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .fact {
    min-width: 0;
    padding: 12px 14px;
    border-left: 1px solid var(--border);
  }

  .fact:first-child {
    border-left: 0;
  }

  .fact dt {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .fact dd {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    margin: 4px 0 0;
  }

  .fact-value {
    overflow: hidden;
    font-size: var(--text-md);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fact-value.mono {
    font-size: var(--text-md);
  }

  .fact-value.off {
    color: var(--muted);
  }

  .fact .meta {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .alerts {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .see {
    color: inherit;
  }

  .inventory {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    list-style: none;
  }

  .inventory li + li {
    border-top: 1px solid var(--border);
  }

  .inv {
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 40px;
    padding: 8px 12px 8px 16px;
    border: 0;
    background: none;
    color: var(--text);
    text-align: left;
  }

  .inventory li:first-child .inv {
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
  }

  .inventory li:last-child .inv {
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
  }

  .inv:hover {
    background: var(--hover);
  }

  .inv-label {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .inv-items {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    min-width: 0;
  }

  .entry {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    width: auto;
    min-height: 0;
    padding: 0;
    font-size: var(--text-base);
    font-weight: 400;
  }

  .entry.off,
  .inv-items .more {
    color: var(--muted);
  }

  .inv :global(.go) {
    color: var(--muted);
  }

  .panel {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .panel.padded {
    padding: 12px 16px;
  }

  .rows {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows > li + li {
    border-top: 1px solid var(--border);
  }

  .row-line {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 6px 12px;
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

  .tool {
    width: 56px;
  }

  .tool-version {
    width: 80px;
  }

  /* Folded details: a summary line that opens on click or Enter. */
  .fold {
    margin-top: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .fold > summary {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding: 6px 12px;
    border-radius: var(--radius-lg);
    cursor: pointer;
    list-style: none;
  }

  .fold > summary::-webkit-details-marker {
    display: none;
  }

  .fold > summary:hover {
    background: var(--hover);
  }

  .fold :global(.chevron) {
    flex-shrink: 0;
    color: var(--muted);
    transition: transform var(--dur-fast) var(--ease-out);
  }

  .fold[open] > summary :global(.chevron) {
    transform: rotate(90deg);
  }

  .fold-title {
    font-weight: 600;
  }

  .fold .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fold-body {
    padding: 2px 12px 12px 34px;
  }

  .folder {
    margin: 0 0 6px;
  }

  .files {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
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

  table.settings {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }

  .settings th {
    height: 34px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 600;
    text-align: left;
  }

  .settings th:first-child {
    border-top-left-radius: var(--radius-lg);
  }

  .settings th:last-child {
    width: 40px;
    border-top-right-radius: var(--radius-lg);
  }

  .settings td {
    height: 38px;
    padding: 4px 12px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }

  .settings tr:last-child td {
    border-bottom: 0;
  }

  td.key {
    width: 34%;
    word-break: break-all;
  }

  td.val {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  td.act {
    padding: 0 8px;
  }

  .decided {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }

  .pairs {
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr);
    gap: 8px 16px;
    margin: 0;
  }

  .pairs dt {
    color: var(--muted);
  }

  .pairs dd {
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
    margin: 0 0 16px;
    padding: 12px 14px;
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

  .editor .labelled {
    width: 220px;
  }

  .editor .labelled.value {
    width: 240px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    font-size: var(--text-sm);
  }

  .describe {
    max-width: 80ch;
    margin: 0;
    font-size: var(--text-sm);
  }

  .editor .labelled.full {
    width: 100%;
  }

  .editor .json {
    height: auto;
    padding: 6px 8px;
    resize: vertical;
  }

  .all-keys {
    margin: 28px 0 10px;
  }

  .buttons {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .buttons .hint {
    flex: 1;
    font-size: var(--text-sm);
  }
</style>
