<script lang="ts">
  import { untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import Activity from "@lucide/svelte/icons/activity";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import ComponentIcon from "@lucide/svelte/icons/component";
  import Droplet from "@lucide/svelte/icons/droplet";
  import FileDown from "@lucide/svelte/icons/file-down";
  import Info from "@lucide/svelte/icons/info";
  import Layers from "@lucide/svelte/icons/layers";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Moon from "@lucide/svelte/icons/moon";
  import Palette from "@lucide/svelte/icons/palette";
  import Play from "@lucide/svelte/icons/play";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Ruler from "@lucide/svelte/icons/ruler";
  import Shapes from "@lucide/svelte/icons/shapes";
  import SquareRoundCorner from "@lucide/svelte/icons/square-round-corner";
  import Sun from "@lucide/svelte/icons/sun";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Type from "@lucide/svelte/icons/type";
  import Users from "@lucide/svelte/icons/users";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import SectionNav from "$lib/ui/SectionNav.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { design, openAt } from "$lib/design.svelte";
  import {
    FAMILY_SECTIONS,
    describe,
    passes,
    request,
    size,
    stillThere,
    swatches,
    tally,
    tokensOf,
    type Design,
    type Finding,
    type Level,
    type Section,
    type Token,
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

  const repos = $derived(scopeRepos());
  const repoName = $derived(repos.find((r) => r.path === design.repo)?.name ?? "");
  // The CTO who takes a request is the project's: under every project, the one that has the repository.
  const owner = $derived(app.workspace?.projects.find((p) => p.repos.some((r) => r.path === design.repo)) ?? null);
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

  async function scan(repo = design.repo) {
    if (!repo) return;
    loading = true;
    error = "";
    try {
      const found = await invoke<Design>("design_scan", { repo });
      if (repo !== design.repo) return;
      if (repo !== scanned) chosen.clear();
      data = found;
      scanned = repo;
      const kept = stillThere(new Set(chosen), found.findings);
      for (const id of [...chosen]) if (!kept.has(id)) chosen.delete(id);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  const findings = $derived(data?.findings ?? []);
  const counts = $derived(tally(findings));
  const shown = $derived(level === "all" ? findings : findings.filter((f) => f.level === level));
  const empty = $derived(!!data && !data.tokens.length && !data.components.length);

  const sections: { id: Section; label: Key; icon: typeof Droplet }[] = [
    { id: "color", label: "designColors", icon: Droplet },
    { id: "type", label: "designType", icon: Type },
    { id: "spacing", label: "designSpacing", icon: Ruler },
    { id: "radius", label: "designRadius", icon: SquareRoundCorner },
    { id: "shadow", label: "designShadows", icon: Layers },
    { id: "motion", label: "designMotion", icon: Activity },
    { id: "other", label: "designOther", icon: Shapes },
  ];
  const navItems = $derived([
    {
      id: "issues" as Section,
      label: t("designIssues"),
      icon: TriangleAlert,
      count: findings.length,
      dot: counts.error ? ("error" as const) : counts.warn ? ("warn" as const) : null,
    },
    ...sections
      .filter((s) => s.id !== "other" || tokensOf(data?.tokens ?? [], "other").length)
      .map((s, i) => ({ ...s, label: t(s.label), count: tokensOf(data?.tokens ?? [], s.id).length, apart: i === 0 })),
    { id: "components" as Section, label: t("designComponents"), icon: ComponentIcon, count: data?.components.length ?? 0, apart: true },
  ]);

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
    const path = await save({
      defaultPath: `${repoName || "brand"}-guidelines.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!path) return;
    exporting = true;
    error = "";
    try {
      await invoke("design_pdf", { repo: design.repo, path });
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
  const svg = (text: string) => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(text)}`;
  const fontTokens = $derived((data?.tokens ?? []).filter((tk) => tk.family === "font"));
  const brandColors = $derived((data?.tokens ?? []).filter((tk) => tk.family === "color" && !tk.alias && tk.color));
</script>

{#snippet source(tk: Token)}
  <button class="src mono" onclick={() => openAt(design.repo, tk.file, tk.line)} title={t("designOpenAt", { file: tk.file, line: tk.line })}>
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
      <p class="loading muted"><LoaderCircle size={16} class="spin" aria-hidden="true" />{t("designScanning")}</p>
    {/if}
  {:else}
    <Tabs
      id="design-tabs"
      label={t("designView")}
      tabs={[
        { id: "system", label: t("designTabSystem"), count: counts.error + counts.warn || null },
        { id: "brand", label: t("designTabBrand") },
      ]}
      selected={design.tab}
      onselect={(id) => (design.tab = id as "system" | "brand")}
    >
      {#if design.tab === "system"}
        {#if empty}
          <EmptyState icon={Palette} title={t("designEmptyTitle")} text={t("designEmptyText")}>
            <button class="btn primary" onclick={() => toCto(t("designGenerateRequest", { repo: repoName }))} disabled={!owner}>
              <Users size={14} aria-hidden="true" />{t("designGenerate")}
            </button>
          </EmptyState>
        {:else}
          <div class="body">
            <SectionNav items={navItems} current={design.section} label={t("designSections")} onselect={(id) => (design.section = id)} />
            <div class="page">
              {#if design.section === "issues"}
                <header class="head">
                  <h2>{t("designIssues")}</h2>
                  <div class="segmented" role="radiogroup" aria-label={t("designFilter")}>
                    {#each filters as [id, label] (id)}
                      <button role="radio" aria-checked={level === id} onclick={() => (level = id)}>
                        {t(label)} <span class="n">{id === "all" ? findings.length : counts[id]}</span>
                      </button>
                    {/each}
                  </div>
                </header>
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
                                  <button class="place" onclick={() => openAt(design.repo, a.file, a.line)} title={t("designOpenAt", { file: a.file, line: a.line })}>
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
              {:else if design.section === "components"}
                <header class="head"><h2>{t("designComponents")}</h2></header>
                {#if !data.components.length}
                  <p class="muted">{t("designNoComponents")}</p>
                {:else}
                  <ul class="components">
                    {#each data.components as c (c.file)}
                      <li>
                        <div class="c-head">
                          <strong>{c.name}</strong>
                          <span class="badge">{c.framework}</span>
                          <span class="grow"></span>
                          <span class="uses" class:none={c.uses === 0} title={c.usedIn.join("\n")}>{uses(c.uses)}</span>
                        </div>
                        <button class="src mono" onclick={() => openAt(design.repo, c.file, 1)}>{c.file}</button>
                        {#each c.variants as v (v.prop)}
                          <div class="variants">
                            <span class="muted">{v.prop}</span>
                            {#each v.values as value (value)}<span class="badge">{value}</span>{/each}
                          </div>
                        {/each}
                        {#if c.stories.length}
                          <p class="stories muted">{t("designStories", { stories: c.stories.join(", ") })}</p>
                        {/if}
                      </li>
                    {/each}
                  </ul>
                {/if}
              {:else}
                {@const list = tokensOf(data.tokens, design.section)}
                {@const title = sections.find((s) => s.id === design.section)?.label ?? "designOther"}
                <header class="head">
                  <h2>{t(title)}</h2>
                  {#if design.section === "color" && data.dark}
                    <span class="legend muted"><Sun size={13} aria-hidden="true" />{t("designLight")}<Moon size={13} aria-hidden="true" />{t("designDark")}</span>
                  {/if}
                </header>
                {#if !list.length}
                  <p class="muted">{t("designNoTokens")}</p>
                {:else if design.section === "color"}
                  <div class="swatches">
                    {#each list as tk (tk.name)}
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
                            <td><span class="sample" style:color={p.fgColor} style:background={p.bgColor}>Aa</span></td>
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
                {:else if design.section === "type"}
                  <p class="hint">{t("designRendered")}</p>
                  <ul class="rows">
                    {#each list as tk (tk.name)}
                      <li class="type-row">
                        {#if tk.family === "font"}
                          <span class="type-sample" style:font-family={tk.value}>{t("designSample")}</span>
                        {:else if tk.family === "text" && tk.px}
                          <span class="type-sample" style:font-size="{Math.min(tk.px, 64)}px">{t("designSample")}</span>
                        {:else if tk.family === "weight"}
                          <span class="type-sample" style:font-weight={tk.value}>{t("designSample")}</span>
                        {/if}
                        <div class="row-meta">
                          <span class="name mono">{tk.name}</span>
                          <span class="value mono">{tk.value}</span>
                          {@render meta(tk)}
                        </div>
                      </li>
                    {/each}
                  </ul>
                {:else}
                  <ul class="rows">
                    {#each list as tk (tk.name)}
                      <li class="token-row">
                        <span class="viz">
                          {#if design.section === "spacing" && tk.px !== null}
                            <span class="bar" style:width="{Math.min(tk.px, 120)}px"></span>
                          {:else if design.section === "radius"}
                            <span class="corner" style:border-radius={tk.px !== null ? `${tk.px}px` : tk.value}></span>
                          {:else if design.section === "shadow"}
                            <span class="lift" style:box-shadow={tk.value}></span>
                          {:else if design.section === "motion"}
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
                {/if}
              {/if}
            </div>
          </div>
        {/if}
      {:else}
        {@const brand = data.brand}
        <div class="page brand">
          {#if !brand}
            <EmptyState icon={Palette} title={t("designBrandEmptyTitle")} text={t("designBrandEmptyText")}>
              <button class="btn primary" onclick={() => toCto(t("designBrandRequest", { repo: repoName }))} disabled={!owner}>
                <Users size={14} aria-hidden="true" />{t("designBrandAsk")}
              </button>
            </EmptyState>
          {:else}
            <header class="head">
              <h2>{t("designTabBrand")}</h2>
              <span class="grow"></span>
              <button class="btn sm primary" onclick={exportPdf} disabled={!brand.guidelines || exporting} title={brand.guidelines ? undefined : t("designNoGuidelines")}>
                {#if exporting}<LoaderCircle size={14} class="spin" aria-hidden="true" />{:else}<FileDown size={14} aria-hidden="true" />{/if}
                {t("designPdf")}
              </button>
            </header>
            {#if brand.pdf}<p class="hint">{t("designPdfInRepo")}</p>{/if}

            <section>
              <h3>{t("designLogo")}</h3>
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

            {#if fontTokens.length}
              <section>
                <h3>{t("designType")}</h3>
                <ul class="rows">
                  {#each fontTokens as tk (tk.name)}
                    <li class="type-row">
                      <span class="type-sample" style:font-family={tk.value}>{t("designSample")}</span>
                      <div class="row-meta"><span class="name mono">{tk.name}</span><span class="value mono">{tk.value}</span></div>
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
                <h3>BRAND.md</h3>
                <pre class="summary">{brand.summary}</pre>
              </section>
            {/if}

            <section>
              <h3>{t("designGuidelines")}</h3>
              {#if brand.guidelines}
                <p class="hint">{t("designPreviewNote")}</p>
                <iframe class="guidelines" title={t("designGuidelines")} sandbox="" srcdoc={brand.guidelines}></iframe>
              {:else}
                <p class="muted">{t("designNoGuidelines")}</p>
              {/if}
            </section>
          {/if}
        </div>
      {/if}
    </Tabs>
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

  .design > :global(.panel) {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
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

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin: 48px auto;
  }

  .body {
    display: grid;
    flex: 1;
    grid-template: minmax(0, 1fr) / 208px minmax(0, 1fr);
    min-height: 0;
  }

  .page {
    min-width: 0;
    min-height: 0;
    padding: 20px 28px 40px;
    overflow-y: auto;
  }

  .page.brand {
    flex: 1;
  }

  .page > :global(*) {
    max-width: 960px;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    margin-bottom: 8px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 28px 0 6px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  section > h3 {
    margin-top: 24px;
  }

  .lede,
  .hint {
    margin: 0 0 12px;
    color: var(--muted);
    font-size: var(--text-sm);
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

  .legend {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-sm);
  }

  .legend :global(svg:last-of-type) {
    margin-left: 8px;
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

  /* Tokens */
  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(176px, 1fr));
    gap: 12px;
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
  .exports {
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  .pairs td,
  .exports td {
    padding: 4px 16px 4px 0;
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

  .lift {
    flex-shrink: 0;
    width: 64px;
    height: 40px;
    border-radius: var(--radius);
    background: var(--panel);
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
  .components {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }

  .components li {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .c-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .components .src {
    font-size: var(--text-xs);
  }

  .variants {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    font-size: var(--text-sm);
  }

  .variants .muted {
    margin-right: 2px;
  }

  .stories {
    margin: 0;
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
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
