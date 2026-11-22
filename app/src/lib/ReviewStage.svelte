<script lang="ts">
  import { untrack } from "svelte";
  import Minus from "@lucide/svelte/icons/minus";
  import Plus from "@lucide/svelte/icons/plus";
  import Maximize from "@lucide/svelte/icons/maximize";
  import Scan from "@lucide/svelte/icons/scan";
  import { t } from "$lib/i18n";
  import { image } from "$lib/visual.svelte";
  import { clamp, fit, focus, toImage, zoomAt, type Mode, type Pin, type Review, type Screen, type Version, type View } from "$lib/visual-logic";

  /**
   * A screen's captures in one of the modes, every frame on the same zoom and position. Boxes mark what changed; pins
   * sit on the "after" image, in its pixels. `free`: panning without zooming first (the page; the quick mode leaves the
   * gestures to its card until zoomed); `controls`: the zoom bar, and the keys on the focused stage.
   */
  let {
    review,
    version,
    screen,
    mode,
    before = false,
    contain = false,
    free = true,
    controls = true,
    pins = [],
    picked = null,
    onpin,
  }: {
    review: Review;
    version: Version;
    screen: Screen;
    mode: Mode;
    before?: boolean;
    contain?: boolean;
    free?: boolean;
    controls?: boolean;
    pins?: Pin[];
    picked?: number | null;
    onpin?: (x: number, y: number) => void;
  } = $props();

  type Kind = "before" | "after" | "heatmap";
  let urls = $state<Partial<Record<Kind, string>>>({});
  let failed = $state<Partial<Record<Kind, string>>>({});
  let size = $state({ w: 0, h: 0 });
  let frameW = $state(0);
  let frameH = $state(0);
  /** Null: fitted to the frame. */
  let own = $state<View | null>(null);
  let split = $state(0.5);
  let alpha = $state(0.5);
  let pinFrame = $state<HTMLElement>();
  const uid = `stage-${Math.random().toString(36).slice(2)}`;

  const key = $derived(`${review.id}/${version.n}/${screen.id}`);

  $effect(() => {
    void key;
    let live = true;
    untrack(() => {
      urls = {};
      failed = {};
      own = null;
      size = { w: screen.width, h: screen.height };
      for (const kind of ["before", "after", "heatmap"] as const) {
        if (!screen[kind]) continue;
        image(review.id, version.n, screen.id, kind).then(
          (url) => live && (urls[kind] = url),
          (e) => live && (failed[kind] = String(e)),
        );
      }
    });
    return () => (live = false);
  });

  const base = $derived(fit(frameW, frameH, size.w, size.h, contain));
  const changes = $derived(focus(screen.rects, frameW, frameH, size.w, size.h, base, contain));
  // The page opens on what changed; the quick mode's card shows the whole screen, its zoom goes to the changes.
  const view = $derived(own ?? (contain ? base : (changes ?? base)));
  const percent = $derived(Math.round(view.scale * 100));
  const whole = $derived(view.scale === base.scale && view.x === base.x && view.y === base.y);

  function set(v: View) {
    own = clamp(v, frameW, frameH, size.w, size.h, contain);
  }

  export function zoom(factor: number, px = frameW / 2, py = frameH / 2) {
    set(zoomAt(view, px, py, factor));
  }

  export function reset() {
    own = null;
  }

  export const zoomed = () => own !== null;

  export function toggleZoom() {
    if (own) reset();
    else if (changes) own = changes;
    else zoom(2);
  }

  /** A point of the screen, in pixels of the "after" image, or null off it. */
  export function pointAt(clientX: number, clientY: number) {
    const r = pinFrame?.getBoundingClientRect();
    return r ? toImage(view, clientX - r.left, clientY - r.top, size.w, size.h) : null;
  }

  /** Where a pin goes from the keyboard: the middle of what the frame shows. */
  export function center() {
    return toImage(view, frameW / 2, frameH / 2, size.w, size.h);
  }

  function loaded(e: Event) {
    const img = e.currentTarget as HTMLImageElement;
    size = { w: img.naturalWidth, h: img.naturalHeight };
  }

  /** Drags the images along; a click that did not move pins a comment there. */
  function down(e: PointerEvent, pinnable: boolean) {
    if (e.button !== 0 || !(free || own)) return;
    e.stopPropagation();
    const frame = e.currentTarget as HTMLElement;
    frame.setPointerCapture(e.pointerId);
    const [sx, sy, start] = [e.clientX, e.clientY, view];
    let moved = false;
    const move = (m: PointerEvent) => {
      const [dx, dy] = [m.clientX - sx, m.clientY - sy];
      if (!moved && Math.hypot(dx, dy) > 4) moved = true;
      if (moved) set({ ...start, x: start.x + dx, y: start.y + dy });
    };
    const up = (u: PointerEvent) => {
      frame.removeEventListener("pointermove", move);
      frame.removeEventListener("pointerup", up);
      frame.removeEventListener("pointercancel", up);
      if (moved || u.type !== "pointerup" || !pinnable || !onpin) return;
      const r = frame.getBoundingClientRect();
      const at = toImage(view, u.clientX - r.left, u.clientY - r.top, size.w, size.h);
      if (at) onpin(at.x, at.y);
    };
    frame.addEventListener("pointermove", move);
    frame.addEventListener("pointerup", up);
    frame.addEventListener("pointercancel", up);
  }

  /** Pinch (or ⌘ + wheel) zooms; the wheel pans once panning is the stage's, else it goes on to the card. */
  function wheel(node: HTMLElement) {
    const on = (e: WheelEvent) => {
      const r = node.getBoundingClientRect();
      if (e.ctrlKey || e.metaKey) zoom(Math.exp(-e.deltaY * 0.01), e.clientX - r.left, e.clientY - r.top);
      else if (free || own) set({ ...view, x: view.x - e.deltaX, y: view.y - e.deltaY });
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    node.addEventListener("wheel", on, { passive: false });
    return () => node.removeEventListener("wheel", on);
  }

  function keys(e: KeyboardEvent) {
    const moves: Record<string, [number, number]> = { ArrowLeft: [80, 0], ArrowRight: [-80, 0], ArrowUp: [0, 80], ArrowDown: [0, -80] };
    const move = moves[e.key];
    if (move) set({ ...view, x: view.x + move[0], y: view.y + move[1] });
    else if (e.key === "+" || e.key === "=") zoom(1.25);
    else if (e.key === "-") zoom(0.8);
    else if (e.key === "0") reset();
    else if (e.key === "Enter" && onpin) {
      const at = center();
      if (at) onpin(at.x, at.y);
    } else return;
    e.preventDefault();
  }

  function dragSplit(e: PointerEvent) {
    e.stopPropagation();
    const handle = e.currentTarget as HTMLElement;
    const frame = handle.parentElement!;
    handle.setPointerCapture(e.pointerId);
    const move = (m: PointerEvent) => {
      const r = frame.getBoundingClientRect();
      split = Math.min(1, Math.max(0, (m.clientX - r.left) / r.width));
    };
    const end = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", end);
      handle.removeEventListener("pointercancel", end);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
  }

  const missing = (kind: Kind) =>
    failed[kind] ?? (!screen[kind] && screen.error ? t("review5bScreenError", { error: screen.error }) : !screen[kind] ? t("review5bNoImage") : null);
</script>

{#snippet picture(kind: Kind, label: string, opacity = 1)}
  {#if urls[kind]}
    <img
      src={urls[kind]}
      alt="{label} · {screen.path}"
      draggable="false"
      onload={kind !== "before" || !screen.after ? loaded : undefined}
      style:opacity
      style:transform="translate({view.x}px, {view.y}px) scale({view.scale})"
    />
  {:else}
    {@const issue = missing(kind)}
    <p class="wait" class:issue={!!issue}>{issue ?? t("review5bLoading")}</p>
  {/if}
{/snippet}

{#snippet marks(withPins: boolean)}
  {#each screen.rects as r, i (i)}
    <span
      class="box"
      style:left="{view.x + r.x * view.scale}px"
      style:top="{view.y + r.y * view.scale}px"
      style:width="{r.w * view.scale}px"
      style:height="{r.h * view.scale}px"
    ></span>
  {/each}
  {#if withPins}
    {#each pins as pin, i (i)}
      <span class="pin" class:picked={picked === i} style:left="{view.x + pin.x * view.scale}px" style:top="{view.y + pin.y * view.scale}px"
        >{i + 1}</span
      >
    {/each}
  {/if}
{/snippet}

<div class="stage">
  {#if controls}
    <p id="{uid}-keys" class="sr-only">{t("review5bStageKeys")}</p>
    <div class="bar">
      {#if mode === "slider"}
        <label class="range">
          <span>{t("review5bSliderPos")}</span>
          <input type="range" min="0" max="100" value={Math.round(split * 100)} oninput={(e) => (split = Number(e.currentTarget.value) / 100)} />
        </label>
      {:else if mode === "overlay"}
        <label class="range">
          <span>{t("review5bOpacity")}</span>
          <input type="range" min="0" max="100" value={Math.round(alpha * 100)} oninput={(e) => (alpha = Number(e.currentTarget.value) / 100)} />
        </label>
      {/if}
      <div class="zoom" role="group" aria-label={t("review5bZoom")}>
        <button class="icon-btn sm" onclick={() => zoom(0.8)} aria-label={t("review5bZoomOut")} title={t("review5bZoomOut")}>
          <Minus size={14} aria-hidden="true" />
        </button>
        <span class="mono" aria-live="polite">{t("review5bZoomLevel", { percent })}</span>
        <button class="icon-btn sm" onclick={() => zoom(1.25)} aria-label={t("review5bZoomIn")} title={t("review5bZoomIn")}>
          <Plus size={14} aria-hidden="true" />
        </button>
        {#if changes}
          <button class="btn sm quiet" aria-pressed={!own} onclick={reset}><Scan size={14} aria-hidden="true" />{t("review5bFocus")}</button>
        {/if}
        <button class="btn sm quiet" aria-pressed={whole} onclick={() => (own = base)}><Maximize size={14} aria-hidden="true" />{t("review5bFit")}</button>
      </div>
    </div>
    {#if mode !== "heatmap"}
      <div class="captions" class:two={mode === "side"} aria-hidden="true">
        {#if mode === "side"}
          <span>{t("review5bBefore")}</span><span>{t("review5bAfter")}</span>
        {:else if mode === "slider"}
          <span class="ends"><span>← {t("review5bBefore")}</span><span>{t("review5bAfter")} →</span></span>
        {:else}
          <span>{t("review5bOverlayBadge", { percent: Math.round(alpha * 100) })}</span>
        {/if}
      </div>
    {/if}
  {/if}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (the keys pan, zoom and pin: a focusable picture) -->
  <div
    class="frames"
    class:two={mode === "side"}
    role="group"
    aria-label={t("review5bStage", { path: screen.path })}
    aria-describedby={controls ? `${uid}-keys` : undefined}
    tabindex={controls ? 0 : undefined}
    onkeydown={controls ? keys : undefined}
  >
    {#if mode === "side"}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="frame" class:grab={free || own} bind:clientWidth={frameW} bind:clientHeight={frameH} onpointerdown={(e) => down(e, false)} {@attach wheel}>
        {@render picture("before", t("review5bBefore"))}
        {@render marks(false)}
      </div>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="frame" class:grab={free || own} bind:this={pinFrame} onpointerdown={(e) => down(e, true)} {@attach wheel}>
        {@render picture("after", t("review5bAfter"))}
        {@render marks(true)}
      </div>
    {:else}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="frame"
        class:grab={free || own}
        bind:this={pinFrame}
        bind:clientWidth={frameW}
        bind:clientHeight={frameH}
        onpointerdown={(e) => down(e, true)}
        {@attach wheel}
      >
        {#if mode === "heatmap"}
          {@render picture("heatmap", t("review5bHeatmap"))}
        {:else if mode === "single"}
          {@render picture(before ? "before" : "after", t(before ? "review5bBefore" : "review5bAfter"))}
        {:else}
          {@render picture("before", t("review5bBefore"))}
          {#if mode === "slider"}
            <div class="clip" style:clip-path="inset(0 0 0 {split * 100}%)">{@render picture("after", t("review5bAfter"))}</div>
          {:else}
            {@render picture("after", t("review5bAfter"), alpha)}
          {/if}
        {/if}
        {@render marks(!(mode === "single" && before))}
        {#if mode === "slider"}
          <!-- svelte-ignore a11y_no_static_element_interactions (the range above is its keyboard twin) -->
          <span class="handle" style:left="{split * 100}%" onpointerdown={dragSplit}></span>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .stage {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    min-height: 0;
    height: 100%;
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 12px;
    min-height: 28px;
  }

  .zoom {
    display: flex;
    align-items: center;
    gap: 2px;
    font-size: var(--text-sm);
  }

  .zoom .mono {
    min-width: 48px;
    color: var(--muted);
    text-align: center;
  }

  .range {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-right: auto;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .range input {
    width: 110px;
    accent-color: var(--accent);
  }

  .frames {
    display: grid;
    flex: 1;
    grid-template: minmax(0, 1fr) / minmax(0, 1fr);
    gap: 8px;
    min-height: 0;
    border-radius: var(--radius);
  }

  .frames.two {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }

  .frames:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }

  .frame {
    position: relative;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    touch-action: none;
    user-select: none;
  }

  .frame.grab {
    cursor: grab;
  }

  .frame.grab:active {
    cursor: grabbing;
  }

  img,
  .clip {
    position: absolute;
    top: 0;
    left: 0;
  }

  img {
    max-width: none;
    transform-origin: 0 0;
    pointer-events: none;
  }

  .clip {
    inset: 0;
  }

  .wait {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    margin: 0;
    padding: 16px;
    color: var(--muted);
    font-size: var(--text-sm);
    text-align: center;
  }

  .wait.issue {
    color: var(--danger);
  }

  /* What changed: drawn over any page, light or dark, so a ring of each. */
  .box {
    position: absolute;
    border: 2px solid var(--accent);
    border-radius: 3px;
    box-shadow:
      0 0 0 1px rgb(255 255 255 / 0.9),
      inset 0 0 0 1px rgb(255 255 255 / 0.6);
    pointer-events: none;
  }

  .pin {
    position: absolute;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin: -12px 0 0 -12px;
    border: 2px solid #fff;
    border-radius: 50%;
    background: var(--waiting-ring);
    color: #fff;
    font-size: var(--text-xs);
    font-weight: 700;
    box-shadow: var(--shadow-2);
    pointer-events: none;
  }

  .pin.picked {
    outline: 3px solid var(--focus);
    outline-offset: 1px;
  }

  .captions {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 8px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .captions.two {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }

  .ends {
    display: flex;
    justify-content: space-between;
  }

  .handle {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--accent);
    cursor: ew-resize;
  }

  .handle::before {
    content: "";
    position: absolute;
    inset: 0 -10px;
  }

  .handle::after {
    content: "";
    position: absolute;
    top: 50%;
    left: 50%;
    width: 22px;
    height: 22px;
    margin: -11px 0 0 -11px;
    border: 2px solid #fff;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: var(--shadow-2);
  }
</style>
