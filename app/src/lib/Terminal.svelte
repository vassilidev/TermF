<script module lang="ts">
  const fontFamily = '"JetBrains Mono Variable", Menlo, Consolas, monospace';
  // xterm measures its cells when it opens: the bundled font must be there by then.
  const fontReady = document.fonts.load(`13px ${fontFamily}`).catch(() => {});

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
  import { onMount } from "svelte";
  import { Channel, invoke } from "@tauri-apps/api/core";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import "@xterm/xterm/css/xterm.css";

  let { id }: { id: number } = $props();
  let el: HTMLDivElement;

  function start(): () => void {
    const term = new Terminal({
      fontFamily,
      fontSize: 13,
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

    // Live output can arrive before the scrollback snapshot resolves; hold it until the replay is written.
    const pending: Uint8Array[] = [];
    let replayed = false;
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (data) => {
      const bytes = new Uint8Array(data);
      if (replayed) term.write(bytes);
      else pending.push(bytes);
    };
    invoke<ArrayBuffer>("pty_attach", { id, onData: channel }).then((snapshot) => {
      term.write(new Uint8Array(snapshot));
      pending.forEach((bytes) => term.write(bytes));
      replayed = true;
    });

    const input = term.onData((data) => invoke("pty_write", { id, data }).catch(() => {}));
    const resize = term.onResize(({ cols, rows }) =>
      invoke("pty_resize", { id, cols, rows }).catch(() => {}),
    );
    // Hidden tiles have no layout; fitting them would shrink the PTY to nothing.
    const observer = new ResizeObserver(() => {
      if (el.offsetParent !== null) fit.fit();
    });
    observer.observe(el);

    return () => {
      observer.disconnect();
      input.dispose();
      resize.dispose();
      term.dispose();
    };
  }

  onMount(() => {
    let stop: (() => void) | undefined;
    let gone = false;
    fontReady.then(() => {
      if (!gone) stop = start();
    });
    return () => {
      gone = true;
      stop?.();
    };
  });
</script>

<div class="term" bind:this={el}></div>

<style>
  .term {
    flex: 1;
    min-height: 0;
    padding: 6px 4px 4px 10px;
    background: var(--term-bg);
  }
</style>
