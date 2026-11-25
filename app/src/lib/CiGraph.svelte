<script lang="ts">
  import { tick } from "svelte";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleDashed from "@lucide/svelte/icons/circle-dashed";
  import CircleSlash from "@lucide/svelte/icons/circle-slash";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { t } from "$lib/i18n";
  import { duration, type Job } from "$lib/ci.svelte";
  import { cellState, type Cell, type CellState } from "$lib/ci-logic";

  /** The run's jobs in columns, each right of what it needs; a click opens a job's steps. */
  let { cols, selected = null, onselect }: { cols: Cell<Job>[][]; selected?: number | null; onselect: (job: Job) => void } = $props();

  const icons = { passed: CircleCheck, failed: CircleX, running: LoaderCircle, queued: CircleDashed, waiting: CircleDashed, skipped: CircleDashed, stopped: CircleSlash };
  const labels = { passed: "ciPassed", failed: "ciFailed", running: "ciRunning", queued: "ciQueued", waiting: "cvWaiting", skipped: "cvSkipped", stopped: "ciStopped" } as const;

  let box = $state<HTMLElement>();
  let wires = $state<{ d: string; bad: boolean }[]>([]);
  let size = $state({ w: 0, h: 0 });

  const live = $derived(cols.some((c) => c.some((cell) => ["running", "queued"].includes(cellState(cell)))));
  let now = $state(Date.now());
  $effect(() => {
    if (!live) return;
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  const stateOf = (job: Job): CellState => cellState({ jobs: [job] });
  const time = (job: Job, state: CellState) =>
    state === "skipped" ? t("cvSkipped") : now && job.started ? duration(job.started, job.completed) : "";

  /** A curve from each box of a job to each box of the jobs that need it, measured once laid out. */
  async function draw() {
    await tick();
    if (!box) return;
    const at = box.getBoundingClientRect();
    const boxes = (key: string) => [...box!.querySelectorAll<HTMLElement>(`[data-key="${CSS.escape(key)}"]`)].map((e) => e.getBoundingClientRect());
    const found: { d: string; bad: boolean }[] = [];
    for (const cell of cols.flat()) {
      for (const need of cell.needs) {
        const parent = cols.flat().find((c) => c.key === need);
        const bad = parent ? cellState(parent) === "failed" : false;
        for (const from of boxes(need))
          for (const to of boxes(cell.key)) {
            const [x1, y1] = [from.right - at.left + box.scrollLeft, from.top + from.height / 2 - at.top];
            const [x2, y2] = [to.left - at.left + box.scrollLeft, to.top + to.height / 2 - at.top];
            const mid = (x1 + x2) / 2;
            found.push({ d: `M${x1} ${y1} C${mid} ${y1}, ${mid} ${y2}, ${x2} ${y2}`, bad });
          }
      }
    }
    [wires, size] = [found, { w: box.scrollWidth, h: box.scrollHeight }];
  }

  $effect(() => {
    void cols;
    void draw();
    if (!box) return;
    const watch = new ResizeObserver(() => void draw());
    watch.observe(box);
    return () => watch.disconnect();
  });
</script>

<div class="dag" bind:this={box} role="group" aria-label={t("cvGraph")}>
  <svg class="wires" width={size.w} height={size.h} aria-hidden="true">
    {#each wires as w, i (i)}<path d={w.d} class:bad={w.bad} />{/each}
  </svg>
  {#each cols as col, i (i)}
    <div class="col">
      {#each col as cell (cell.key)}
        {#if cell.jobs.length}
          {#each cell.jobs as job (job.id)}
            {@const state = stateOf(job)}
            {@const Icon = icons[state]}
            <button
              class="job {state}"
              data-key={cell.key}
              aria-pressed={selected === job.id}
              aria-label="{job.name}, {t(labels[state])}{job.started ? `, ${duration(job.started, job.completed)}` : ''}"
              title={job.name}
              onclick={() => onselect(job)}
            >
              <Icon size={15} aria-hidden="true" class={state === "running" ? "spin" : ""} />
              <span class="text"><span class="name">{job.name}</span><span class="d num">{time(job, state) || t(labels[state])}</span></span>
            </button>
          {/each}
        {:else}
          <div class="job waiting" data-key={cell.key} title={cell.name}>
            <CircleDashed size={15} aria-hidden="true" />
            <span class="text"><span class="name">{cell.name}</span><span class="d">{t("cvWaiting")}</span></span>
          </div>
        {/if}
      {/each}
    </div>
  {/each}
</div>

<style>
  .dag {
    position: relative;
    display: grid;
    grid-auto-columns: minmax(120px, max-content);
    grid-auto-flow: column;
    justify-content: space-between;
    gap: 40px;
    padding: 20px;
    overflow-x: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background-color: var(--well);
    background-image: radial-gradient(var(--border) 1px, transparent 1px);
    background-size: 16px 16px;
  }

  .wires {
    position: absolute;
    inset: 0 auto auto 0;
    overflow: visible;
    pointer-events: none;
  }

  path {
    fill: none;
    stroke: var(--border-strong);
    stroke-width: 1.5;
  }

  path.bad {
    stroke: var(--danger);
    stroke-opacity: 0.5;
  }

  .col {
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 12px;
  }

  .job {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    min-width: 0;
    padding: 5px 12px 5px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
    box-shadow: var(--shadow-1);
    color: var(--text);
    font: inherit;
    font-weight: 500;
    text-align: left;
    transition:
      border-color var(--dur-fast) var(--ease-out),
      box-shadow var(--dur-fast) var(--ease-out);
  }

  button.job {
    cursor: pointer;
  }

  button.job:hover {
    border-color: var(--border-strong);
  }

  button.job[aria-pressed="true"] {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.3;
  }

  .name {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .d {
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 400;
  }

  .passed :global(svg) {
    color: var(--done);
  }

  .failed {
    border-color: var(--danger-line);
    box-shadow: 0 0 0 3px var(--danger-soft);
  }

  .failed :global(svg) {
    color: var(--danger);
  }

  .running {
    border-color: var(--working);
  }

  .running :global(svg),
  .queued :global(svg) {
    color: var(--working);
  }

  .skipped,
  .waiting,
  .stopped {
    border-style: dashed;
    border-color: var(--border-strong);
    box-shadow: none;
    color: var(--muted);
  }
</style>
