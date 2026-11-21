<script lang="ts">
  import { onMount, type Component, type Snippet } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
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
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plug from "@lucide/svelte/icons/plug";
  import Puzzle from "@lucide/svelte/icons/puzzle";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Webhook from "@lucide/svelte/icons/webhook";
  import Wrench from "@lucide/svelte/icons/wrench";
  import ClaudeArtifacts from "$lib/ClaudeArtifacts.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { t } from "$lib/i18n";
  import { allRepos } from "$lib/files.svelte";
  import { alerts, type Alert } from "$lib/claude-logic";
  import {
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
    type Folder,
    type Health,
    type Overview,
    type Preview,
    type Scope,
    type Section,
    type Settings,
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
    { id: "account", label: "claudeViewAccount", icon: CircleUser },
    { id: "tools", label: "claudeViewTools", icon: Wrench },
    { id: "artifacts", label: "claudeViewTabArtifacts", icon: Images },
  ];
  const section = $derived(claudeView.section);
  const config = $derived(section !== "artifacts");
  const formats: ["md" | "json", Key][] = [
    ["md", "claudeViewExportMd"],
    ["json", "claudeViewExportJson"],
  ];

  /** The setting being edited, and its preview once asked. */
  let edit = $state<{ key: string; scope: Scope; value: string; remove: boolean } | null>(null);
  let preview = $state<Preview | null>(null);
  let editError = $state("");
  let saving = $state(false);

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
    edit = { key, scope, value: current ? show(current.value) : "", remove: false };
    preview = null;
    editError = "";
    go("settings");
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
        <span class="fact-value mono">{mode ? show(mode.value) : "default"}</span>
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
  <section class="sub first">
    {@render subhead("mcp", t("claudeViewMcpHelp"))}
    {#if o.mcp.error}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{o.mcp.error}</p>{/if}
    {#if o.mcp.servers.length}
      <ul class="rows panel">
        {#each o.mcp.servers as s (s.name)}
          {@const details = Object.entries(s.details ?? {})}
          <li>
            {#snippet line()}
              <span class="what">
                <span class="name">{s.name}</span>
                <span class="mono desc">{s.target}</span>
              </span>
              {#if s.scope}{@render scopeBadge(s.scope)}{/if}
              <span class="badge health-{s.health}" title={s.status}>{healthLabel(s.health)}</span>
            {/snippet}
            {#if details.length}
              <details class="row-fold">
                <summary>{@render line()}<ChevronDown size={14} aria-hidden="true" class="chevron" /></summary>
                <dl class="pairs details">
                  {#each details as [k, v] (k)}
                    <dt>{k}</dt>
                    <dd class="mono">{typeof v === "object" && v ? Object.entries(v).map(([a, b]) => `${a}=${b}`).join("  ") : show(v)}</dd>
                  {/each}
                </dl>
              </details>
            {:else}
              <div class="row-line">{@render line()}<span class="chevron-space"></span></div>
            {/if}
          </li>
        {/each}
      </ul>
    {:else if !o.mcp.error}
      <p class="empty-line">{t("claudeViewNoMcp")}</p>
    {/if}
  </section>

  <section class="sub">
    {@render subhead("plugins", t("claudeViewPluginsHelp"))}
    {#if o.plugins.error}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{o.plugins.error}</p>{/if}
    <h4>{t("claudeViewPluginsInstalled")} <span class="count">{o.plugins.plugins.length}</span></h4>
    {#if o.plugins.plugins.length}
      <ul class="rows panel">
        {#each o.plugins.plugins as p, i (i)}
          <li>
            <div class="row-line">
              <span class="what"><span class="name mono">{show(p.id)}</span></span>
              {#if p.version}<span class="muted small mono">{show(p.version)}</span>{/if}
              {#if typeof p.scope === "string"}{@render scopeBadge(p.scope)}{/if}
              <span class="badge {p.enabled ? 'state-ok' : ''}">{p.enabled ? t("claudeViewOn") : t("claudeViewOff")}</span>
            </div>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty-line">{t("claudeViewNone")}</p>
    {/if}
    <h4>{t("claudeViewMarketplaces")} <span class="count">{o.plugins.marketplaces.length}</span></h4>
    {#if o.plugins.marketplaces.length}
      <ul class="rows panel">
        {#each o.plugins.marketplaces as m, i (i)}
          <li>
            <div class="row-line">
              <span class="what">
                <span class="name">{show(m.name)}</span>
                <span class="mono desc">{show(m.repo ?? m.url ?? m.path ?? m.source ?? "")}</span>
              </span>
            </div>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty-line">{t("claudeViewNone")}</p>
    {/if}
  </section>
{/snippet}

{#snippet extensionsPage(o: Overview)}
  {#snippet actions()}{@render copyButton("extensions")}{/snippet}
  {@render head(blockTitle("extensions"), t("claudeViewExtensionsHelp"), actions)}
  {#each o.extensions as f (f.path)}
    {#snippet summary()}
      <span class="fold-title">{kindLabel(f.kind)}</span>
      <span class="count">{f.items.length}</span>
      {@render scopeBadge(f.scope)}
      <span class="mono muted path">{f.path}</span>
    {/snippet}
    {#snippet body()}
      {#if f.items.length}
        <ul class="items">
          {#each f.items as item (item.path)}
            <li>
              <span class="name mono">{f.kind === "commands" ? `/${item.name}` : item.name}</span>
              <span class="desc">{item.description ?? ""}</span>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="empty-line">{t("claudeViewEmptyFolder")}</p>
      {/if}
    {/snippet}
    {@render fold(false, summary, body)}
  {:else}
    <EmptyState icon={Puzzle} text={t("claudeViewNone")} />
  {/each}
{/snippet}

{#snippet hooksPage(o: Overview)}
  {#snippet actions()}{@render copyButton("hooks")}{/snippet}
  {@render head(blockTitle("hooks"), t("claudeViewHooksHelp"), actions)}
  {#if o.hooks.length}
    <ul class="rows panel">
      {#each o.hooks as h, i (i)}
        <li>
          <div class="row-line">
            <span class="what">
              <span class="name">{h.event}{#if h.matcher}<span class="matcher mono">{h.matcher}</span>{/if}</span>
              <span class="mono desc">{h.kind === "command" ? h.run : `${h.kind}: ${h.run}`}</span>
            </span>
            {@render scopeBadge(h.scope)}
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <EmptyState icon={Webhook} text={t("claudeViewNone")} />
  {/if}
{/snippet}

{#snippet memoryPage(o: Overview)}
  {#snippet actions()}{@render copyButton("memory")}{/snippet}
  {@render head(blockTitle("memory"), t("claudeViewMemoryHelp"), actions)}
  <ul class="rows panel">
    {#each o.memory.files as f (f.path)}
      <li>
        <div class="row-line">
          <span class="what">
            <span class="name">{fileName(f.path)}</span>
            <span class="mono desc" title={f.path}>{f.path}</span>
          </span>
          {#if f.size !== null}<span class="muted small">{size(f.size)}</span>{/if}
          {#if f.scope}{@render scopeBadge(f.scope)}{/if}
          {@render stateBadge(f.state)}
        </div>
      </li>
    {/each}
  </ul>
  <h4>{t("claudeViewAutoMemory")} <span class="count">{o.memory.auto.length}</span></h4>
  {#each o.memory.auto as m (m.path)}
    {#snippet summary()}
      <span class="fold-title mono">{m.project || m.path}</span>
      <span class="muted small">{t("claudeViewFileCount", { count: m.files.length })} · {size(m.files.reduce((s, [, b]) => s + b, 0))}</span>
    {/snippet}
    {#snippet body()}
      <p class="mono muted small folder">{m.path}</p>
      <ul class="items">
        {#each m.files as [name, bytes] (name)}
          <li><span class="name mono">{name}</span><span class="desc">{size(bytes)}</span></li>
        {/each}
      </ul>
    {/snippet}
    {@render fold(false, summary, body)}
  {:else}
    <p class="empty-line">{t("claudeViewNone")}</p>
  {/each}
{/snippet}

{#snippet accountPage(o: Overview)}
  {#snippet actions()}{@render copyButton("account")}{/snippet}
  {@render head(blockTitle("account"), "", actions)}
  {#if o.account.authError}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{o.account.authError}</p>{/if}
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
  </dl>
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
      {#if config}
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
    <nav class="sections" aria-label={t("claudeViewSections")}>
      <ul>
        {#each sections as s (s.id)}
          {@const n = count(s.id)}
          {@const dot = tone(s.id)}
          <li>
            {#if s.id === "artifacts"}<hr />{/if}
            <button class="item" aria-current={section === s.id ? "page" : undefined} onclick={() => go(s.id)}>
              <s.icon size={16} aria-hidden="true" />
              <span class="label">{t(s.label)}</span>
              {#if dot}<span class="dot {dot === 'error' ? 'error' : 'warn'}" aria-hidden="true"></span>{/if}
              {#if n !== null}<span class="n">{n}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
    </nav>

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
        {:else if !o && claudeView.loading}
          <p class="loading" role="status"><LoaderCircle size={16} aria-hidden="true" class="spin" />{t("claudeViewLoading")}</p>
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

  .sections {
    min-height: 0;
    padding: 12px 8px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--chrome);
  }

  .sections ul {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .sections hr {
    margin: 8px 8px;
    border: 0;
    border-top: 1px solid var(--border);
  }

  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 30px;
    padding: 5px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    font-weight: 500;
    text-align: left;
  }

  .item:hover {
    background: var(--hover);
  }

  .item[aria-current] {
    background: var(--selected);
  }

  .item > :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .item[aria-current] > :global(svg) {
    color: var(--accent-ink);
  }

  .label {
    flex: 1;
    min-width: 0;
  }

  .n {
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
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

  .loading {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 40px 0;
    color: var(--muted);
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

  h4 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 16px 0 6px;
    color: var(--muted);
    font-size: var(--text-sm);
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

  .row-line,
  .row-fold > summary {
    display: flex;
    align-items: center;
    gap: 10px;
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

  .chevron-space {
    width: 14px;
    flex-shrink: 0;
  }

  .details {
    margin: 0;
    padding: 4px 12px 12px 29px;
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

  .matcher {
    margin-left: 8px;
    color: var(--muted);
    font-weight: 400;
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

  .items {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .items li {
    display: grid;
    grid-template-columns: minmax(120px, max-content) minmax(0, 1fr);
    gap: 16px;
    padding: 5px 0;
    border-top: 1px solid var(--border);
  }

  .items li:first-child {
    border-top: 0;
  }

  .items .desc {
    font-size: var(--text-sm);
    white-space: normal;
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
