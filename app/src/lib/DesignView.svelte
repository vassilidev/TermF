<script lang="ts">
  import { untrack } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import ComponentIcon from "@lucide/svelte/icons/component";
  import Download from "@lucide/svelte/icons/download";
  import Droplet from "@lucide/svelte/icons/droplet";
  import FileDown from "@lucide/svelte/icons/file-down";
  import Info from "@lucide/svelte/icons/info";
  import Layers from "@lucide/svelte/icons/layers";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Monitor from "@lucide/svelte/icons/monitor";
  import Moon from "@lucide/svelte/icons/moon";
  import Palette from "@lucide/svelte/icons/palette";
  import Play from "@lucide/svelte/icons/play";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Ruler from "@lucide/svelte/icons/ruler";
  import Shapes from "@lucide/svelte/icons/shapes";
  import Stamp from "@lucide/svelte/icons/stamp";
  import Sun from "@lucide/svelte/icons/sun";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Type from "@lucide/svelte/icons/type";
  import Users from "@lucide/svelte/icons/users";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import SectionNav from "$lib/ui/SectionNav.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { design, openAt } from "$lib/design.svelte";
  import {
    aliased,
    describe,
    families,
    fontState,
    kit,
    latest,
    passes,
    primitive,
    ramps,
    request,
    roles,
    size,
    stackOf,
    stillThere,
    swatches,
    tally,
    tokensOf,
    tone,
    weights,
    type Component,
    type Design,
    type Font,
    type Kit,
    type Level,
    type Section,
    type Token,
    type Tone,
  } from "$lib/design-logic";
  import { currentProject, scopeRepos } from "$lib/files.svelte";
  import { talkCto } from "$lib/home.svelte";
  import { lang, t } from "$lib/i18n";
  import { app, onScreen, setProject } from "$lib/state.svelte";

  type Key = Parameters<typeof t>[0];

  let data = $state<Design | null>(null);
  /** The repository `data` is about. */
  let scanned = $state("");
  let loading = $state(false);
  let error = $state("");
  let done = $state("");
  let level = $state<Level | "all">("all");
  const chosen = new SvelteSet<string>();
  let open = $state<string | null>(null);
  let exporting = $state(false);
  /** The motion token whose preview runs. */
  let playing = $state<string | null>(null);
  /** The project's theme its previews are drawn in. */
  let canvas = $state<"light" | "dark">(window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  /** Fonts fetched from a web font service, on the user's word, for the repository read. */
  let fetched = $state<Font[]>([]);
  let fetching = $state<string | null>(null);

  const repos = $derived(scopeRepos());
  // Everything shown, and every action, is about the repository read: never the one just picked, not read yet.
  const repoName = $derived(repos.find((r) => r.path === scanned)?.name ?? "");
  // The CTO who takes a request is the project's: under every project, the one that has the repository.
  const owner = $derived(app.workspace?.projects.find((p) => p.repos.some((r) => r.path === scanned)) ?? null);
  const fmt = new Intl.NumberFormat(lang, { maximumFractionDigits: 2 });
  const number = (n: number) => fmt.format(n);
  const tt = (key: string, vars?: Record<string, string | number>) => t(key as Key, vars);

  $effect(() => {
    if (!repos.some((r) => r.path === design.repo)) design.repo = repos[0]?.path ?? "";
  });
  // Read again each time it comes on screen: the team may have fixed things meanwhile.
  $effect(() => {
    const repo = design.repo;
    if (repo && onScreen("design")) untrack(() => scan(repo));
  });

  const reads = latest();

  async function scan(repo = design.repo) {
    if (!repo) return;
    const read = reads.start();
    if (repo !== scanned) {
      data = null;
      scanned = "";
      fetched = [];
      chosen.clear();
      open = null;
    }
    loading = true;
    error = "";
    try {
      const found = await invoke<Design>("design_scan", { repo });
      if (!reads.isLast(read)) return;
      data = found;
      scanned = repo;
      const kept = stillThere(new Set(chosen), found.findings);
      for (const id of [...chosen]) if (!kept.has(id)) chosen.delete(id);
    } catch (e) {
      if (reads.isLast(read)) error = String(e);
    } finally {
      if (reads.isLast(read)) loading = false;
    }
  }

  // Fonts: the project's files, loaded under names of their own so a family named like one of TermF's never changes
  // the app, and dropped when the repository changes.
  const fonts = $derived([...fetched, ...(data?.fonts ?? []).filter((f) => !fetched.some((x) => x.family.toLowerCase() === f.family.toLowerCase()))]);
  const aliases = new SvelteMap<string, string>();
  const broken = new SvelteMap<string, string>();
  let added: FontFace[] = [];
  let loadedKey = "";
  let serial = 0;

  function bytes(src: string): ArrayBuffer {
    const text = atob(src.slice(src.indexOf(",") + 1));
    const out = new Uint8Array(text.length);
    for (let i = 0; i < text.length; i++) out[i] = text.charCodeAt(i);
    return out.buffer;
  }

  async function loadFonts(list: Font[]) {
    for (const f of added) document.fonts.delete(f);
    added = [];
    aliases.clear();
    broken.clear();
    for (const font of list) {
      if (!font.faces.length) continue;
      const alias = `termf-design-${++serial}`;
      const key = font.family.toLowerCase();
      try {
        const faces = font.faces.map((face) => new FontFace(alias, bytes(face.src), { weight: face.weight, style: face.style }));
        for (const face of faces) {
          added.push(face);
          document.fonts.add(face);
        }
        await Promise.all(faces.map((face) => face.load()));
        aliases.set(key, alias);
      } catch {
        broken.set(key, font.faces[0].file);
      }
    }
  }

  $effect(() => {
    const list = fonts;
    const key = JSON.stringify(list.map((f) => [f.family, f.faces.map((x) => `${x.file}:${x.src.length}`)]));
    if (key !== loadedKey) untrack(() => ((loadedKey = key), loadFonts(list)));
  });

  const probe = document.createElement("canvas").getContext("2d");
  const seen = new Map<string, boolean>();
  /** The machine has the family: it draws differently from each generic family alone. */
  function installed(family: string): boolean {
    if (!probe) return false;
    if (!seen.has(family)) {
      const text = "mmmmmmmmmmlli10OQW";
      seen.set(
        family,
        ["monospace", "serif", "sans-serif"].some((base) => {
          probe.font = `72px ${base}`;
          const width = probe.measureText(text).width;
          probe.font = `72px "${family.replace(/"/g, "")}", ${base}`;
          return probe.measureText(text).width !== width;
        }),
      );
    }
    return seen.get(family)!;
  }
  const loaded = (family: string) => aliases.has(family.toLowerCase());

  async function fetchFonts(url: string) {
    fetching = url;
    error = "";
    try {
      const got = await invoke<Font[]>("design_web_fonts", { url });
      fetched = [...fetched.filter((f) => !got.some((g) => g.family === f.family)), ...got];
    } catch (e) {
      error = String(e);
    } finally {
      fetching = null;
    }
  }

  const host = (url: string) => {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  };

  const findings = $derived(data?.findings ?? []);
  const counts = $derived(tally(findings));
  const shown = $derived(level === "all" ? findings : findings.filter((f) => f.level === level));
  const empty = $derived(!!data && !data.tokens.length && !data.components.length);
  const tokens = $derived(data?.tokens ?? []);
  const k = $derived(kit(tokens, canvas === "dark" && !!data?.dark));
  const role = $derived(roles(data?.fontUses ?? [], tokens));
  const stack = (s: string | undefined) => (s ? aliased(s, aliases) : undefined);

  const sections: { id: Section; label: Key; icon: typeof Droplet }[] = [
    { id: "color", label: "designColors", icon: Droplet },
    { id: "type", label: "designType", icon: Type },
    { id: "spacing", label: "designSpacingRadius", icon: Ruler },
    { id: "depth", label: "designDepth", icon: Layers },
    { id: "other", label: "designOther", icon: Shapes },
  ];
  const navItems = $derived([
    ...sections
      .filter((s) => s.id !== "other" || tokensOf(tokens, "other").length)
      .map((s) => ({ ...s, label: t(s.label), count: tokensOf(tokens, s.id).length })),
    { id: "components" as Section, label: t("designComponents"), icon: ComponentIcon, count: data?.components.length ?? 0 },
    {
      id: "issues" as Section,
      label: t("designIssues"),
      icon: TriangleAlert,
      count: findings.length,
      dot: counts.error ? ("error" as const) : counts.warn ? ("warn" as const) : null,
      apart: true,
    },
    { id: "brand" as Section, label: t("designTabBrand"), icon: Stamp, count: null },
  ]);

  /** Every family the project names: in its font tokens, then the fonts it loads. */
  const specimens = $derived.by(() => {
    const out: { family: string; stack: string; tokens: Token[] }[] = [];
    const add = (s: string, tk?: Token) => {
      const family = families(s)[0];
      if (!family) return;
      let card = out.find((c) => c.family.toLowerCase() === family.toLowerCase());
      if (!card) out.push((card = { family, stack: s, tokens: [] }));
      if (tk) card.tokens.push(tk);
    };
    for (const tk of tokensOf(tokens, "type").filter((tk) => tk.family === "font")) add(stackOf(tk.value, tokens), tk);
    for (const f of fonts) add(`"${f.family}"`);
    return out;
  });

  const textSizes = $derived(
    tokensOf(tokens, "type")
      .filter((tk) => tk.family === "text" && tk.px)
      .sort((a, b) => b.px! - a.px!),
  );
  /** The sizes of the specimen page, from the project's scale when it has one. */
  const sizes = $derived.by(() => {
    const px = textSizes.map((tk) => tk.px!);
    const base = textSizes.find((tk) => /(base|body|md|default)$/.test(tk.name))?.px ?? px.find((p) => p <= 16 && p >= 14) ?? 16;
    const h1 = px.find((p) => p <= 44 && p >= base * 1.6) ?? Math.round(base * 2.25);
    const lead = px.find((p) => p > base && p <= base * 1.4) ?? Math.round(base * 1.25);
    return { h1, lead, base };
  });

  const looks = ["variant", "kind", "intent", "appearance", "tone", "color", "severity", "status"];
  const prims = $derived((data?.components ?? []).filter((c) => primitive(c.name)).slice(0, 12));
  const others = $derived((data?.components ?? []).filter((c) => !primitive(c.name)));
  const variantsOf = (c: Component) => c.variants.find((v) => looks.includes(v.prop))?.values.slice(0, 4) ?? [];
  const toneColor = (k: Kit, look: Tone) =>
    look === "danger" ? k.danger : look === "success" ? k.success : look === "warning" ? k.warning : look === "info" ? k.info : k.accent;

  function toggle(id: string) {
    if (chosen.has(id)) chosen.delete(id);
    else chosen.add(id);
  }

  const allShown = $derived(shown.length > 0 && shown.every((f) => chosen.has(f.id)));
  function chooseAll() {
    if (allShown) for (const f of shown) chosen.delete(f.id);
    else for (const f of shown) chosen.add(f.id);
  }

  /** Writes the request in the CTO's message box, on the Home of the repository's project; nothing is sent. */
  function toCto(text: string) {
    if (!owner) return;
    if (app.project !== owner.id) setProject(owner.id);
    talkCto(text);
  }

  function fix() {
    toCto(request(findings.filter((f) => chosen.has(f.id)), repoName, tt, number));
  }

  async function exportPdf() {
    const repo = scanned;
    const path = await save({
      defaultPath: `${repoName || "brand"}-guidelines.pdf`,
      filters: [{ name: t("designPdfFilter"), extensions: ["pdf"] }],
    });
    if (!path || !repo) return;
    exporting = true;
    error = "";
    try {
      await invoke("design_pdf", { repo, path });
      done = t("designPdfDone", { path });
    } catch (e) {
      error = String(e);
    } finally {
      exporting = false;
    }
  }

  function uses(n: number): string {
    return n === 0 ? t("designUsesNone") : n === 1 ? t("designUsesOne") : t("designUsesMany", { n });
  }

  function play(name: string) {
    playing = playing === name ? null : name;
  }

  const levelIcon = { error: CircleAlert, warn: TriangleAlert, info: Info };
  const levelLabel: Record<Level, Key> = { error: "designLevelError", warn: "designLevelWarn", info: "designLevelInfo" };
  const filters: [Level | "all", Key][] = [
    ["all", "designFilterAll"],
    ["error", "designErrors"],
    ["warn", "designWarns"],
    ["info", "designInfos"],
  ];
  const missingLabel: Record<string, Key> = {
    favicon: "designMissingFavicon",
    appleTouch: "designMissingAppleTouch",
    og: "designMissingOg",
    appIcon: "designMissingAppIcon",
  };
  const roleLabel: Record<string, Key> = { heading: "designRoleHeading", body: "designRoleBody", code: "designRoleCode" };
  const svg = (text: string) => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(text)}`;
  const brandColors = $derived(tokens.filter((tk) => tk.family === "color" && !tk.alias && tk.color));
  const titles: Record<string, Key> = {
    color: "designColors",
    type: "designType",
    spacing: "designSpacingRadius",
    depth: "designDepth",
    other: "designOther",
    components: "designComponents",
    issues: "designIssues",
    brand: "designTabBrand",
  };
</script>

{#snippet source(tk: Token)}
  <button class="src mono" onclick={() => openAt(scanned, tk.file, tk.line)} title={t("designOpenAt", { file: tk.file, line: tk.line })}>
    {tk.file}:{tk.line}
  </button>
{/snippet}

{#snippet meta(tk: Token)}
  <div class="meta-line">
    {@render source(tk)}
    <span class="uses" class:none={tk.uses === 0}>{uses(tk.uses)}</span>
    {#if tk.alias}<span class="badge">{t("designAlias")}</span>{/if}
    {#if tk.generated}<span class="badge">{t("designGenerated")}</span>{/if}
  </div>
{/snippet}

{#snippet colorChip(tk: Token)}
  <div class="chip">
    <span style:background={tk.color}></span>
    {#if data?.dark}<span style:background={tk.darkColor ?? tk.color}></span>{/if}
  </div>
{/snippet}

{#snippet themeSwitch()}
  {#if data?.dark}
    <div class="segmented" role="radiogroup" aria-label={t("designCanvas")}>
      <button role="radio" aria-checked={canvas === "light"} onclick={() => (canvas = "light")}><Sun size={13} aria-hidden="true" />{t("designLight")}</button>
      <button role="radio" aria-checked={canvas === "dark"} onclick={() => (canvas = "dark")}><Moon size={13} aria-hidden="true" />{t("designDark")}</button>
    </div>
  {/if}
{/snippet}

{#snippet tokenRows(list: Token[], kind: string)}
  <ul class="rows">
    {#each list as tk (tk.name)}
      <li class="token-row">
        <span class="viz">
          {#if kind === "spacing" && tk.px !== null}
            <span class="bar" style:width="{Math.min(tk.px, 120)}px"></span>
          {:else if kind === "radius"}
            <span class="corner" style:border-radius={tk.px !== null ? `${tk.px}px` : tk.value}></span>
          {:else if kind === "motion"}
            <button class="icon-btn sm" onclick={() => play(tk.name)} aria-label={t("designPlay")} aria-pressed={playing === tk.name}>
              <Play size={13} aria-hidden="true" />
            </button>
            <span class="track">
              <span
                class="dot"
                class:moved={playing === tk.name}
                style:transition-duration={/m?s$/.test(tk.value.trim()) ? tk.value : "600ms"}
                style:transition-timing-function={/m?s$/.test(tk.value.trim()) ? "ease" : tk.value}
              ></span>
            </span>
          {/if}
        </span>
        <div class="row-meta">
          <span class="name mono">{tk.name}</span>
          <span class="value mono">{tk.value}</span>
          {@render meta(tk)}
        </div>
      </li>
    {/each}
  </ul>
{/snippet}

{#snippet preview(c: Component)}
  {@const kind = primitive(c.name)}
  {@const values = variantsOf(c)}
  <div class="stage" style:background={k.bg} style:color={k.text} style:font-family={stack(role.body?.stack)}>
    {#if kind === "button"}
      <div class="p-row">
        {#each values.length ? values : [""] as value, i (i)}
          {@const look = tone(value, i === 0)}
          {@const ink = toneColor(k, look)}
          <span
            class="p-btn"
            style:border-radius={k.radius}
            style:background={look === "outline" || look === "ghost" ? "transparent" : look === "soft" ? k.soft : ink}
            style:color={look === "outline" ? k.text : look === "soft" ? k.onSoft : look === "ghost" ? k.accent : look === "accent" ? k.onAccent : "#ffffff"}
            style:border-color={look === "outline" ? k.border : look === "ghost" || look === "soft" ? "transparent" : ink}>{value || t("designPreviewButton")}</span
          >
        {/each}
      </div>
    {:else if kind === "field"}
      <div class="p-field">
        <span class="p-label">{t("designPreviewLabel")}</span>
        <span class="p-input" style:border-color={k.border} style:border-radius={k.radius} style:background={k.surface} style:color={k.muted}>{t("designPreviewPlaceholder")}</span>
      </div>
    {:else if kind === "card"}
      <div class="p-card" style:background={k.surface} style:border-color={k.border} style:border-radius={k.radiusLarge} style:box-shadow={k.shadow}>
        <strong style:font-family={stack(role.heading?.stack)}>{t("designPreviewCardTitle")}</strong>
        <span style:color={k.muted}>{t("designPreviewCardText")}</span>
      </div>
    {:else if kind === "badge"}
      <div class="p-row">
        {#each values.length ? values : [""] as value, i (i)}
          {@const ink = toneColor(k, tone(value, i === 0))}
          <span class="p-badge" style:border-radius={k.radius} style:background="color-mix(in srgb, {ink} 14%, {k.bg})" style:color="color-mix(in srgb, {ink} 80%, {k.text})"
            >{value || t("designPreviewBadge")}</span
          >
        {/each}
      </div>
    {:else if kind === "alert"}
      <div class="p-stack">
        {#each (values.length ? values : ["info"]).slice(0, 2) as value, i (i)}
          {@const ink = toneColor(k, tone(value, false) === "soft" ? "info" : tone(value, false))}
          <div class="p-alert" style:border-radius={k.radius} style:background="color-mix(in srgb, {ink} 10%, {k.bg})" style:border-color="color-mix(in srgb, {ink} 40%, {k.bg})">
            <strong style:color="color-mix(in srgb, {ink} 75%, {k.text})">{t("designPreviewAlertTitle")}</strong>
            <span>{t("designPreviewAlertText")}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

<div class="design">
  <ViewHeader title={t("designView")} context={currentProject()?.name ?? t("ticketsAll")}>
    {#if repos.length > 1}
      <label class="labelled inline">
        <span>{t("filesRepo")}</span>
        <select class="field" bind:value={design.repo}>
          {#each repos as r (r.path)}<option value={r.path}>{r.name}</option>{/each}
        </select>
      </label>
    {/if}
    {#snippet actions()}
      <button class="btn sm" onclick={() => scan()} disabled={!design.repo || loading}>
        <RefreshCw size={14} aria-hidden="true" class={loading ? "spin" : ""} />{t("filesRefresh")}
      </button>
    {/snippet}
  </ViewHeader>

  {#if error || done}
    <div class="banners">
      {#if error}<Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>{/if}
      {#if done}<Banner kind="ok" ondismiss={() => (done = "")}>{done}</Banner>{/if}
    </div>
  {/if}

  {#if !repos.length}
    <EmptyState icon={Palette} title={t("designNoRepo")} text={t("designNoRepoHelp")} />
  {:else if !data}
    {#if loading}
      <div class="body loading">
        <div class="nav-bones"><Skeleton shape="lines" rows={8} label={t("designScanning")} /></div>
        <div class="page">
          <div class="bone-head"><Skeleton shape="block" height="28px" /></div>
          <Skeleton shape="cards" rows={8} />
        </div>
      </div>
    {/if}
  {:else}
    <div class="body">
      <SectionNav items={navItems} current={design.section} label={t("designSections")} onselect={(id) => (design.section = id)} />
      <div class="page">
        <header class="head">
          <h2>{t(titles[design.section])}</h2>
          <span class="grow"></span>
          {#if design.section === "type" || design.section === "components" || design.section === "depth"}
            {@render themeSwitch()}
          {:else if design.section === "color" && data.dark}
            <span class="legend muted"><Sun size={13} aria-hidden="true" />{t("designLight")}<Moon size={13} aria-hidden="true" />{t("designDark")}</span>
          {:else if design.section === "issues" && findings.length}
            <div class="segmented" role="radiogroup" aria-label={t("designFilter")}>
              {#each filters as [id, label] (id)}
                <button role="radio" aria-checked={level === id} onclick={() => (level = id)}>
                  {t(label)} <span class="n">{id === "all" ? findings.length : counts[id]}</span>
                </button>
              {/each}
            </div>
          {:else if design.section === "brand" && data.brand}
            <button class="btn sm primary" onclick={exportPdf} disabled={!data.brand.printable || exporting} title={data.brand.printable ? undefined : t("designNoGuidelines")}>
              {#if exporting}<LoaderCircle size={14} class="spin" aria-hidden="true" />{:else}<FileDown size={14} aria-hidden="true" />{/if}
              {t("designPdf")}
            </button>
          {/if}
        </header>

        {#if empty && design.section !== "brand" && design.section !== "issues"}
          <EmptyState icon={Palette} title={t("designEmptyTitle")} text={t("designEmptyText")}>
            <div class="how">
              <p>{t("designEmptyHow")}</p>
              <pre class="example">{t("designEmptyExample")}</pre>
              <p>{t("designEmptyOther")}</p>
            </div>
            <button class="btn primary" onclick={() => toCto(t("designAskBrandRequest", { repo: repoName }))} disabled={!owner}>
              <Users size={14} aria-hidden="true" />{t("designAskBrand")}
            </button>
          </EmptyState>
        {:else if design.section === "color"}
          {@const list = tokensOf(tokens, "color")}
          {#if !list.length}
            <p class="muted">{t("designNoTokens")}</p>
          {:else}
            {#each ramps(list) as group (group.name)}
              {#if group.tokens.length > 1}
                <div class="ramp">
                  <div class="ramp-name mono">{group.name}</div>
                  <div class="ramp-strip">
                    {#each group.tokens as tk (tk.name)}
                      <button class="step" onclick={() => openAt(scanned, tk.file, tk.line)} title={`${tk.name} · ${tk.value}`}>
                        <span class="step-chip" style:background={canvas === "dark" && data.dark ? (tk.darkColor ?? tk.color) : tk.color}></span>
                        <span class="step-label mono">{tk.name.slice(group.name.length).replace(/^[-.]/, "")}</span>
                      </button>
                    {/each}
                  </div>
                </div>
              {/if}
            {/each}
            <div class="swatches">
              {#each ramps(list).filter((g) => g.tokens.length === 1) as group (group.name)}
                {@const tk = group.tokens[0]}
                <div class="swatch">
                  {@render colorChip(tk)}
                  <div class="name mono" title={tk.path ?? tk.name}>{tk.name}</div>
                  <div class="value mono">{tk.value}{#if tk.dark}{" · "}{tk.dark.value}{/if}</div>
                  {@render meta(tk)}
                </div>
              {/each}
            </div>
            {#if data.pairs.length}
              <h3>{t("designPairs")}</h3>
              <p class="hint">{t("designPairsHint")}</p>
              <table class="pairs">
                <tbody>
                  {#each data.pairs.slice(0, 60) as p, i (i)}
                    <tr>
                      <td><span class="sample" style:color={p.fgColor} style:background={p.bgColor}>{t("designSampleShort")}</span></td>
                      <td class="mono">{p.fg} <span class="muted">/</span> {p.bg}</td>
                      <td>
                        {#if p.theme === "dark"}<Moon size={13} aria-label={t("designDark")} />{:else}<Sun size={13} aria-label={t("designLight")} />{/if}
                      </td>
                      <td class="ratio">{number(p.ratio)}:1</td>
                      <td>
                        <span class="badge" class:fail={!passes(p.ratio)}>{passes(p.ratio) ? t("designAA") : t("designUnderAA")}</span>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          {/if}
        {:else if design.section === "type"}
          <h3 class="first">{t("designFamilies")}</h3>
          {#if !specimens.length}
            <p class="muted">{t("designFontNone")}</p>
          {/if}
          <div class="specimens">
            {#each specimens as sp (sp.family)}
              {@const st = fontState(sp.stack, fonts, loaded, installed)}
              {@const face = stack(sp.stack)}
              {@const fallback = st.shown || t("designFontDefault")}
              <article class="specimen">
                <div class="proof" style:background={k.bg} style:color={k.text} style:font-family={face}>
                  <span class="aa" aria-hidden="true">{t("designSampleShort")}</span>
                  <div class="glyphs">
                    <span class="family">{sp.family}</span>
                    <span class="alphabet">{t("designAlphabet")}</span>
                    <span class="weights">
                      {#each weights(st.font, tokens) as w (w)}<span style:font-weight={w}>{t("designSampleShort")} {w}</span>{/each}
                    </span>
                  </div>
                </div>
                <div class="status" class:warn={st.state === "missing" || st.state === "installed"}>
                  {#if st.state === "loaded"}
                    <CircleCheck size={14} aria-hidden="true" />
                    <span>
                      {#if st.font?.origin === "repo"}{t("designFontRepo", { file: st.font.faces.map((f) => f.file).join(", ") })}
                      {:else if st.font?.origin === "package"}{t("designFontPackage", { file: st.font.faces[0].file })}
                      {:else if st.font?.url}{t("designFontFetched", { host: host(st.font.url) })}{/if}
                    </span>
                  {:else if st.state === "service" && st.font?.url}
                    <Download size={14} aria-hidden="true" />
                    <span>{t("designFontService", { host: host(st.font.url), shown: fallback })}</span>
                    <button class="btn sm" onclick={() => fetchFonts(st.font!.url!)} disabled={!!fetching} title={t("designFontLoadHint", { host: host(st.font.url) })}>
                      {#if fetching === st.font.url}<LoaderCircle size={14} class="spin" aria-hidden="true" />{/if}
                      {t("designFontLoad", { host: host(st.font.url) })}
                    </button>
                  {:else if st.state === "system"}
                    <Monitor size={14} aria-hidden="true" /><span>{t("designFontSystem")}</span>
                  {:else if st.state === "installed"}
                    <TriangleAlert size={14} aria-hidden="true" /><span>{t("designFontInstalled")}</span>
                  {:else if broken.has(st.family.toLowerCase())}
                    <TriangleAlert size={14} aria-hidden="true" /><span>{t("designFontBroken", { file: broken.get(st.family.toLowerCase()) ?? "", shown: fallback })}</span>
                  {:else}
                    <TriangleAlert size={14} aria-hidden="true" /><span>{t("designFontMissing", { shown: fallback })}</span>
                  {/if}
                </div>
                {#if st.state === "service" && st.font?.url}<p class="hint small">{t("designFontLoadHint", { host: host(st.font.url) })}</p>{/if}
                {#if sp.tokens.length || st.font}
                  <div class="meta-line">
                    {#each sp.tokens as tk (tk.name)}<span class="name mono">{tk.name}</span>{@render source(tk)}{/each}
                    {#if !sp.tokens.length && st.font}
                      <button class="src mono" onclick={() => openAt(scanned, st.font!.file, st.font!.line)}>{st.font.file}:{st.font.line}</button>
                    {/if}
                  </div>
                {/if}
              </article>
            {/each}
          </div>

          {#if role.body || role.heading}
            <h3>{t("designPairing")}</h3>
            <article class="pairing" style:background={k.bg} style:color={k.text} style:font-family={stack(role.body?.stack)} style:font-size="{sizes.base}px">
              <h1 style:font-family={stack(role.heading?.stack)} style:font-size="{sizes.h1}px">{t("designSpecTitle")}</h1>
              <p class="lead" style:font-size="{sizes.lead}px" style:color={k.muted}>{t("designSpecLead")}</p>
              <p>{t("designSpecBody")}</p>
              <ul>
                <li>{t("designSpecItem1")}</li>
                <li>{t("designSpecItem2")}</li>
                <li>{t("designSpecItem3")}</li>
              </ul>
              <p class="numbers">{t("designSpecNumbers")}</p>
              <pre style:font-family={stack(role.code?.stack) ?? "ui-monospace, monospace"} style:border-radius={k.radius} style:background="color-mix(in srgb, {k.text} 6%, {k.bg})"><code
                  >{t("designSpecCode")}</code
                ></pre>
            </article>
            <p class="caption">
              {#each Object.entries(role) as [name, r] (name)}
                {#if r}<span>{t("designRoleFrom", { role: t(roleLabel[name]), family: families(r.stack)[0] ?? "", from: r.from })}</span>{/if}
              {/each}
            </p>
          {/if}

          {#if textSizes.length}
            <h3>{t("designScale")}</h3>
            <ol class="scale" style:background={k.bg} style:color={k.text} style:font-family={stack(role.body?.stack)}>
              {#each textSizes as tk (tk.name)}
                <li>
                  <button class="scale-meta" onclick={() => openAt(scanned, tk.file, tk.line)} title={t("designOpenAt", { file: tk.file, line: tk.line })}>
                    <span class="mono">{tk.name}</span>
                    <span class="mono">{tk.value}{tk.value.endsWith("px") ? "" : ` · ${number(tk.px!)}px`}</span>
                  </button>
                  <span class="scale-sample" style:font-size="{Math.min(tk.px!, 72)}px">{t("designSample")}</span>
                </li>
              {/each}
            </ol>
          {/if}

          {#if tokensOf(tokens, "type").length}
            <h3>{t("designTokens")}</h3>
            {@render tokenRows(tokensOf(tokens, "type"), "type")}
          {/if}
        {:else if design.section === "spacing"}
          {@const space = tokens.filter((tk) => tk.family === "spacing")}
          {@const radii = tokens.filter((tk) => tk.family === "radius")}
          {#if !space.length && !radii.length}<p class="muted">{t("designNoTokens")}</p>{/if}
          {#if space.length}
            <h3 class="first">{t("designSpacing")}</h3>
            {@render tokenRows(space, "spacing")}
          {/if}
          {#if radii.length}
            <h3 class:first={!space.length}>{t("designRadius")}</h3>
            {@render tokenRows(radii, "radius")}
          {/if}
        {:else if design.section === "depth"}
          {@const shadows = tokens.filter((tk) => tk.family === "shadow")}
          {@const motion = tokens.filter((tk) => tk.family === "motion")}
          {#if !shadows.length && !motion.length}<p class="muted">{t("designNoTokens")}</p>{/if}
          {#if shadows.length}
            <h3 class="first">{t("designShadows")}</h3>
            <div class="lifts" style:background={k.bg}>
              {#each shadows as tk (tk.name)}
                <div class="lift-cell">
                  <span class="lift" style:box-shadow={tk.value} style:background={k.surface} style:border-radius={k.radiusLarge}></span>
                  <span class="name mono" style:color={k.text}>{tk.name}</span>
                </div>
              {/each}
            </div>
            {@render tokenRows(shadows, "shadow")}
          {/if}
          {#if motion.length}
            <h3 class:first={!shadows.length}>{t("designMotion")}</h3>
            {@render tokenRows(motion, "motion")}
          {/if}
        {:else if design.section === "other"}
          {@render tokenRows(tokensOf(tokens, "other"), "other")}
        {:else if design.section === "components"}
          {#if data.storybook}<p class="notice"><Info size={14} aria-hidden="true" />{t("designStorybook")}</p>{/if}
          {#if !data.components.length}
            <p class="muted">{t("designNoComponents")}</p>
          {/if}
          {#if prims.length}
            <h3 class="first">{t("designPrimitives")}</h3>
            <p class="hint">{t("designRebuiltHint")}</p>
            <ul class="gallery">
              {#each prims as c (c.file)}
                <li class="comp">
                  {@render preview(c)}
                  <div class="c-body">
                    <div class="c-head">
                      <strong>{c.name}</strong>
                      <span class="badge">{c.framework}</span>
                      <span class="grow"></span>
                      <span class="uses" class:none={c.uses === 0}>{uses(c.uses)}</span>
                    </div>
                    <button class="src mono" onclick={() => openAt(scanned, c.file, 1)}>{c.file}</button>
                    {#each c.variants as v (v.prop)}
                      <div class="variants">
                        <span class="muted">{v.prop}</span>
                        {#each v.values as value (value)}<span class="badge">{value}</span>{/each}
                      </div>
                    {/each}
                    {#if c.usedIn.length}<p class="small muted">{t("designUsedIn", { files: c.usedIn.slice(0, 3).join(", ") })}</p>{/if}
                    {#if c.stories.length}<p class="small muted">{t("designStories", { stories: c.stories.join(", ") })}</p>{/if}
                    <span class="rebuilt">{t("designRebuilt")}</span>
                  </div>
                </li>
              {/each}
            </ul>
          {/if}
          {#if others.length}
            <h3 class:first={!prims.length}>{prims.length ? t("designOtherComponents") : t("designComponents")}</h3>
            <table class="others">
              <tbody>
                {#each others as c (c.file)}
                  <tr>
                    <td><strong>{c.name}</strong></td>
                    <td><span class="badge">{c.framework}</span></td>
                    <td class="vals">
                      {#each c.variants as v (v.prop)}<span class="muted">{v.prop}</span>{#each v.values as value (value)}<span class="badge">{value}</span>{/each}{/each}
                      {#if c.stories.length}<span class="muted" title={c.stories.join("\n")}>{t("designStories", { stories: c.stories.length })}</span>{/if}
                    </td>
                    <td><span class="uses" class:none={c.uses === 0} title={c.usedIn.join("\n")}>{uses(c.uses)}</span></td>
                    <td><button class="src mono" onclick={() => openAt(scanned, c.file, 1)}>{c.file}</button></td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        {:else if design.section === "issues"}
          <p class="lede">
            {t("designStats", { tokens: data.tokens.length, components: data.components.length, files: data.files })}
            {#if data.sources.length}· {t("designSources", { files: data.sources.join(", ") })}{/if}
          </p>
          {#each data.notes as note, i (i)}
            <p class="notice">
              <Info size={14} aria-hidden="true" />
              {#if note.kind === "unread"}{t("designNoteUnread", { file: note.file ?? "", detail: note.detail ?? "" })}
              {:else if note.kind === "truncated"}{t("designNoteTruncated", { detail: note.detail ?? "" })}
              {:else if note.kind === "large"}{t("designNoteLarge", { detail: note.detail ?? "" })}
              {:else if note.kind === "packages"}{t("designNotePackages", { detail: note.detail ?? "" })}
              {:else}{t("designNotePublished")}{/if}
            </p>
          {/each}
          {#if !findings.length}
            <EmptyState icon={Palette} title={t("designNoIssues")} />
          {:else}
            <div class="bulk">
              <label class="check">
                <input type="checkbox" checked={allShown} onchange={chooseAll} />
                {t("designChooseAll")}
              </label>
              <span class="muted">{t("designChosen", { n: chosen.size })}</span>
              <span class="grow"></span>
              {#if !owner}<span class="muted hint">{t("designFixNoProject")}</span>{/if}
              <button class="btn sm primary" onclick={fix} disabled={!chosen.size || !owner} title={t("designFixHint")}>
                <Users size={14} aria-hidden="true" />{t("designFix")}
              </button>
            </div>
            <ul class="findings">
              {#each shown as f (f.id)}
                {@const LevelIcon = levelIcon[f.level]}
                {@const sentence = describe(f, tt, number)}
                <li class="finding {f.level}" class:open={open === f.id}>
                  <input type="checkbox" checked={chosen.has(f.id)} onchange={() => toggle(f.id)} aria-label={sentence} />
                  <span class="level" title={t(levelLabel[f.level])}>
                    <LevelIcon size={15} aria-hidden="true" /><span class="sr-only">{t(levelLabel[f.level])}</span>
                  </span>
                  <div class="what">
                    <button class="line" aria-expanded={open === f.id} onclick={() => (open = open === f.id ? null : f.id)}>
                      {#each swatches(f, data.tokens) as c, i (i)}<span class="sw" style:background={c}></span>{/each}
                      <span class="text">{sentence}</span>
                      {#if f.count}
                        <span class="where">
                          {f.count === 1 ? `${f.at[0].file}:${f.at[0].line}` : t("designPlaces", { n: f.count })}
                        </span>
                      {/if}
                    </button>
                    {#if open === f.id}
                      <ul class="places">
                        {#each f.at as a, i (i)}
                          <li>
                            <button class="place" onclick={() => openAt(scanned, a.file, a.line)} title={t("designOpenAt", { file: a.file, line: a.line })}>
                              <span class="mono loc">{a.file}:{a.line}</span>
                              {#if a.text}<code>{a.text}</code>{/if}
                            </button>
                          </li>
                        {/each}
                        {#if f.count > f.at.length}<li class="muted more">{t("designMore", { n: f.count - f.at.length })}</li>{/if}
                      </ul>
                    {/if}
                  </div>
                </li>
              {/each}
            </ul>
          {/if}
        {:else}
          {@const brand = data.brand}
          {#if !brand}
            <EmptyState icon={Stamp} title={t("designBrandEmptyTitle")} text={t("designBrandEmptyText")}>
              <button class="btn primary" onclick={() => toCto(t("designBrandRequest", { repo: repoName }))} disabled={!owner}>
                <Users size={14} aria-hidden="true" />{t("designBrandAsk")}
              </button>
            </EmptyState>
          {:else}
            {#if brand.pdf}<p class="hint">{t("designPdfInRepo")}</p>{/if}

            <section>
              <h3 class="first">{t("designLogo")}</h3>
              {#if !brand.logos.length}
                <p class="muted">{t("designNoLogo")}</p>
              {:else}
                <div class="logos">
                  {#each brand.logos as logo (logo.file)}
                    <figure>
                      <div class="grounds">
                        <div class="ground light" title={t("designOnLight")}><img src={svg(logo.svg)} alt={logo.file} /></div>
                        <div class="ground dark" title={t("designOnDark")}><img src={svg(logo.svg)} alt="" /></div>
                      </div>
                      <figcaption class="mono">{logo.file.replace(/^brand\/logo\//, "")}</figcaption>
                    </figure>
                  {/each}
                </div>
              {/if}
            </section>

            {#if brandColors.length}
              <section>
                <h3>{t("designColors")}</h3>
                <div class="swatches compact">
                  {#each brandColors as tk (tk.name)}
                    <div class="swatch">
                      {@render colorChip(tk)}
                      <div class="name mono">{tk.name}</div>
                      <div class="value mono">{tk.color}{#if data.dark && tk.darkColor && tk.darkColor !== tk.color}{" · "}{tk.darkColor}{/if}</div>
                    </div>
                  {/each}
                </div>
              </section>
            {/if}

            {#if specimens.length}
              <section>
                <h3>{t("designType")}</h3>
                <ul class="rows">
                  {#each specimens as sp (sp.family)}
                    <li class="type-row">
                      <span class="type-sample" style:font-family={stack(sp.stack)}>{t("designSample")}</span>
                      <div class="row-meta"><span class="name mono">{sp.family}</span><span class="value mono">{sp.stack}</span></div>
                    </li>
                  {/each}
                </ul>
              </section>
            {/if}

            <section>
              <h3>{t("designExports")}</h3>
              {#if brand.missing.length}
                <p class="notice warn">
                  <TriangleAlert size={14} aria-hidden="true" />
                  {t("designMissing", { list: brand.missing.map((m) => t(missingLabel[m])).join(", ") })}
                </p>
              {/if}
              {#if !brand.exports.length}
                <p class="muted">{t("designNoExports")}</p>
              {:else}
                <table class="exports">
                  <tbody>
                    {#each brand.exports as e (e.file)}
                      <tr>
                        <td class="mono">{e.file.replace(/^brand\/exports\//, "")}</td>
                        <td class="muted">{e.width && e.height ? `${e.width} × ${e.height}` : ""}</td>
                        <td class="muted num">{size(e.size, lang)}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              {/if}
            </section>

            {#if brand.summary}
              <section>
                <h3>{t("designBrandFile")}</h3>
                <pre class="summary">{brand.summary}</pre>
              </section>
            {/if}

            <section>
              <h3>{t("designGuidelines")}</h3>
              {#if brand.guidelines}
                <p class="hint">{t("designPreviewNote")}</p>
                <iframe class="guidelines" title={t("designGuidelines")} sandbox="" srcdoc={brand.guidelines}></iframe>
              {:else if brand.printable}
                <p class="muted">{t("designGuidelinesTooBig")}</p>
              {:else}
                <p class="muted">{t("designNoGuidelines")}</p>
              {/if}
            </section>
          {/if}
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .design {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--panel);
  }

  .inline {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }

  .inline select {
    width: 200px;
  }

  .body {
    display: grid;
    flex: 1;
    grid-template: minmax(0, 1fr) / 208px minmax(0, 1fr);
    min-height: 0;
  }

  .nav-bones {
    padding: 16px 14px;
    border-right: 1px solid var(--border);
  }

  .bone-head {
    width: 220px;
    height: 28px;
    margin-bottom: 20px;
  }

  .page {
    min-width: 0;
    min-height: 0;
    padding: 20px 28px 48px;
    overflow-y: auto;
  }

  .page > :global(:not(.empty)) {
    max-width: 980px;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    min-height: 28px;
    margin-bottom: 12px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 32px 0 10px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  h3.first {
    margin-top: 4px;
  }

  .lede,
  .hint {
    margin: 0 0 12px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .small {
    margin: 0;
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }

  .notice {
    margin-bottom: 8px;
  }

  .grow {
    flex: 1;
  }

  .n {
    margin-left: 2px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .segmented button {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .legend {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-sm);
  }

  .legend :global(svg:last-of-type) {
    margin-left: 8px;
  }

  /* Empty */
  .how {
    width: 100%;
    margin: 4px 0 16px;
    font-size: var(--text-sm);
    text-align: left;
  }

  .how p {
    margin: 0 0 8px;
  }

  .example {
    margin: 0 0 8px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--hover);
    color: var(--text);
    font: var(--text-sm) / 1.6 var(--font-mono);
  }

  /* Findings */
  .bulk {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 500;
  }

  .findings {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .finding {
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr);
    align-items: start;
    gap: 0 10px;
    padding: 7px 0;
    border-bottom: 1px solid var(--border);
  }

  .finding > input {
    margin: 5px 0 0;
  }

  .level {
    display: inline-flex;
    margin-top: 4px;
    color: var(--muted);
  }

  .finding.error .level {
    color: var(--danger);
  }

  .finding.warn .level {
    color: var(--warning);
  }

  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 26px;
    padding: 2px 6px;
    margin-left: -6px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
  }

  .line:hover {
    background: var(--hover);
  }

  .text {
    flex: 1;
    min-width: 0;
    overflow-wrap: break-word;
  }

  .where {
    flex-shrink: 0;
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .sw {
    flex-shrink: 0;
    width: 14px;
    height: 14px;
    border-radius: var(--radius-sm);
    box-shadow: inset 0 0 0 1px rgb(127 127 127 / 0.35);
  }

  .sw + .sw {
    margin-left: -4px;
  }

  .places {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 4px 0 6px;
    padding: 0;
    list-style: none;
  }

  .place {
    display: grid;
    grid-template-columns: minmax(140px, max-content) minmax(0, 1fr);
    align-items: baseline;
    gap: 12px;
    width: 100%;
    padding: 3px 6px;
    margin-left: -6px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    text-align: left;
  }

  .place:hover {
    background: var(--hover);
  }

  .loc {
    color: var(--accent-ink);
  }

  .place code {
    overflow: hidden;
    color: var(--muted);
    font: var(--text-xs) / 1.5 var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .more {
    padding: 3px 0;
    font-size: var(--text-sm);
  }

  /* Colors */
  .ramp {
    margin-bottom: 16px;
  }

  .ramp-name {
    margin-bottom: 6px;
    color: var(--text);
    font-size: var(--text-sm);
  }

  .ramp-strip {
    display: flex;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .step {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
  }

  .step-chip {
    display: block;
    height: 48px;
  }

  .step-label {
    padding: 4px 0;
    border-top: 1px solid var(--border);
    font-size: var(--text-xs);
    text-align: center;
  }

  .step:hover .step-label {
    color: var(--text);
  }

  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(176px, 1fr));
    gap: 16px 12px;
    margin-top: 8px;
  }

  .swatches.compact {
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  }

  .swatch {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  /* Translucent colors over a checkerboard, as design tools show them. */
  .chip {
    display: flex;
    height: 52px;
    margin-bottom: 6px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: repeating-conic-gradient(#d9dbe0 0 25%, #ffffff 0 50%) 0 0 / 12px 12px;
  }

  .chip span {
    flex: 1;
  }

  .name {
    overflow: hidden;
    color: var(--text);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .value {
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
    margin-top: 2px;
    font-size: var(--text-xs);
  }

  .src {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    text-align: left;
    overflow-wrap: anywhere;
  }

  .src:hover {
    text-decoration: underline;
  }

  .uses {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .uses.none {
    color: var(--warning);
  }

  .pairs,
  .exports,
  .others {
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  .pairs td,
  .exports td,
  .others td {
    padding: 6px 16px 6px 0;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }

  .sample {
    display: inline-grid;
    place-items: center;
    width: 36px;
    height: 24px;
    border-radius: var(--radius-sm);
    box-shadow: inset 0 0 0 1px rgb(127 127 127 / 0.3);
    font-weight: 600;
  }

  .ratio,
  .num {
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .badge.fail {
    border-color: var(--danger);
    color: var(--danger);
  }

  /* The project's own look, its ground, ink and fonts, framed apart from TermF's. */
  .proof,
  .pairing,
  .scale,
  .stage,
  .lifts {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  /* Typography */
  .specimens {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .specimen {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .proof {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: 8px 28px;
    padding: 20px 24px;
  }

  .aa {
    font-size: 76px;
    line-height: 1;
    letter-spacing: -0.02em;
  }

  .glyphs {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .family {
    overflow: hidden;
    font-size: 24px;
    line-height: 1.2;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .alphabet {
    font-size: 15px;
    line-height: 1.5;
    opacity: 0.82;
    overflow-wrap: anywhere;
  }

  .weights {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
    font-size: 15px;
  }

  .status {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
    color: var(--muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .status > :global(svg) {
    flex-shrink: 0;
    color: var(--done);
  }

  .status.warn,
  .status.warn > :global(svg) {
    color: var(--warning);
  }

  .pairing {
    padding: 28px 32px;
    line-height: 1.55;
  }

  .pairing h1 {
    max-width: 22ch;
    margin: 0 0 8px;
    font-weight: 700;
    line-height: 1.15;
    letter-spacing: -0.015em;
    text-wrap: balance;
  }

  .pairing p,
  .pairing ul {
    max-width: 66ch;
    margin: 0 0 12px;
  }

  .pairing .lead {
    line-height: 1.4;
  }

  .pairing ul {
    padding-left: 1.2em;
  }

  .numbers {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }

  .pairing pre {
    margin: 0;
    padding: 10px 14px;
    overflow-x: auto;
    font-size: 0.85em;
  }

  .caption {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 16px;
    margin: 8px 0 0;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .scale {
    margin: 0;
    padding: 4px 20px;
    list-style: none;
  }

  .scale li {
    display: grid;
    grid-template-columns: 168px minmax(0, 1fr);
    align-items: baseline;
    gap: 16px;
    padding: 10px 0;
  }

  .scale li + li {
    border-top: 1px solid color-mix(in srgb, currentColor 12%, transparent);
  }

  .scale-meta {
    display: flex;
    flex-direction: column;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font-size: var(--text-xs);
    text-align: left;
    opacity: 0.7;
  }

  .scale-meta:hover {
    opacity: 1;
  }

  .scale-sample {
    overflow: hidden;
    line-height: 1.2;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Token rows */
  .rows {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows > li {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }

  .type-row {
    flex-direction: column;
    align-items: flex-start !important;
    gap: 4px !important;
  }

  .type-sample {
    max-width: 100%;
    overflow: hidden;
    line-height: 1.2;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 12px;
    min-width: 0;
  }

  /* The previews share one column, so the names line up. */
  .viz {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 8px;
    width: 136px;
  }

  .viz:empty {
    display: none;
  }

  .bar {
    flex-shrink: 0;
    height: 12px;
    border-radius: 2px;
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .corner {
    flex-shrink: 0;
    width: 40px;
    height: 40px;
    border: 2px solid var(--accent);
    background: var(--accent-soft);
  }

  .lifts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 28px 20px;
    margin-bottom: 8px;
    padding: 28px 24px 20px;
  }

  .lift-cell {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }

  .lift {
    display: block;
    height: 72px;
  }

  .track {
    position: relative;
    flex-shrink: 0;
    width: 96px;
    height: 4px;
    border-radius: 2px;
    background: var(--hover);
  }

  .dot {
    position: absolute;
    top: -4px;
    left: 0;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent);
    transition-property: transform;
  }

  .dot.moved {
    transform: translateX(84px);
  }

  @media (prefers-reduced-motion: reduce) {
    .dot {
      transition: none !important;
    }
  }

  /* Components */
  .gallery {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .comp {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .stage {
    display: grid;
    place-items: center;
    min-height: 132px;
    padding: 20px;
    border-bottom-right-radius: 0;
    border-bottom-left-radius: 0;
    font-size: 14px;
  }

  .c-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
    border: 1px solid var(--border);
    border-top: 0;
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
  }

  .c-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .c-body .src {
    font-size: var(--text-xs);
  }

  .rebuilt {
    align-self: flex-start;
    color: var(--muted);
    font-size: var(--text-xs);
    font-style: italic;
  }

  .variants,
  .vals {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    font-size: var(--text-sm);
  }

  .variants .muted,
  .vals .muted {
    margin-right: 2px;
  }

  .p-row {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
  }

  .p-btn {
    display: inline-flex;
    align-items: center;
    height: 36px;
    padding: 0 16px;
    border: 1px solid;
    font-weight: 500;
    white-space: nowrap;
  }

  .p-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: min(100%, 240px);
  }

  .p-label {
    font-weight: 500;
  }

  .p-input {
    display: flex;
    align-items: center;
    height: 38px;
    padding: 0 12px;
    border: 1px solid;
  }

  .p-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: min(100%, 260px);
    padding: 16px;
    border: 1px solid;
  }

  .p-card strong {
    font-size: 16px;
  }

  .p-badge {
    padding: 2px 10px;
    font-size: 12px;
    font-weight: 600;
  }

  .p-stack {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
  }

  .p-alert {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 14px;
    border: 1px solid;
  }

  /* Brand */
  .logos {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 16px;
  }

  figure {
    margin: 0;
  }

  .grounds {
    display: grid;
    grid-template-columns: 1fr 1fr;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .ground {
    display: grid;
    place-items: center;
    height: 112px;
    padding: 16px;
  }

  .ground.light {
    background: #ffffff;
  }

  .ground.dark {
    background: #111216;
  }

  .ground img {
    max-width: 100%;
    max-height: 72px;
  }

  figcaption {
    margin-top: 6px;
    color: var(--muted);
  }

  .summary {
    max-height: 240px;
    margin: 0;
    padding: 12px;
    overflow: auto;
    border-radius: var(--radius);
    background: var(--hover);
    font: var(--text-sm) / 1.5 var(--font-mono);
    white-space: pre-wrap;
  }

  .guidelines {
    width: 100%;
    height: 640px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: #ffffff;
  }
</style>
