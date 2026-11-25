<script lang="ts">
  import { untrack } from "svelte";
  import { lang, t } from "$lib/i18n";
  import { autoChart, chartPoints, columnKind, pieReadable, SLICES, ticks, type ChartSpec, type ChartType, type Table } from "$lib/ops-logic";

  /** A result as a chart: the one it asks for first, then its type, X axis and series as the user picks them. */
  let { table }: { table: Table } = $props();

  // The design system's colours, the accent first; the role families are the other clear, distinct hues.
  const palette = ["--accent", "--c-creative", "--c-quality", "--c-specialists", "--c-build", "--c-product", "--waiting-ring", "--muted"].map(
    (v) => `var(${v})`,
  );
  const types: { id: ChartType; label: Parameters<typeof t>[0] }[] = [
    { id: "bar", label: "sqlcsBar" },
    { id: "line", label: "sqlcsLine" },
    { id: "area", label: "sqlcsArea" },
    { id: "pie", label: "sqlcsPie" },
  ];
  const H = 300;
  const DAY = 86_400_000;

  let spec = $state<ChartSpec | null>(untrack(() => autoChart(table)));
  let width = $state(0);
  let hover = $state<number | null>(null);
  let svg = $state<SVGSVGElement>();

  const kinds = $derived(table.columns.map((_, i) => columnKind(table, i)));
  const numeric = $derived(kinds.flatMap((k, i) => (k === "number" ? [i] : [])));
  const xKind = $derived(spec ? kinds[spec.x] : "text");
  const points = $derived(spec?.series.length ? chartPoints(table, spec, xKind) : []);
  const pie = $derived(spec ? pieReadable(points, spec.series.length) : false);
  const type = $derived(spec?.type === "pie" && !pie ? "bar" : (spec?.type ?? "bar"));
  const names = $derived(spec?.series.map((s) => table.columns[s]) ?? []);

  const M = { top: 34, right: 16, bottom: 44, left: 58 };
  const plotW = $derived(Math.max(0, width - M.left - M.right));
  const plotH = H - M.top - M.bottom;
  const values = $derived(points.flatMap((p) => p.values).filter((v): v is number => v !== null));
  const yTicks = $derived(ticks(Math.min(0, ...values), Math.max(0, ...values)));
  const [y0, y1] = $derived([yTicks[0], yTicks.at(-1)!]);
  const y = (v: number) => M.top + plotH - ((v - y0) / (y1 - y0)) * plotH;
  const band = $derived(plotW / Math.max(1, points.length));
  const linear = $derived(type !== "bar" && xKind !== "text");
  const [lo, hi] = $derived([points[0]?.at ?? 0, points.at(-1)?.at ?? 0]);
  const x = (i: number) => (linear ? M.left + (hi === lo ? plotW / 2 : ((points[i].at - lo) / (hi - lo)) * plotW) : M.left + (i + 0.5) * band);

  const compact = $derived(new Intl.NumberFormat(lang, { notation: "compact", maximumFractionDigits: 1 }));
  const full = (v: number | null) => (v === null ? "NULL" : v.toLocaleString(lang));
  function label(i: number): string {
    const p = points[i];
    if (xKind !== "date") return p.label.length > 14 ? `${p.label.slice(0, 13)}…` : p.label;
    const span = hi - lo;
    const o: Intl.DateTimeFormatOptions = span >= 2 * DAY || span === 0 ? { day: "numeric", month: "short" } : { hour: "2-digit", minute: "2-digit" };
    if (span > 330 * DAY) o.year = "numeric";
    return new Date(p.at).toLocaleString(lang, { ...o, timeZone: "UTC" });
  }
  /** A point's X in full, for the tooltip: a date as a date, the rest as the database gave it. */
  function said(i: number): string {
    const p = points[i];
    if (xKind !== "date") return p.label;
    const o: Intl.DateTimeFormatOptions = p.at < DAY ? { timeStyle: "short" } : p.at % DAY ? { dateStyle: "medium", timeStyle: "short" } : { dateStyle: "medium" };
    return new Date(p.at).toLocaleString(lang, { ...o, timeZone: "UTC" });
  }
  const step = $derived(Math.ceil(points.length / Math.max(1, Math.floor(plotW / 84))));

  /** A series' line, broken where a value is missing. */
  function line(s: number): string {
    let d = "";
    let open = false;
    points.forEach((p, i) => {
      const v = p.values[s];
      if (v === null) return void (open = false);
      d += `${open ? "L" : "M"}${x(i).toFixed(1)} ${y(v).toFixed(1)}`;
      open = true;
    });
    return d;
  }

  function area(s: number): string {
    const kept = points.flatMap((p, i) => (p.values[s] === null ? [] : [i]));
    if (!kept.length) return "";
    const top = kept.map((i) => `L${x(i).toFixed(1)} ${y(points[i].values[s]!).toFixed(1)}`).join("");
    return `M${x(kept[0]).toFixed(1)} ${y(0)}${top}L${x(kept.at(-1)!).toFixed(1)} ${y(0)}Z`;
  }

  const total = $derived(points.reduce((sum, p) => sum + (p.values[0] ?? 0), 0));
  const R = (H - 40) / 2;
  const slices = $derived.by(() => {
    let from = -Math.PI / 2;
    return points.map((p) => {
      const share = (p.values[0] ?? 0) / total;
      const to = from + share * 2 * Math.PI;
      const at = (a: number) => `${(R + 24 + R * Math.cos(a)).toFixed(1)} ${(H / 2 + R * Math.sin(a)).toFixed(1)}`;
      const d = share >= 0.9999 ? "" : `M${R + 24} ${H / 2}L${at(from)}A${R} ${R} 0 ${share > 0.5 ? 1 : 0} 1 ${at(to)}Z`;
      from = to;
      return { d, share };
    });
  });

  function move(e: PointerEvent) {
    if (!points.length || type === "pie") return;
    const px = e.clientX - (e.currentTarget as SVGElement).getBoundingClientRect().left;
    if (px < M.left || px > M.left + plotW) return void (hover = null);
    if (!linear) return void (hover = Math.min(points.length - 1, Math.floor((px - M.left) / band)));
    let best = 0;
    points.forEach((_, i) => Math.abs(x(i) - px) < Math.abs(x(best) - px) && (best = i));
    hover = best;
  }

  function setType(id: ChartType) {
    if (spec) spec.type = id;
  }

  function setX(i: number) {
    if (!spec) return;
    spec.x = i;
    spec.series = spec.series.filter((s) => s !== i);
  }

  function toggle(i: number) {
    if (!spec) return;
    spec.series = spec.series.includes(i) ? spec.series.filter((s) => s !== i) : [...spec.series, i].sort((a, b) => a - b);
  }

  const color = (s: number) => palette[s % palette.length];

  const KEPT = ["fill", "fill-opacity", "stroke", "stroke-width", "stroke-dasharray", "opacity", "font-family", "font-size", "font-weight"];

  /** The chart as a file of its own: its colours and fonts written in, on the panel's ground; PNG at twice the size. */
  export async function image(format: "png" | "svg"): Promise<Uint8Array | null> {
    if (!svg) return null;
    const copy = svg.cloneNode(true) as SVGSVGElement;
    const from = [svg, ...svg.querySelectorAll("*")];
    [copy, ...copy.querySelectorAll("*")].forEach((el, i) => {
      const style = getComputedStyle(from[i]);
      el.setAttribute("style", KEPT.map((p) => `${p}:${style.getPropertyValue(p)}`).join(";"));
    });
    copy.querySelectorAll(".hover").forEach((el) => el.remove());
    const ground = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    ground.setAttribute("width", "100%");
    ground.setAttribute("height", "100%");
    ground.setAttribute("fill", getComputedStyle(document.documentElement).getPropertyValue("--panel").trim());
    copy.prepend(ground);
    copy.setAttribute("xmlns", "http://www.w3.org/2000/svg");
    copy.setAttribute("width", String(width));
    copy.setAttribute("height", String(H));
    const text = new XMLSerializer().serializeToString(copy);
    if (format === "svg") return new TextEncoder().encode(text);
    const img = new Image();
    img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(text)}`;
    await img.decode();
    const canvas = document.createElement("canvas");
    canvas.width = width * 2;
    canvas.height = H * 2;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.scale(2, 2);
    ctx.drawImage(img, 0, 0);
    const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done, "image/png"));
    return blob ? new Uint8Array(await blob.arrayBuffer()) : null;
  }
</script>

<div class="chart">
  {#if !spec}
    <p class="muted none">{t("sqlcsNone")}</p>
  {:else}
    <div class="settings">
      <div class="segmented" role="group" aria-label={t("sqlcsType")}>
        {#each types as k (k.id)}
          {@const off = k.id === "pie" && !pie}
          <button aria-pressed={type === k.id} disabled={off} title={off ? t("sqlcsPieNo", { slices: SLICES }) : undefined} onclick={() => setType(k.id)}>
            {t(k.label)}
          </button>
        {/each}
      </div>
      <label class="inline">
        <span>{t("sqlcsX")}</span>
        <select class="field" value={spec.x} onchange={(e) => setX(Number(e.currentTarget.value))}>
          {#each table.columns as c, i (i)}<option value={i}>{c}</option>{/each}
        </select>
      </label>
      <div class="series" role="group" aria-label={t("sqlcsSeries")}>
        <span>{t("sqlcsSeries")}</span>
        {#each numeric.filter((i) => i !== spec?.x) as i (i)}
          {@const on = spec.series.includes(i)}
          <button class="chip" aria-pressed={on} onclick={() => toggle(i)}>
            <span class="swatch" style:background={on ? color(spec.series.indexOf(i)) : "transparent"}></span>{table.columns[i]}
          </button>
        {/each}
      </div>
    </div>

    {#if !spec.series.length}
      <p class="muted none">{t("sqlcsNoSeries")}</p>
    {:else}
      <div class="plot" bind:clientWidth={width}>
        {#if width}
          <svg
            bind:this={svg}
            {width}
            height={H}
            viewBox="0 0 {width} {H}"
            role="img"
            aria-label={t("sqlcsChartOf", { series: names.join(", "), x: table.columns[spec.x] })}
            onpointermove={move}
            onpointerleave={() => (hover = null)}
          >
            {#if type === "pie"}
              {#each slices as s, i (i)}
                {#if s.d}
                  <path
                    class="slice"
                    d={s.d}
                    role="img"
                    aria-label="{points[i].label} · {full(points[i].values[0])}"
                    style:fill={color(i)}
                    onpointerenter={() => (hover = i)}
                    onpointerleave={() => (hover = null)}
                  ></path>
                {:else}
                  <circle
                    class="slice"
                    cx={R + 24}
                    cy={H / 2}
                    r={R}
                    role="img"
                    aria-label="{points[i].label} · {full(points[i].values[0])}"
                    style:fill={color(i)}
                    onpointerenter={() => (hover = i)}
                    onpointerleave={() => (hover = null)}
                  ></circle>
                {/if}
              {/each}
              {#each points as p, i (i)}
                <g transform="translate({2 * R + 64} {H / 2 - (points.length * 22) / 2 + i * 22})">
                  <rect width="10" height="10" y="-5" rx="2" style:fill={color(i)}></rect>
                  <text x="18" dy="0.35em" class="legend">{p.label} · {full(p.values[0])} ({Math.round(slices[i].share * 100)} %)</text>
                </g>
              {/each}
            {:else}
              {#each yTicks as v (v)}
                <line class="grid" class:zero={v === 0} x1={M.left} x2={M.left + plotW} y1={y(v)} y2={y(v)}></line>
                <text class="axis" x={M.left - 8} y={y(v)} dy="0.35em" text-anchor="end">{compact.format(v)}</text>
              {/each}
              {#each points as _, i (i)}
                {#if i % step === 0}
                  <text class="axis" x={x(i)} y={H - M.bottom + 18} text-anchor="middle">{label(i)}</text>
                {/if}
              {/each}
              {#if hover !== null && type === "bar"}
                <rect class="hover band" x={M.left + hover * band} y={M.top} width={band} height={plotH}></rect>
              {/if}
              {#each spec.series as _, s (s)}
                {#if type === "bar"}
                  {@const group = band * 0.72}
                  {#each points as p, i (i)}
                    {@const v = p.values[s]}
                    {#if v !== null}
                      <rect
                        x={M.left + i * band + (band - group) / 2 + (s * group) / spec.series.length}
                        y={Math.min(y(v), y(0))}
                        width={Math.max(1, group / spec.series.length - 1)}
                        height={Math.abs(y(0) - y(v))}
                        rx="2"
                        style:fill={color(s)}
                      ></rect>
                    {/if}
                  {/each}
                {:else}
                  {#if type === "area"}<path class="area" d={area(s)} style:fill={color(s)}></path>{/if}
                  <path class="line" d={line(s)} style:stroke={color(s)}></path>
                  {#if points.length <= 60}
                    {#each points as p, i (i)}
                      {#if p.values[s] !== null}<circle cx={x(i)} cy={y(p.values[s]!)} r={hover === i ? 4 : 2.5} style:fill={color(s)}></circle>{/if}
                    {/each}
                  {/if}
                {/if}
              {/each}
              {#if hover !== null && type !== "bar"}
                <line class="hover guide" x1={x(hover)} x2={x(hover)} y1={M.top} y2={M.top + plotH}></line>
              {/if}
              {#each names as name, s (s)}
                <g transform="translate({M.left + s * 150} 12)">
                  <rect width="10" height="10" y="-5" rx="2" style:fill={color(s)}></rect>
                  <text x="16" dy="0.35em" class="legend">{name.length > 18 ? `${name.slice(0, 17)}…` : name}</text>
                </g>
              {/each}
            {/if}
          </svg>
          {#if hover !== null && points[hover]}
            {@const p = points[hover]}
            {@const left = type === "pie" ? 2 * R + 64 : x(hover) > width / 2 ? Math.max(0, x(hover) - 232) : x(hover) + 12}
            <div class="tip" style:left="{left}px" role="status">
              <b>{said(hover)}</b>
              {#each names as name, s (s)}
                <span><i style:background={color(type === "pie" ? hover : s)}></i>{name}<em>{full(p.values[s])}</em></span>
              {/each}
            </div>
          {/if}
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .chart {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .none {
    margin: 0;
    font-size: var(--text-sm);
  }

  .settings {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 14px;
    font-size: var(--text-sm);
  }

  .settings .inline {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }

  .settings select {
    width: auto;
    max-width: 180px;
    height: var(--h-sm);
  }

  .segmented button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .series {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .series > span {
    color: var(--muted);
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: none;
    color: var(--muted);
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .chip[aria-pressed="true"] {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .swatch {
    width: 9px;
    height: 9px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
  }

  .plot {
    position: relative;
  }

  svg {
    display: block;
    overflow: visible;
    font-family: var(--font-ui);
  }

  .grid {
    stroke: var(--border);
    stroke-width: 1;
  }

  .grid.zero {
    stroke: var(--border-strong);
  }

  .axis {
    fill: var(--muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .legend {
    fill: var(--soft);
    font-size: 12px;
  }

  .line {
    fill: none;
    stroke-width: 2;
    stroke-linejoin: round;
  }

  .area {
    fill-opacity: 0.16;
  }

  .band {
    fill: var(--hover);
  }

  .guide {
    stroke: var(--border-strong);
    stroke-dasharray: 3 3;
  }

  .slice {
    stroke: var(--panel);
    stroke-width: 2;
  }

  .slice:hover {
    opacity: 0.85;
  }

  .tip {
    position: absolute;
    top: 30px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 140px;
    max-width: 220px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--raised);
    box-shadow: var(--shadow-2);
    font-size: var(--text-xs);
    pointer-events: none;
  }

  .tip b {
    overflow-wrap: anywhere;
  }

  .tip span {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }

  .tip i {
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }

  .tip em {
    margin-left: auto;
    color: var(--text);
    font-style: normal;
    font-variant-numeric: tabular-nums;
  }
</style>
