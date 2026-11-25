<script lang="ts">
  import { untrack } from "svelte";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import Link2 from "@lucide/svelte/icons/link-2";
  import Maximize from "@lucide/svelte/icons/maximize";
  import MessageCircleQuestion from "@lucide/svelte/icons/message-circle-question";
  import Search from "@lucide/svelte/icons/search";
  import SquarePen from "@lucide/svelte/icons/square-pen";
  import Table2 from "@lucide/svelte/icons/table-2";
  import X from "@lucide/svelte/icons/x";
  import ZoomIn from "@lucide/svelte/icons/zoom-in";
  import ZoomOut from "@lucide/svelte/icons/zoom-out";
  import { isMac } from "$lib/files.svelte";
  import { t } from "$lib/i18n";
  import { CARD, cardHeight, layout, relations, tableName, type Relation, type SchemaTable } from "$lib/ops-logic";

  /** The whole database as an entity-relationship diagram: tables as cards, foreign keys as links; a click opens a
   * table's columns, indexes and relations. */
  let { tables, onquery, onask }: { tables: SchemaTable[]; onquery: (table: string) => void; onask: (table: string) => void } = $props();

  const DETAIL = 320;
  const links = $derived(relations(tables));
  const at = $derived(layout(tables, links));
  const byName = $derived(new Map(tables.map((tb) => [tableName(tb), tb])));
  const size = $derived.by(() => {
    let [w, h] = [0, 0];
    for (const tb of tables) {
      const p = at.get(tableName(tb))!;
      [w, h] = [Math.max(w, p.x + CARD.width), Math.max(h, p.y + cardHeight(tb))];
    }
    return { w, h };
  });

  let search = $state("");
  let selected = $state<string | null>(null);
  let zoom = $state(1);
  let pan = $state({ x: 0, y: 0 });
  let box = $state<HTMLElement>();
  let boxW = $state(0);
  let boxH = $state(0);

  const q = $derived(search.trim().toLowerCase());
  const found = $derived(
    q ? tables.filter((tb) => tableName(tb).toLowerCase().includes(q) || tb.columns.some((c) => c.name.toLowerCase().includes(q))).map(tableName) : null,
  );
  const table = $derived(selected ? byName.get(selected) : undefined);
  const outgoing = $derived(links.filter((l) => l.from === selected));
  const incoming = $derived(links.filter((l) => l.to === selected && l.from !== selected));
  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

  /** Everything in sight; at first no smaller than readable, from the top left. */
  function fit(readable = false) {
    if (!boxW || !boxH || !size.w) return;
    zoom = clamp(Math.min((boxW - 48) / size.w, (boxH - 48) / size.h), readable ? 0.8 : 0.15, 1);
    pan = { x: Math.max(24, (boxW - size.w * zoom) / 2), y: Math.max(24, (boxH - size.h * zoom) / 2) };
  }

  let fitted = false;
  $effect(() => {
    if (boxW && boxH && size.w && !fitted) {
      fitted = true;
      untrack(() => fit(true));
    }
  });

  function zoomAt(factor: number, cx = boxW / 2, cy = boxH / 2) {
    const next = clamp(zoom * factor, 0.15, 2);
    pan = { x: cx - ((cx - pan.x) * next) / zoom, y: cy - ((cy - pan.y) * next) / zoom };
    zoom = next;
  }

  /** Puts a table in sight, beside the details when they show. */
  function show(name: string) {
    const p = at.get(name);
    if (!p) return;
    const room = boxW - (selected ? DETAIL : 0);
    pan = { x: room / 2 - (p.x + CARD.width / 2) * zoom, y: Math.min(boxH / 2, 160) - (p.y + CARD.head) * zoom };
  }

  function pick(name: string) {
    selected = name;
    show(name);
  }

  // A pinch, or the wheel with ⌘ / Ctrl, zooms where the pointer is; the wheel alone moves.
  function wheel(e: WheelEvent) {
    e.preventDefault();
    const r = box!.getBoundingClientRect();
    if (e.ctrlKey || e.metaKey) zoomAt(Math.exp(-e.deltaY / 300), e.clientX - r.left, e.clientY - r.top);
    else pan = { x: pan.x - e.deltaX, y: pan.y - e.deltaY };
  }

  $effect(() => {
    const el = box;
    if (!el) return;
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  });

  let drag = $state<{ x: number; y: number; px: number; py: number } | null>(null);
  let moved = $state(false);
  function down(e: PointerEvent) {
    if (e.button !== 0 || (e.target as Element).closest(".detail, .tools")) return;
    drag = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
    moved = false;
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const [dx, dy] = [e.clientX - drag.x, e.clientY - drag.y];
    if (!moved && Math.hypot(dx, dy) < 4) return;
    if (!moved) box?.setPointerCapture(e.pointerId);
    moved = true;
    pan = { x: drag.px + dx, y: drag.py + dy };
  }
  function up() {
    drag = null;
  }

  /** A link from a key's row to the row it references, out of the facing sides of the two cards. */
  function path(l: Relation): string {
    const [a, b] = [at.get(l.from)!, at.get(l.to)!];
    const ya = a.y + CARD.head + l.fromRow * CARD.row + CARD.row / 2;
    const yb = b.y + CARD.head + l.toRow * CARD.row + CARD.row / 2;
    if (a.x === b.x) {
      const xr = a.x + CARD.width;
      return `M${xr} ${ya}C${xr + 56} ${ya},${xr + 56} ${yb},${xr} ${yb}`;
    }
    const [x1, x2] = a.x > b.x ? [a.x, b.x + CARD.width] : [a.x + CARD.width, b.x];
    const d = Math.max(40, Math.abs(x1 - x2) / 2) * Math.sign(x2 - x1);
    return `M${x1} ${ya}C${x1 + d} ${ya},${x2 - d} ${yb},${x2} ${yb}`;
  }

  const target = (l: Relation) => {
    const tb = byName.get(l.to);
    return `${l.to}.${tb?.columns[l.toRow]?.name ?? ""}`;
  };
  const keyLabel = { pk: "opsdbKeyPk", fk: "opsdbKeyFk", unique: "opsdbKeyUnique", index: "opsdbKeyIndex" } as const;
</script>

{#snippet keyIcon(key: SchemaTable["columns"][number]["key"])}
  <span class="key" title={key ? t(keyLabel[key]) : undefined}>
    {#if key === "pk"}<KeyRound size={11} aria-label={t("opsdbKeyPk")} />{:else if key === "fk"}<Link2 size={11} aria-label={t("opsdbKeyFk")} />{/if}
  </span>
{/snippet}

<div class="diagram">
  <div class="tools">
    <label class="search">
      <Search size={13} aria-hidden="true" />
      <input
        class="field"
        type="search"
        placeholder={t("sqlcsSearch")}
        aria-label={t("sqlcsSearch")}
        bind:value={search}
        oninput={() => found?.length && show(found[0])}
        onkeydown={(e) => e.key === "Enter" && found?.length && pick(found[0])}
      />
    </label>
    {#if found}<span class="muted small">{found.length ? t("opsdbSchemaCount", { count: found.length }) : t("opsdbSchemaNoMatch")}</span>{/if}
    <span class="muted small help">{t("sqlcsHelp", { key: isMac ? "⌘" : "Ctrl" })}</span>
    <div class="zoom">
      <button class="icon-btn" title={t("sqlcsZoomOut")} aria-label={t("sqlcsZoomOut")} onclick={() => zoomAt(1 / 1.25)}><ZoomOut size={14} aria-hidden="true" /></button>
      <span class="pct">{Math.round(zoom * 100)} %</span>
      <button class="icon-btn" title={t("sqlcsZoomIn")} aria-label={t("sqlcsZoomIn")} onclick={() => zoomAt(1.25)}><ZoomIn size={14} aria-hidden="true" /></button>
      <button class="icon-btn" title={t("sqlcsFit")} aria-label={t("sqlcsFit")} onclick={() => fit()}><Maximize size={14} aria-hidden="true" /></button>
    </div>
  </div>

  <div
    class="canvas"
    class:dragging={moved && !!drag}
    role="application"
    aria-label={t("opsdbSchema")}
    bind:this={box}
    bind:clientWidth={boxW}
    bind:clientHeight={boxH}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
  >
    <div class="world" style:width="{size.w}px" style:height="{size.h}px" style:transform="translate({pan.x}px, {pan.y}px) scale({zoom})">
      <svg class="links" width={size.w} height={size.h} aria-hidden="true">
        {#each links as l, i (i)}
          {@const on = selected !== null && (l.from === selected || l.to === selected)}
          <path class:on d={path(l)}></path>
        {/each}
      </svg>
      {#each tables as tb (tableName(tb))}
        {@const name = tableName(tb)}
        {@const p = at.get(name)!}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (its header is the button) -->
        <div
          class="card"
          class:on={selected === name}
          class:match={found?.includes(name)}
          class:dim={found && !found.includes(name)}
          style:left="{p.x}px"
          style:top="{p.y}px"
          style:width="{CARD.width}px"
          style:height="{cardHeight(tb)}px"
          onclick={() => !moved && pick(name)}
        >
          <button class="head" style:height="{CARD.head}px" onclick={(e) => (e.stopPropagation(), !moved && pick(name))}>
            <Table2 size={13} aria-hidden="true" /><span>{name}</span>
          </button>
          {#each tb.columns as c (c.name)}
            <div class="row" style:height="{CARD.row}px">
              {@render keyIcon(c.key)}<span class="col">{c.name}</span><span class="type">{c.type}</span>
            </div>
          {/each}
        </div>
      {/each}
    </div>

    {#if table && selected}
      <aside class="detail" aria-label={selected}>
        <header>
          <Table2 size={14} aria-hidden="true" /><b>{selected}</b>
          <button class="icon-btn" title={t("close")} aria-label={t("close")} onclick={() => (selected = null)}><X size={14} aria-hidden="true" /></button>
        </header>
        <div class="actions">
          <button class="btn sm primary" onclick={() => onquery(selected!)}><SquarePen size={13} aria-hidden="true" />{t("sqlcsQueryTable")}</button>
          <button class="btn sm" onclick={() => onask(selected!)}><MessageCircleQuestion size={13} aria-hidden="true" />{t("sqlcsAskDba")}</button>
        </div>
        <h4>{t("sqlcsColumns")}</h4>
        <ul class="list">
          {#each table.columns as c (c.name)}
            <li>
              {@render keyIcon(c.key)}<span class="col">{c.name}</span><span class="type">{c.type}</span>
              {#if !c.nullable}<span class="muted req">{t("sqlcsRequired")}</span>{/if}
            </li>
          {/each}
        </ul>
        <h4>{t("sqlcsIndexes")}</h4>
        {#if table.indexes.length}
          <ul class="list">
            {#each table.indexes as ix (ix.name)}
              <li class="index" title={ix.columns}>
                <span class="col">{ix.name}</span>{#if ix.unique}<span class="badge">{t("opsdbKeyUnique")}</span>{/if}
                <span class="type">{ix.columns}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted small">{t("sqlcsNoIndex")}</p>
        {/if}
        {#if !outgoing.length && !incoming.length}
          <p class="muted small">{t("sqlcsNoRelation")}</p>
        {/if}
        {#if outgoing.length}
          <h4>{t("sqlcsPointsTo")}</h4>
          <ul class="list">
            {#each outgoing as l, i (i)}
              <li><button class="rel" onclick={() => pick(l.to)}>{table.columns[l.fromRow]?.name} → {target(l)}</button></li>
            {/each}
          </ul>
        {/if}
        {#if incoming.length}
          <h4>{t("sqlcsPointedBy")}</h4>
          <ul class="list">
            {#each incoming as l, i (i)}
              <li>
                <button class="rel" onclick={() => pick(l.from)}>{l.from}.{byName.get(l.from)?.columns[l.fromRow]?.name} → {target(l).split(".").at(-1)}</button>
              </li>
            {/each}
          </ul>
        {/if}
      </aside>
    {/if}
  </div>
</div>

<style>
  .diagram {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 8px;
    min-height: 420px;
  }

  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
  }

  .search {
    position: relative;
    display: flex;
    align-items: center;
    color: var(--muted);
  }

  .search :global(svg) {
    position: absolute;
    left: 9px;
  }

  .search input {
    width: 260px;
    height: var(--h-sm);
    padding-left: 28px;
  }

  .small {
    font-size: var(--text-xs);
  }

  .help {
    margin-left: auto;
  }

  .zoom {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .pct {
    min-width: 44px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    text-align: center;
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .icon-btn:hover {
    background: var(--hover);
    color: var(--text);
  }

  .canvas {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background-color: var(--well);
    background-image: radial-gradient(var(--border-strong) 1px, transparent 1px);
    background-size: 20px 20px;
    cursor: grab;
    touch-action: none;
    user-select: none;
  }

  .canvas.dragging {
    cursor: grabbing;
  }

  .world {
    position: absolute;
    top: 0;
    left: 0;
    transform-origin: 0 0;
  }

  .links {
    position: absolute;
    inset: 0;
    overflow: visible;
    pointer-events: none;
  }

  .links path {
    fill: none;
    stroke: var(--border-strong);
    stroke-width: 1.5;
  }

  .links path.on {
    stroke: var(--accent);
    stroke-width: 2.5;
  }

  .card {
    position: absolute;
    box-sizing: border-box;
    overflow: hidden;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-1);
    transition: opacity var(--dur-fast) ease;
  }

  .card.on,
  .card.match {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .card.dim {
    opacity: 0.35;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 0 10px;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
    color: var(--text);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }

  .head span,
  .col,
  .type {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .head:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    font-size: var(--text-xs);
  }

  .key {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 12px;
    color: var(--waiting-ring);
  }

  .type {
    margin-left: auto;
    max-width: 50%;
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .detail {
    position: absolute;
    top: 8px;
    right: 8px;
    bottom: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 304px;
    padding: 12px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--raised);
    box-shadow: var(--shadow-2);
    cursor: auto;
    user-select: text;
  }

  .detail header {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .detail header b {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  h4 {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-xs);
  }

  .list li {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 22px;
  }

  .list .index {
    flex-wrap: wrap;
  }

  .list .index .type {
    flex-basis: 100%;
    max-width: none;
    margin-left: 18px;
  }

  .req {
    font-size: 11px;
  }

  .badge {
    padding: 0 6px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: 11px;
  }

  .rel {
    padding: 2px 6px;
    overflow: hidden;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--accent-ink);
    font-family: var(--font-mono);
    font-size: 11px;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }

  .rel:hover {
    background: var(--hover);
  }
</style>
