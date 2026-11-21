<script lang="ts">
  import { onMount } from "svelte";
  import { Channel, invoke } from "@tauri-apps/api/core";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import "@xterm/xterm/css/xterm.css";

  let { id }: { id: number } = $props();
  let el: HTMLDivElement;

  onMount(() => {
    const term = new Terminal({
      fontFamily: '"JetBrains Mono", Menlo, Consolas, monospace',
      fontSize: 13,
      cursorBlink: true,
      scrollback: 10_000,
      theme: {
        background: "#0f1115",
        foreground: "#d8dee9",
        cursor: "#8ab4f8",
        selectionBackground: "#2f3a4d",
      },
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
  });
</script>

<div class="term" bind:this={el}></div>

<style>
  .term {
    flex: 1;
    min-height: 0;
    padding: 4px 6px;
  }
</style>
