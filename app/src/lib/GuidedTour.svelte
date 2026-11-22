<script lang="ts">
  import { tick, untrack } from "svelte";
  import { isMac } from "$lib/files.svelte";
  import { t } from "$lib/i18n";
  import { action } from "$lib/keymap-logic";
  import { shortcutOf } from "$lib/keymap.svelte";
  import { endTour, tour } from "$lib/tour.svelte";

  type Key = Parameters<typeof t>[0];
  type Step = { at: string; title: Key; text: Key; keys: string[] };

  /** Each step points at the navigation, where the thing it teaches lives; the shortcuts come from the registry. */
  const steps: Step[] = [
    { at: ".nav .switcher", title: "t6bTour1Title", text: "t6bTour1", keys: ["switch_project", "project"] },
    { at: '[data-view="terminals"]', title: "t6bTour2Title", text: "t6bTour2", keys: ["new_session", "split_right", "split_down"] },
    { at: ".nav .find", title: "t6bTour3Title", text: "t6bTour3", keys: ["next_waiting", "palette"] },
    { at: '[data-view="tickets"]', title: "viewTickets", text: "t6bTour4", keys: ["launch_ticket"] },
    { at: '[data-view="conversations"]', title: "t6bTour5Title", text: "t6bTour5", keys: [] },
    { at: '[data-view="files"]', title: "t6bTour6Title", text: "t6bTour6", keys: ["split_view", "other_panel"] },
    { at: '[data-view="settings"]', title: "t6bTour7Title", text: "t6bTour7", keys: ["help"] },
  ];
  const mod = isMac ? "⌘" : "Ctrl";

  let card = $state<HTMLElement>();
  let next = $state<HTMLButtonElement>();
  /** The target's box, null when it is not on screen. */
  let box = $state<DOMRect | null>(null);
  let top = $state(0);
  /** Beside the navigation, whatever the target's own width. */
  let left = $state(0);

  const step = $derived(steps[tour.step ?? 0]);
  const last = $derived(tour.step === steps.length - 1);

  function place() {
    const target = document.querySelector<HTMLElement>(step.at);
    box = target?.getClientRects().length ? target.getBoundingClientRect() : null;
    left = Math.max(box?.right ?? 0, target?.closest("nav")?.getBoundingClientRect().right ?? 0) + 14;
    const height = card?.offsetHeight ?? 0;
    const middle = box ? box.top + box.height / 2 : innerHeight / 2;
    top = Math.max(12, Math.min(innerHeight - height - 12, middle - height / 2));
  }

  $effect(() => {
    void tour.step;
    untrack(place);
    tick().then(() => {
      place();
      next?.focus();
    });
    addEventListener("resize", place);
    return () => removeEventListener("resize", place);
  });

  function go(by: number) {
    const to = (tour.step ?? 0) + by;
    if (to >= steps.length) endTour();
    else if (to >= 0) tour.step = to;
  }

  function onkeydown(e: KeyboardEvent) {
    const by = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
    if (e.key === "Escape") endTour();
    else if (by) go(by);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

{#if box}
  <div
    class="spot"
    style:left="{box.left - 4}px"
    style:top="{box.top - 4}px"
    style:width="{box.width + 8}px"
    style:height="{box.height + 8}px"
    aria-hidden="true"
  ></div>
{/if}

<!-- Not modal: the app stays usable, the tour only points. -->
<div
  bind:this={card}
  class="card"
  class:floating={!box}
  role="dialog"
  aria-labelledby="tour-title"
  aria-describedby="tour-text"
  tabindex="-1"
  style:left={box ? `${left}px` : undefined}
  style:top="{top}px"
  {onkeydown}
>
  {#if box}<span class="arrow" style:top="{Math.max(14, box.top + box.height / 2 - top - 6)}px" aria-hidden="true"></span>{/if}
  <h2 id="tour-title">{t(step.title)}</h2>
  <p id="tour-text">{t(step.text, { key: mod })}</p>
  {#if step.keys.length}
    <dl>
      {#each step.keys as id (id)}
        {@const a = action(id)}
        {@const keys = shortcutOf(id)}
        {#if a && keys.length}
          <dt>{#each keys as key (key)}<kbd>{key}</kbd>{/each}</dt>
          <dd>{t(a.label)}</dd>
        {/if}
      {/each}
    </dl>
  {/if}
  <footer>
    <span class="count">{t("t6bTourStep", { n: (tour.step ?? 0) + 1, total: steps.length })}</span>
    <button class="btn sm quiet" onclick={endTour}>{t("t6bTourSkip")}</button>
    {#if tour.step}
      <button class="btn sm" onclick={() => go(-1)}>{t("t6bTourBack")}</button>
    {/if}
    <button bind:this={next} class="btn sm primary" onclick={() => go(1)}>
      {t(last ? "t6bTourDone" : "t6bTourNext")}
    </button>
  </footer>
</div>

<style>
  /* Dims everything but the target, which stays clickable. */
  .spot {
    position: fixed;
    z-index: 40;
    border-radius: var(--radius-lg);
    box-shadow:
      0 0 0 2px var(--accent),
      0 0 0 100vmax rgb(10 12 20 / 0.32);
    pointer-events: none;
    transition:
      left var(--dur-slow) var(--ease-out),
      top var(--dur-slow) var(--ease-out),
      width var(--dur-slow) var(--ease-out),
      height var(--dur-slow) var(--ease-out);
  }

  .card {
    position: fixed;
    z-index: 41;
    width: 340px;
    padding: 16px 16px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--raised);
    box-shadow: var(--shadow-3);
    transition: top var(--dur-slow) var(--ease-out);
  }

  .card.floating {
    left: calc(50% - 170px);
  }

  .card:focus {
    outline: none;
  }

  .arrow {
    position: absolute;
    left: -7px;
    width: 12px;
    height: 12px;
    border-bottom: 1px solid var(--border);
    border-left: 1px solid var(--border);
    background: var(--raised);
    transform: rotate(45deg);
  }

  h2 {
    margin: 0 0 6px;
    font-size: var(--text-md);
    font-weight: 650;
  }

  p {
    margin: 0;
    color: var(--muted);
    line-height: 1.55;
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 10px;
    align-items: center;
    margin: 12px 0 0;
  }

  dt {
    display: flex;
    gap: 4px;
  }

  dd {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  footer {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 16px;
  }

  .count {
    margin-right: auto;
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }
</style>
