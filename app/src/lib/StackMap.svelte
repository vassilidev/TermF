<script lang="ts">
  import Cylinder from "@lucide/svelte/icons/cylinder";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import ShieldAlert from "@lucide/svelte/icons/shield-exclamation-point";
  import CircleArrowUp from "@lucide/svelte/icons/circle-arrow-up";
  import CalendarX from "@lucide/svelte/icons/calendar-x";
  import { t } from "$lib/i18n";
  import { arc, dbLabel, drop, headline, layout, short, type Box, type StackMap } from "$lib/stack-logic";

  type Tally = { updates: number; vulnerable: number; eol: number; worst: string | null; bySeverity: Record<string, number> };

  /**
   * The project's repositories and their databases, with the links their files say: who calls whom, who depends on a
   * package another publishes. A click on a repository opens it below.
   */
  let {
    map,
    tallies,
    selected,
    onpick,
  }: { map: StackMap; tallies: Record<string, Tally | "loading" | undefined>; selected: string; onpick: (path: string) => void } = $props();

  let width = $state(0);
  const placed = $derived(width ? layout(map, width) : null);
  const boxOf = (path: string): Box | undefined => placed?.repos[map.repos.findIndex((r) => r.path === path)];
  const nameOf = (path: string) => map.repos.find((r) => r.path === path)?.name ?? path;
  const arrows = $derived(map.links.filter((l) => l.kind !== "database"));
  /** Links between the same two repositories are spread side by side. */
  const lanes = $derived(
    arrows.map((l, i) => arrows.slice(0, i).filter((o) => [o.from, o.to].sort().join() === [l.from, l.to].sort().join()).length),
  );
  const describe = (l: (typeof arrows)[number]) =>
    t(l.kind === "calls" ? "stackCallsVia" : "stackPackageVia", { from: nameOf(l.from), to: nameOf(l.to), via: l.via.join(", ") });
</script>

<div class="map" bind:clientWidth={width}>
  {#if placed}
    <div class="canvas" style:height="{placed.height}px">
      <svg width={placed.width} height={placed.height} aria-hidden="true">
        <defs>
          <marker id="stack-calls" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
            <path d="M 0 0 L 10 5 L 0 10 z" class="head calls" />
          </marker>
          <marker id="stack-package" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
            <path d="M 0 0 L 10 5 L 0 10 z" class="head package" />
          </marker>
        </defs>
        {#each placed.dbs as db (db.key)}
          {#each db.repos as path (path)}
            {@const box = boxOf(path)}
            {#if box}<path class="link database" class:lit={path === selected} d={drop(box, db)} />{/if}
          {/each}
        {/each}
        {#each arrows as link, i (`${link.from}>${link.to}>${link.kind}`)}
          {@const a = boxOf(link.from)}
          {@const b = boxOf(link.to)}
          {#if a && b}
            <path
              class="link {link.kind}"
              class:lit={link.from === selected || link.to === selected}
              d={arc(a, b, lanes[i])}
              marker-end="url(#stack-{link.kind})"
            >
              <title>{describe(link)}</title>
            </path>
          {/if}
        {/each}
      </svg>
      {#each map.repos as repo, i (repo.path)}
        {@const box = placed.repos[i]}
        {@const tally = tallies[repo.path]}
        <button
          class="node"
          class:selected={repo.path === selected}
          style:left="{box.x}px"
          style:top="{box.y}px"
          style:width="{box.w}px"
          style:height="{box.h}px"
          title={repo.path}
          aria-pressed={repo.path === selected}
          onclick={() => onpick(repo.path)}
        >
          <span class="name"><FolderGit2 size={14} aria-hidden="true" />{repo.name}</span>
          <span class="tags">
            {#each headline(repo.tags) as tag (tag.name)}
              <span class="tag">{tag.name}{#if tag.version}<span class="v">{short(tag.version)}</span>{/if}</span>
            {/each}
          </span>
          <span class="badges">
            {#if tally === "loading" || tally === undefined}
              <span class="bone" aria-hidden="true"></span>
            {:else}
              {#if tally.vulnerable}
                <span class="badge sev-{tally.worst}" title={t("stackBadgeVulns", { count: tally.vulnerable })}>
                  <ShieldAlert size={12} aria-hidden="true" />{tally.vulnerable}<span class="sr-only">{t("stackBadgeVulns", { count: tally.vulnerable })}</span>
                </span>
              {/if}
              {#if tally.eol}
                <span class="badge eol" title={t("stackBadgeEol", { count: tally.eol })}>
                  <CalendarX size={12} aria-hidden="true" />{tally.eol}<span class="sr-only">{t("stackBadgeEol", { count: tally.eol })}</span>
                </span>
              {/if}
              {#if tally.updates}
                <span class="badge updates" title={t("stackBadgeUpdates", { count: tally.updates })}>
                  <CircleArrowUp size={12} aria-hidden="true" />{tally.updates}<span class="sr-only">{t("stackBadgeUpdates", { count: tally.updates })}</span>
                </span>
              {/if}
              {#if !tally.vulnerable && !tally.eol && !tally.updates}
                <span class="badge ok">{t("stackUpToDate")}</span>
              {/if}
            {/if}
          </span>
        </button>
      {/each}
      {#each placed.dbs as db (db.key)}
        <div
          class="db"
          class:lit={db.repos.includes(selected)}
          style:left="{db.x}px"
          style:top="{db.y}px"
          style:width="{db.w}px"
          style:height="{db.h}px"
          title={t("stackDatabaseOf", { name: db.db.name, engine: db.db.engine, repos: db.repos.map(nameOf).join(", ") })}
        >
          <Cylinder size={14} aria-hidden="true" />
          <span class="db-text">
            <span class="db-name">{dbLabel(db.db)}</span>
            <span class="db-engine">{db.db.engine}{db.repos.length > 1 ? ` · ${db.repos.length}` : ""}</span>
          </span>
        </div>
      {/each}
    </div>
  {/if}
  <div class="legend">
    <span><svg width="26" height="8" aria-hidden="true"><path class="link calls lit" d="M 1 4 L 25 4" /></svg>{t("stackLegendCalls")}</span>
    <span><svg width="26" height="8" aria-hidden="true"><path class="link package lit" d="M 1 4 L 25 4" /></svg>{t("stackLegendPackage")}</span>
    <span><svg width="26" height="8" aria-hidden="true"><path class="link database lit" d="M 1 4 L 25 4" /></svg>{t("stackLegendDatabase")}</span>
    {#if !arrows.length}<span class="none">{t("stackNoLinks")}</span>{/if}
  </div>
  <ul class="sr-only">
    {#each arrows as link (`${link.from}>${link.to}>${link.kind}`)}<li>{describe(link)}</li>{/each}
  </ul>
</div>

<style>
  .map {
    min-width: 0;
  }

  .canvas {
    position: relative;
  }

  svg {
    position: absolute;
    inset: 0;
    overflow: visible;
  }

  .link {
    fill: none;
    stroke-width: 1.5;
    opacity: 0.55;
    transition: opacity var(--dur-fast) ease;
  }

  .link.lit,
  .link:hover {
    opacity: 1;
  }

  .link.calls {
    stroke: var(--accent-ink);
  }

  .link.package {
    stroke: var(--c-product);
    stroke-dasharray: 6 4;
  }

  .link.database {
    stroke: var(--muted);
    stroke-dasharray: 2 3;
  }

  .head.calls {
    fill: var(--accent-ink);
  }

  .head.package {
    fill: var(--c-product);
  }

  .node {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--raised);
    color: var(--text);
    text-align: left;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.04);
    transition:
      border-color var(--dur-fast) ease,
      box-shadow var(--dur-fast) ease;
  }

  .node:hover {
    border-color: var(--accent);
  }

  .node.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name :global(svg) {
    flex: none;
    color: var(--muted);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    max-height: 20px;
    overflow: hidden;
  }

  .tag {
    display: inline-flex;
    gap: 3px;
    padding: 1px 6px;
    border-radius: var(--radius-pill);
    background: var(--well);
    color: var(--soft);
    font-size: var(--text-xs);
  }

  .v {
    color: var(--muted);
    font-family: var(--font-mono);
  }

  .badges {
    display: flex;
    gap: 6px;
    margin-top: auto;
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 6px;
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .badge.sev-critical,
  .badge.sev-high,
  .badge.eol {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .badge.sev-moderate {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .badge.sev-low,
  .badge.sev-unknown {
    background: var(--well);
    color: var(--soft);
  }

  .badge.updates {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .badge.ok {
    background: var(--done-soft);
    color: var(--done);
  }

  .bone {
    width: 72px;
    height: 14px;
    border-radius: var(--radius-pill);
    background: var(--well);
    animation: pulse 1.4s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }

  .db {
    position: absolute;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--well);
    color: var(--soft);
  }

  .db.lit {
    border-style: solid;
    border-color: var(--muted);
    color: var(--text);
  }

  .db :global(svg) {
    flex: none;
  }

  .db-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.25;
  }

  .db-name {
    overflow: hidden;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .db-engine {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 18px;
    margin-top: 14px;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .legend svg {
    position: static;
  }

  .none {
    margin-left: auto;
  }

  @media (prefers-reduced-motion: reduce) {
    .bone {
      animation: none;
    }
  }
</style>
