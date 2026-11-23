<script module lang="ts">
  const fontFamily = '"JetBrains Mono Variable", Menlo, Consolas, monospace';
  const fontReady = document.fonts.load(`13px ${fontFamily}`).then(
    () => {},
    () => {},
  );
  type Modules = [typeof import("@xterm/xterm"), typeof import("@xterm/addon-fit"), typeof import("@xterm/addon-webgl")];
  let modules: Promise<Modules> | undefined;
  /**
   * xterm, loaded with the first terminal shown rather than at startup, and the font, since xterm measures its cells
   * when it opens: terminals open (and can take the focus) once this resolves.
   */
  export const terminalReady = () =>
    (modules ??= fontReady.then(() =>
      Promise.all([import("@xterm/xterm"), import("@xterm/addon-fit"), import("@xterm/addon-webgl")]),
    ));

  /** The terminals on screen, for what the page asks of them (the selection to copy from a context menu). */
  const shown = new Map<number, import("@xterm/xterm").Terminal>();
  export const selectionOf = (id: number) => shown.get(id)?.getSelection() ?? "";

  /** Dark in both app themes: agent TUIs pick their colours for a dark terminal. */
  const theme = {
    background: "#111216",
    foreground: "#dcdfe6",
    cursor: "#a3a5ff",
    cursorAccent: "#111216",
    selectionBackground: "#3a3c7a",
    black: "#2a2c33",
    red: "#f27a7a",
    green: "#80d197",
    yellow: "#e6c27a",
    blue: "#7aa6ff",
    magenta: "#c99cf2",
    cyan: "#70d0d6",
    white: "#d5d8de",
    brightBlack: "#7d8291",
    brightRed: "#ff9d9d",
    brightGreen: "#a3e2b3",
    brightYellow: "#f0d59c",
    brightBlue: "#a3c2ff",
    brightMagenta: "#dcbcf7",
    brightCyan: "#9be3e7",
    brightWhite: "#f4f5f7",
  };
</script>

<script lang="ts">
  import { Channel, invoke } from "@tauri-apps/api/core";
  import "@xterm/xterm/css/xterm.css";
  import { FONT } from "$lib/workspace-logic";
  import { zoom } from "$lib/workspace.svelte";

  /**
   * `visible`: off screen, xterm is let go (ARCHITECTURE §15): the output only fills the session's scrollback, which
   * the next xterm replays, back where the view and the selection were.
   */
  let { id, visible = true }: { id: number; visible?: boolean } = $props();
  let el: HTMLDivElement;
  let kept: { top: number; length: number; cols: number; selection?: [number, number, number] } | null = null;
  let resized: ((size: number) => void) | null = $state(null);
  const fontSize = $derived(zoom.fonts[id] ?? FONT.base);

  // ⌘+ ⌘− ⌘0 in this terminal: its cells are measured again, and the program told its new size.
  $effect(() => {
    const size = fontSize;
    resized?.(size);
  });

  function start([{ Terminal }, { FitAddon }, { WebglAddon }]: Modules): () => void {
    const term = new Terminal({
      fontFamily,
      fontSize,
      lineHeight: 1.1,
      cursorBlink: true,
      scrollback: 10_000,
      theme,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose());
      term.loadAddon(webgl);
    } catch {
      // No WebGL (some WebKitGTK setups): xterm keeps its DOM renderer.
    }

    // Replaying the history, xterm answers the queries in it again (cursor position, attributes, colours): those
    // answers must not reach the program as if typed.
    let replaying = true;
    const input = term.onData((data) => !replaying && invoke("pty_write", { id, data }).catch(() => {}));
    const resize = term.onResize(({ cols, rows }) =>
      invoke("pty_resize", { id, cols, rows }).catch(() => {}),
    );
    // Hidden tiles have no layout; fitting them would shrink the PTY to nothing.
    const fitShown = () => el.offsetParent !== null && fit.fit();
    // Before the replay, so the history lays out at the width it was written for.
    fitShown();
    const observer = new ResizeObserver(fitShown);
    observer.observe(el);
    shown.set(id, term);
    resized = (size) => {
      if (term.options.fontSize === size) return;
      term.options.fontSize = size;
      fitShown();
    };

    // Only onto the same history: once the program wrote meanwhile, or the width changed, the lines moved.
    const restore = () => {
      replaying = false;
      const buffer = term.buffer.active;
      if (kept && kept.length === buffer.length && kept.cols === term.cols) {
        term.scrollToLine(kept.top);
        if (kept.selection) term.select(...kept.selection);
      }
      kept = null;
    };
    // Live output can arrive before the scrollback snapshot resolves; hold it until the replay is written.
    const pending: Uint8Array[] = [];
    let replayed = false;
    let gone = false;
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (data) => {
      if (gone) return;
      const bytes = new Uint8Array(data);
      if (replayed) term.write(bytes);
      else pending.push(bytes);
    };
    invoke<ArrayBuffer>("pty_attach", { id, onData: channel }).then((snapshot) => {
      if (gone) return;
      term.write(new Uint8Array(snapshot), restore);
      pending.forEach((bytes) => term.write(bytes));
      replayed = true;
    });

    return () => {
      const buffer = term.buffer.active;
      const range = term.getSelectionPosition();
      kept = {
        top: buffer.viewportY,
        length: buffer.length,
        cols: term.cols,
        selection: range && [range.start.x, range.start.y, (range.end.y - range.start.y) * term.cols + range.end.x - range.start.x],
      };
      gone = true;
      resized = null;
      shown.delete(id);
      invoke("pty_detach", { id, view: channel.id }).catch(() => {});
      observer.disconnect();
      input.dispose();
      resize.dispose();
      term.dispose();
    };
  }

  $effect(() => {
    if (!visible) return;
    let stop: (() => void) | undefined;
    let gone = false;
    terminalReady().then((m) => {
      if (!gone) stop = start(m);
    });
    return () => {
      gone = true;
      stop?.();
    };
  });
</script>

<div class="term" data-term={id} bind:this={el}></div>

<style>
  .term {
    flex: 1;
    min-height: 0;
    padding: 6px 4px 4px 10px;
    background: var(--term-bg);
  }
</style>
