<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Component } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import CircleSlash from "@lucide/svelte/icons/circle-slash";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import Import from "@lucide/svelte/icons/import";
  import Info from "@lucide/svelte/icons/info";
  import LibraryBig from "@lucide/svelte/icons/library-big";
  import Puzzle from "@lucide/svelte/icons/puzzle";
  import Scale from "@lucide/svelte/icons/scale";
  import Search from "@lucide/svelte/icons/search";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import SquareSlash from "@lucide/svelte/icons/square-slash";
  import UserRound from "@lucide/svelte/icons/user-round";
  import Workflow from "@lucide/svelte/icons/workflow";
  import Zap from "@lucide/svelte/icons/zap";
  import ConfigTransfer from "$lib/ConfigTransfer.svelte";
  import Conventions from "$lib/Conventions.svelte";
  import LibraryScout from "$lib/LibraryScout.svelte";
  import WorktreeSetup from "$lib/WorktreeSetup.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import Button from "$lib/ui/Button.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import List from "$lib/ui/List.svelte";
  import Segments from "$lib/ui/Segments.svelte";
  import SidePanel from "$lib/ui/SidePanel.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { lang, t } from "$lib/i18n";
  import { claudeView, loadOverview, show, showClaude, type Plugins } from "$lib/claude.svelte";
  import { talkCto } from "$lib/home.svelte";
  import { app, onScreen, shortPath } from "$lib/state.svelte";
  import { busy, kinds, library, matches, packageLabel, type Item, type Kind } from "$lib/library.svelte";
  import { broughtBy, cost, groupOf, ladder, merge, rolesOf, sorted, switchTo, why, type Entry, type Rung, type Write } from "$lib/library-logic";

  type Plugin = { id: string; version: string; scope: string; enabled: boolean; description: string };
  type Picked = { plugin: Plugin } | { entry: Entry };

  let entries = $state<Entry[] | null>(null);
  let query = $state("");
  let selected = $state<string | null>(null);
  let sheetOpen = $state(true);
  let search = $state<HTMLInputElement>();

  const project = $derived(app.workspace?.projects.find((p) => p.id === app.project) ?? null);
  const scoped = $derived(app.workspace?.projects.find((p) => p.id === library.scope));

  $effect(() => {
    if (!app.project && library.tab !== "items") library.tab = "items";
  });

  /** Each repository's sessions resolve their own library: the list is what they get, merged. */
  async function refresh() {
    const p = project;
    try {
      const lists = p?.repos.length
        ? await Promise.all(
            p.repos.map((r) =>
              invoke<Item[]>("library_list", { project: p.id, repo: r.path }).then((items) => ({ repo: r.name, items })),
            ),
          )
        : [{ repo: null, items: await invoke<Item[]>("library_list", { project: p?.id ?? null }) }];
      if ((p?.id ?? null) === app.project) entries = merge(lists);
    } catch (e) {
      library.error = String(e);
      entries ??= [];
    }
  }

  $effect(() => {
    void app.project;
    void library.version;
    refresh();
  });

  // Claude Code's own plugins come with the machine's overview, read once.
  $effect(() => {
    if (!claudeView.overview && !claudeView.loading) loadOverview();
  });
  const plugins = $derived<Plugin[]>(
    (claudeView.overview?.plugins.plugins ?? []).map((p) => ({
      id: show(p.id),
      version: p.version ? show(p.version) : "",
      scope: typeof p.scope === "string" ? p.scope : "user",
      enabled: !!p.enabled,
      description: typeof p.description === "string" ? p.description : "",
    })),
  );

  const kindLabel: Record<Kind, Parameters<typeof t>[0]> = {
    skill: "kindSkills",
    command: "kindCommands",
    role: "kindRoles",
    preference: "kindPreferences",
    rule: "kindRules",
    workflow: "kindWorkflows",
  };
  const oneKind: Record<Kind, Parameters<typeof t>[0]> = {
    skill: "kindSkill",
    command: "kindCommand",
    role: "kindRole",
    preference: "kindPreference",
    rule: "kindRule",
    workflow: "kindWorkflow",
  };
  const kindIcon: Record<Kind, Component> = {
    skill: Zap,
    command: SquareSlash,
    role: UserRound,
    preference: SlidersHorizontal,
    rule: Scale,
    workflow: Workflow,
  };
  const layerLabel = {
    shipped: "layerShipped",
    package: "stackLayer",
    global: "layerGlobal",
    project: "layerProject",
    repo: "layerRepo",
    run: "layerRun",
  } as const;

  const fold = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const matching = $derived((entries ?? []).filter((e) => matches(e, query)));
  const pluginsMatching = $derived(plugins.filter((p) => !query.trim() || fold(`${p.id} ${p.description}`).includes(fold(query.trim()))));
  const filters = $derived([
    { id: "all", label: t("lib2All"), count: matching.length + pluginsMatching.length },
    ...kinds
      .map((kind) => ({ id: kind, label: t(kindLabel[kind]), count: matching.filter((e) => e.kind === kind).length }))
      .filter((f) => f.count || f.id === library.kind),
    ...(plugins.length || library.kind === "plugin" ? [{ id: "plugin", label: t("lib2Plugins"), count: pluginsMatching.length }] : []),
  ]);
  const shown = $derived(library.kind === "all" ? matching : matching.filter((e) => e.kind === library.kind));
  const groups = $derived({
    on: sorted(shown.filter((e) => groupOf(e) === "on")),
    offered: sorted(shown.filter((e) => groupOf(e) === "offered")),
    off: sorted(shown.filter((e) => groupOf(e) === "off")),
  });
  const pluginRows = $derived(library.kind === "all" || library.kind === "plugin" ? pluginsMatching : []);

  const keyOf = (e: Entry) => `${e.kind}/${e.name}`;
  const pluginKey = (p: Plugin) => `plugin/${p.id}`;
  const first = $derived(groups.on[0] ?? groups.offered[0] ?? groups.off[0]);
  const picked = $derived.by<Picked | null>(() => {
    const plugin = pluginRows.find((p) => pluginKey(p) === selected);
    if (plugin) return { plugin };
    const entry = shown.find((e) => keyOf(e) === selected) ?? first;
    if (entry) return { entry };
    return pluginRows[0] ? { plugin: pluginRows[0] } : null;
  });
  const pickedKey = $derived(picked ? ("plugin" in picked ? pluginKey(picked.plugin) : keyOf(picked.entry)) : null);

  function pick(key: string) {
    selected = key;
    sheetOpen = true;
  }

  const name = (e: Pick<Item, "kind" | "name">) => (e.kind === "command" ? `/${e.name}` : e.name);
  const list = (xs: string[]) => new Intl.ListFormat(lang, { type: "conjunction" }).format(xs);
  const number = (n: number) => new Intl.NumberFormat(lang).format(n);

  function whyText(e: Entry): string {
    const w = why(e);
    const text = (() => {
      switch (w.key) {
        case "found":
          return t("lib2WhyFound", { tags: w.tags, repos: w.repos });
        case "offered":
          return t("lib2WhyOffered", { repo: w.repo });
        case "package":
          return t(w.on ? "lib2WhyPackageOn" : "lib2WhyPackageOff", { package: w.name });
        default: {
          const key = { onHere: "lib2WhyOnHere", offHere: "lib2WhyOffHere", offHereOnlyHere: "lib2WhyOffHereOnlyHere", onEverywhere: "lib2WhyOnEverywhere", offEverywhere: "lib2WhyOffEverywhere", project: "lib2WhyProject", global: "lib2WhyGlobal", always: "lib2WhyAlways", shipped: "lib2WhyShipped", run: "lib2WhyRun" } as const;
          return t(key[w.key]);
        }
      }
    })();
    // On in some of the project's repositories only: say which.
    const repos = project?.repos.length ?? 0;
    const partial = e.active && repos > 1 && e.activeIn.length < repos && w.key !== "found";
    return partial ? `${text} · ${t("lib2OnlyIn", { repos: list(e.activeIn) })}` : text;
  }

  function whyIcon(e: Entry): Component | null {
    const key = why(e).key;
    if (key === "found" || key === "always") return Sparkles;
    if (key === "offered") return FolderGit2;
    if (key.startsWith("off")) return CircleSlash;
    if (key === "onHere" || key === "onEverywhere") return Check;
    return null;
  }

  function rungText(e: Entry, r: Rung): string {
    switch (r.says) {
      case "offered":
        return t("lib2RungOffered", { repo: e.repo ?? "" });
      case "on":
        return t("lib2RungOn");
      case "off":
        return t("lib2RungOff");
      case "wrote":
        return t("lib2RungWrote");
      case "nothing":
        return t("lib2RungNothing");
      case "package":
        return t(e.active ? "lib2WhyPackageOn" : "lib2WhyPackageOff", { package: e.package ? packageLabel(e.package) : "" });
      case "notShipped":
        return t("lib2RungNotShipped");
      case "shipped": {
        const w = why({ ...e, toggles: {}, layer: "shipped" });
        if (w.key === "found") return t("lib2WhyFound", { tags: w.tags, repos: w.repos });
        return w.key === "always" ? t("lib2WhyAlways") : t("lib2RungShippedOff");
      }
    }
  }
  const rungLayer = { repo: "repo", project: "project", global: "global", base: "shipped" } as const;

  function whenText(e: Entry): string {
    switch (e.kind) {
      case "skill":
        return t("lib2WhenSkill");
      case "command":
        return t("lib2WhenCommand", { name: e.name });
      case "role":
        return t("lib2WhenRole");
      case "workflow":
        return t("lib2WhenWorkflow");
      default:
        return t("lib2WhenAlways");
    }
  }

  function costText(e: Entry): string {
    const c = cost(e);
    if (c.each === "none") return t("lib2CostNone");
    if (c.each === "turn") return t("lib2CostTurn", { always: number(c.always) });
    return t("lib2CostSession", { always: number(c.always), used: number(c.used) });
  }

  const roleItems = $derived((entries ?? []).filter((e) => e.kind === "role"));

  function users(e: Entry): string[] {
    const out: string[] = [];
    if (e.kind === "workflow") return out;
    if (e.kind === "role") out.push(t("lib2UsersTeam"));
    else if (!e.active) out.push(t("lib2UsersNobody"));
    else {
      const roles = rolesOf(e);
      out.push(roles.length ? t("lib2UsersRoles", { roles: list(roles) }) : t("lib2UsersAll"));
      if (e.activeIn.length) out.push(t("lib2UsersRepos", { repos: list(e.activeIn) }));
    }
    const brought = broughtBy(e, roleItems);
    if (brought.length) out.push(t("lib2UsersBrought", { roles: list(brought) }));
    return out;
  }

  /** Writes the switches, then reads the library again; a refusal says why and changes nothing shown. */
  async function write(e: Entry, writes: Write[]): Promise<boolean> {
    library.busy = true;
    try {
      for (const w of writes) {
        await invoke("library_toggle", { kind: e.kind, name: e.name, project: w.project, enabled: w.enabled });
      }
      return true;
    } catch (err) {
      toast(String(err), { kind: "error" });
      return false;
    } finally {
      library.busy = false;
      await refresh();
    }
  }

  async function setOn(e: Entry, on: boolean, input?: HTMLInputElement, everywhere = false) {
    const id = project?.id ?? null;
    const { apply, undo } = switchTo(e, id, on, everywhere || !id);
    if (!(await write(e, apply))) {
      if (input) input.checked = e.active;
      return;
    }
    const wide = everywhere || !id;
    const key = on ? (wide ? "lib2ToastOnEverywhere" : "lib2ToastOnHere") : wide ? "lib2ToastOffEverywhere" : "lib2ToastOffHere";
    toast(t(key, { name: name(e), project: project?.name ?? "" }), {
      undo: () => void write(e, undo).then((ok) => ok && toast(t("lib2Undone", { name: name(e) }), { kind: "info" })),
    });
  }

  async function toDefault(e: Entry) {
    const layer = project ? "project" : "global";
    const was = e.toggles[layer] ?? null;
    if (!(await write(e, [{ project: project?.id ?? null, enabled: null }]))) return;
    toast(t("lib2ToastDefault", { name: name(e) }), {
      undo: () => void write(e, [{ project: project?.id ?? null, enabled: was }]),
    });
  }

  async function setPlugin(p: Plugin, on: boolean, input: HTMLInputElement) {
    const run = (enable: boolean) =>
      invoke<{ plugins: Plugins }>("claude_plugin", {
        repo: null,
        action: { op: enable ? "enable" : "disable", plugin: p.id, scope: p.scope },
      }).then((r) => {
        if (claudeView.overview) claudeView.overview.plugins = r.plugins;
      });
    library.busy = true;
    try {
      await run(on);
      toast(t(on ? "lib2ToastOnEverywhere" : "lib2ToastOffEverywhere", { name: p.id }), { undo: () => void run(!on).catch((e) => toast(String(e), { kind: "error" })) });
    } catch (e) {
      input.checked = p.enabled;
      toast(String(e), { kind: "error" });
    } finally {
      library.busy = false;
    }
  }

  const libraryDir = $derived(
    app.workspace ? (project ? `${app.workspace.configDir}/projects/${project.id}/library` : `${app.workspace.configDir}/library`) : "",
  );

  function writeWithClaude() {
    if (!project) return;
    talkCto(t("lib2WriteDraft", { project: project.name, dir: libraryDir }));
  }

  /** The role the item names first, else the CTO; the request goes through the CTO's conversation, as `@role`. */
  const askRole = (e: Entry) => rolesOf(e)[0] ?? null;

  async function improve(e: Entry) {
    if (!project) return;
    let path = e.path;
    if (!path) {
      try {
        path = await invoke<string>("library_copy", { kind: e.kind, name: e.name, project: project.id });
      } catch (err) {
        toast(String(err), { kind: "error" });
        return;
      }
      toast(t("lib2Copied", { name: name(e), where: t("lib2CopyWhereProject") }));
      library.version++;
    }
    const role = askRole(e);
    const text = t("lib2ImproveDraft", { name: e.name, kind: t(oneKind[e.kind]).toLocaleLowerCase(lang), path });
    talkCto(role ? `@${role} ${text}` : text);
  }

  async function copyHere(e: Entry) {
    try {
      await invoke<string>("library_copy", { kind: e.kind, name: e.name, project: project?.id ?? null });
      toast(t("lib2Copied", { name: name(e), where: t(project ? "lib2CopyWhereProject" : "lib2CopyWhereGlobal") }));
    } catch (err) {
      toast(String(err), { kind: "error" });
    } finally {
      library.version++;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "/" || !onScreen("library") || library.panel || library.tab !== "items") return;
    const target = e.target as HTMLElement;
    if (target.closest("input, textarea, select, [contenteditable], .xterm")) return;
    e.preventDefault();
    search?.focus();
  }

  const tabs = $derived([
    { id: "items", label: t("lib2Items") },
    ...(app.project
      ? [
          { id: "conventions", label: t("conventions"), divider: true },
          { id: "worktrees", label: t("wtTab") },
        ]
      : []),
    { id: "scout", label: t("scoutOpen"), divider: !app.project },
  ]);
  const tab = $derived(library.panel?.kind === "scout" ? "scout" : library.panel ? null : library.tab);

  function selectTab(id: string) {
    if (id === "scout") {
      library.panel = { kind: "scout" };
      return;
    }
    // The tools of a project work on that project.
    Object.assign(library, { tab: id, panel: null, global: false });
  }
</script>

<svelte:window {onkeydown} />

<div class="library">
  <ViewHeader title={t("viewLibrary")} context={project ? t("lib2Context", { project: project.name }) : t("lib2ContextGlobal")}>
    {#if project && library.panel}
      <div class="segmented" role="group" aria-label={t("libraryScope")}>
        {#each [false, true] as global (global)}
          <button aria-pressed={library.global === global} onclick={() => (library.global = global)}>
            {global ? t("libraryGlobal") : project.name}
          </button>
        {/each}
      </div>
    {/if}
    {#snippet actions()}
      <Button variant="quiet" size="sm" icon={Import} onclick={() => (library.panel = { kind: "claude" })}>{t("claudeImport")}</Button>
      <Button
        variant="primary"
        size="sm"
        icon={Sparkles}
        disabled={!project}
        title={project ? "" : t("lib2NeedProject")}
        onclick={writeWithClaude}>{t("lib2Write")}</Button
      >
    {/snippet}
  </ViewHeader>

  <Tabs id="library" label={t("viewLibrary")} {tabs} selected={tab} onselect={selectTab}>
    {#if library.error}
      <div class="banners">
        <Banner kind="error" ondismiss={() => (library.error = "")}>{library.error}</Banner>
      </div>
    {/if}

    {#if library.panel?.kind === "scout"}
      <LibraryScout />
    {:else if library.panel}
      <ConfigTransfer panel={library.panel} />
    {:else if library.tab === "conventions" && scoped}
      {#key scoped.id}
        <Conventions project={scoped} />
      {/key}
    {:else if library.tab === "worktrees" && scoped}
      {#key scoped.id}
        <WorktreeSetup project={scoped} />
      {/key}
    {:else}
      <div class="items">
        <section class="catalog" aria-label={t("lib2Items")}>
          <div class="find">
            <label class="search">
              <Search size={15} aria-hidden="true" />
              <input
                bind:this={search}
                type="search"
                placeholder={t("lib2Search")}
                aria-label={t("lib2Search")}
                bind:value={query}
                onkeydown={(e) => e.key === "Escape" && query && (e.stopPropagation(), (query = ""))}
              />
              {#if !query}<kbd aria-hidden="true">/</kbd>{/if}
            </label>
          </div>
          <div class="filters">
            <Segments label={t("lib2Filter")} options={filters} selected={library.kind} onselect={(id) => (library.kind = id as typeof library.kind)} />
          </div>

          <div class="scroll">
            {#if !entries}
              <div class="loading"><Skeleton shape="list" rows={8} label={t("lib2Loading")} /></div>
            {:else if !shown.length && !pluginRows.length}
              {#if query.trim()}
                <EmptyState
                  icon={Search}
                  text={t("lib2NoMatch", { query: query.trim() })}
                  action={{ label: t("lib2ClearSearch"), run: () => ((query = ""), search?.focus()) }}
                />
              {:else}
                <EmptyState icon={LibraryBig} text={t("libraryEmpty")} />
              {/if}
            {:else}
              {@render group("on", t(project ? "lib2GroupOn" : "lib2GroupOnGlobal"), groups.on)}
              {@render group("offered", t("lib2GroupOffered"), groups.offered, t("lib2OfferedHint"))}
              {@render group("off", t("lib2GroupOff"), groups.off)}
              {#if pluginRows.length}
                <section class="group" aria-labelledby="library-group-plugins">
                  <h2 id="library-group-plugins">{t("lib2GroupPlugins")}<span class="n num">{pluginRows.length}</span></h2>
                  <List label={t("lib2GroupPlugins")} items={pluginRows} key={pluginKey} selected={pickedKey} onselect={(p) => pick(pluginKey(p))} row={pluginRow} />
                </section>
              {/if}
            {/if}
          </div>
        </section>

        {#if picked && sheetOpen}
          <SidePanel id="library-sheet" title={t("lib2Sheet")} bind:open={sheetOpen}>
            {#if "entry" in picked}
              {@render sheet(picked.entry)}
            {:else}
              {@render pluginSheet(picked.plugin)}
            {/if}
          </SidePanel>
        {/if}
      </div>
    {/if}
  </Tabs>
</div>

{#snippet group(id: string, title: string, list: Entry[], hint?: string)}
  {#if list.length}
    <section class="group" aria-labelledby="library-group-{id}" data-group={id}>
      <h2 id="library-group-{id}">{title}<span class="n num">{list.length}</span></h2>
      {#if hint}<p class="hint">{hint}</p>{/if}
      <List label={title} items={list} key={keyOf} selected={pickedKey} onselect={(e) => pick(keyOf(e))} row={entryRow} />
    </section>
  {/if}
{/snippet}

{#snippet entryRow(e: Entry)}
  {@const Icon = kindIcon[e.kind]}
  {@const WhyIcon = whyIcon(e)}
  <div class="entry" class:off={!e.active}>
    <span class="kind" title={t(oneKind[e.kind])}><Icon size={15} aria-hidden="true" /></span>
    <div class="text">
      <span class="title">{name(e)}</span>
      {#if e.description}<span class="desc">{e.description}</span>{/if}
      <span class="why">{#if WhyIcon}<WhyIcon size={12} aria-hidden="true" />{/if}{whyText(e)}</span>
    </div>
    <span class="ly ly-{e.layer}">{t(layerLabel[e.layer])}</span>
    <input
      class="switch"
      type="checkbox"
      role="switch"
      checked={e.active}
      disabled={library.busy}
      aria-label={t("itemActive", { name: e.name })}
      onchange={(ev) => setOn(e, ev.currentTarget.checked, ev.currentTarget)}
    />
  </div>
{/snippet}

{#snippet pluginRow(p: Plugin)}
  <div class="entry" class:off={!p.enabled}>
    <span class="kind" title={t("lib2Plugin")}><Puzzle size={15} aria-hidden="true" /></span>
    <div class="text">
      <span class="title">{p.id}</span>
      {#if p.description}<span class="desc">{p.description}</span>{/if}
      <span class="why">{t(p.scope === "user" ? "lib2WhyPluginUser" : "lib2WhyPluginRepo")}</span>
    </div>
    <span class="ly">{t("lib2Plugin")}</span>
    <input
      class="switch"
      type="checkbox"
      role="switch"
      checked={p.enabled}
      disabled={library.busy || p.scope !== "user"}
      aria-label={t("itemActive", { name: p.id })}
      onchange={(ev) => setPlugin(p, ev.currentTarget.checked, ev.currentTarget)}
    />
  </div>
{/snippet}

{#snippet sheet(e: Entry)}
  {@const Icon = kindIcon[e.kind]}
  {@const role = askRole(e)}
  <header class="head">
    <div class="tags">
      <span class="tag"><Icon size={12} aria-hidden="true" />{t(oneKind[e.kind])}</span>
      <span class="ly ly-{e.layer}">{t(layerLabel[e.layer])}</span>
    </div>
    <h3>{name(e)}</h3>
    {#if e.description}<p class="description">{e.description}</p>{/if}
  </header>

  <div class="onoff">
    <label class="state">
      <input
        class="switch"
        type="checkbox"
        role="switch"
        checked={e.active}
        disabled={library.busy}
        onchange={(ev) => setOn(e, ev.currentTarget.checked, ev.currentTarget, library.everywhere)}
      />
      <span>
        {#if project}{t(e.active ? "lib2ActiveIn" : "lib2InactiveIn", { project: project.name })}{:else}{t(e.active ? "itemOn" : "itemOff")}{/if}
      </span>
    </label>
    {#if project}
      <Segments
        label={t("lib2ReachLabel")}
        options={[
          { id: "here", label: t("lib2ReachHere") },
          { id: "everywhere", label: t("lib2ReachEverywhere") },
        ]}
        selected={library.everywhere ? "everywhere" : "here"}
        onselect={(id) => (library.everywhere = id === "everywhere")}
      />
      <p class="muted small">
        {library.everywhere ? t("lib2ReachHintEverywhere") : t("lib2ReachHintHere", { project: project.name })}
      </p>
    {/if}
    {#if e.toggles[project ? "project" : "global"] !== undefined}
      <Button variant="link" size="sm" onclick={() => toDefault(e)}>{t("lib2Default")}</Button>
    {/if}
  </div>

  <section class="block">
    <h4>{t(e.active ? "lib2WhyOnTitle" : "lib2WhyOffTitle")}</h4>
    <ol class="ladder">
      {#each ladder(e) as rung (rung.layer)}
        <li class:decides={rung.decides}>
          <span class="ly ly-{rung.layer === 'base' && e.layer === 'package' ? 'package' : rungLayer[rung.layer]}">
            {t(layerLabel[rung.layer === "base" && e.layer === "package" ? "package" : rungLayer[rung.layer]])}
          </span>
          <span class="says">{rungText(e, rung)}</span>
          {#if rung.decides}<span class="mark" title={t("lib2Decides")}><Check size={13} aria-label={t("lib2Decides")} /></span>{/if}
        </li>
      {/each}
    </ol>
    {#if e.shadows.length}
      <p class="muted small">
        {t("whyShadows", { layers: e.shadows.map((l) => t(layerLabel[l]).toLocaleLowerCase(lang)).join(", ") })}
      </p>
    {/if}
  </section>

  <dl class="facts">
    <dt>{t("lib2WhenTitle")}</dt>
    <dd>{whenText(e)}</dd>
    <dt>{t("lib2CostTitle")}</dt>
    <dd class="num">{costText(e)}</dd>
    {#if users(e).length}
      <dt>{t("lib2UsersTitle")}</dt>
      <dd>{#each users(e) as line (line)}<span class="line">{line}</span>{/each}</dd>
    {/if}
    <dt>{t("lib2SourceTitle")}</dt>
    <dd class="mono">
      {#if e.path}{shortPath(e.path)}{:else if e.package}{packageLabel(e.package)}{:else}{t("lib2SourceShipped")}{/if}
    </dd>
  </dl>

  {#if e.error}
    <p class="notice"><Info size={14} aria-hidden="true" />{t("itemError", { error: e.error })}</p>
  {/if}

  {#if e.body.trim()}
    <section class="block">
      <h4>{t("lib2Content")}</h4>
      <pre class="body">{e.body.trim()}</pre>
    </section>
  {/if}

  <div class="ai">
    <Button
      size="sm"
      icon={Sparkles}
      disabled={!project}
      title={project ? "" : t("lib2NeedProject")}
      onclick={() => improve(e)}
    >
      {role ? t("lib2AskRole", { role }) : t("lib2AskCto")}
    </Button>
    {#if !e.path && e.layer !== "repo"}
      <Button size="sm" variant="quiet" onclick={() => copyHere(e)}>{t("lib2Copy")}</Button>
    {/if}
  </div>
{/snippet}

{#snippet pluginSheet(p: Plugin)}
  <header class="head">
    <div class="tags"><span class="tag"><Puzzle size={12} aria-hidden="true" />{t("lib2Plugin")}</span></div>
    <h3>{p.id}</h3>
    {#if p.description}<p class="description">{p.description}</p>{/if}
  </header>
  <dl class="facts">
    <dt>{t("lib2WhenTitle")}</dt>
    <dd>{t("lib2WhenPlugin")}</dd>
    <dt>{t("lib2SourceTitle")}</dt>
    <dd class="mono">{p.version ? `${p.id} ${p.version}` : p.id}</dd>
  </dl>
  <p class="muted small">{t("lib2PluginScope", { scope: p.scope })}</p>
  <div class="ai">
    <Button size="sm" icon={Puzzle} onclick={() => showClaude("mcp")}>{t("lib2PluginManage")}</Button>
  </div>
{/snippet}

<style>
  .library {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }

  .items {
    position: relative;
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .catalog {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .find {
    padding: 16px 24px 10px;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    height: var(--h-lg);
    padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--panel);
    color: var(--muted);
    cursor: text;
  }

  .search:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: none;
    background: none;
    color: var(--text);
    font: inherit;
  }

  .search input::placeholder {
    color: var(--muted);
  }

  .filters {
    padding: 0 24px 12px;
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    padding: 8px 24px 32px;
    overflow-y: auto;
  }

  .loading {
    padding-top: 12px;
  }

  .group {
    margin-top: 16px;
  }

  .group h2 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0 0 8px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .n {
    color: var(--muted);
    font-weight: 500;
  }

  .hint {
    max-width: 70ch;
    margin: -2px 0 8px;
    color: var(--soft);
    font-size: var(--text-sm);
  }

  .entry {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-width: 0;
  }

  .kind {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--well);
    color: var(--soft);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .title {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .desc,
  .why {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .desc {
    color: var(--soft);
    font-size: var(--text-sm);
  }

  .why {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-top: 2px;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .why :global(svg) {
    flex-shrink: 0;
  }

  .entry.off .title,
  .entry.off .desc {
    color: var(--muted);
  }

  .ly {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 20px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--panel);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    white-space: nowrap;
  }

  .ly-global {
    color: var(--c-build);
  }

  .ly-project {
    color: var(--c-cto);
  }

  .ly-repo {
    color: var(--c-quality);
  }

  .ly-package {
    color: var(--c-specialists);
  }

  .head h3 {
    margin: 10px 0 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.015em;
    overflow-wrap: anywhere;
  }

  .tags {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .tag {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 20px;
    padding: 0 7px;
    border-radius: 4px;
    background: var(--well);
    color: var(--soft);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .description {
    margin: 4px 0 0;
    color: var(--soft);
  }

  .onoff {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .state {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 600;
    cursor: pointer;
  }

  .small {
    margin: 0;
    font-size: var(--text-sm);
  }

  .block h4 {
    margin: 0 0 6px;
    font-size: var(--text-base);
    font-weight: 600;
  }

  .ladder {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .ladder li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 0;
    border-top: 1px dashed var(--border);
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .ladder li:first-child {
    border-top: 0;
  }

  .ladder .ly {
    flex-shrink: 0;
    width: 64px;
  }

  .ladder .says {
    flex: 1;
    min-width: 0;
  }

  .ladder li.decides {
    color: var(--text);
    font-weight: 500;
  }

  .mark {
    display: grid;
    color: var(--done);
  }

  .facts {
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr);
    gap: 8px 12px;
    margin: 0;
    font-size: var(--text-sm);
  }

  .facts dt {
    color: var(--muted);
  }

  .facts dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .line {
    display: block;
  }

  .notice {
    display: flex;
    gap: 6px;
    margin: 0;
    color: var(--soft);
    font-size: var(--text-sm);
  }

  .body {
    max-height: 320px;
    margin: 0;
    padding: 12px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--well);
    color: var(--soft);
    font: 12px / 1.6 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .ai {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
</style>
