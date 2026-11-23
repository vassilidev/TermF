<script lang="ts">
  /** What a command printed, as a terminal shows it: the command first when known, the lines that say what broke marked. */
  let { text, command }: { text: string; command?: string } = $props();

  const BROKE = /\b(fail(ed|ure)?|error|assert(ion)?error|panic(ked)?)\b|✗|×/i;
</script>

<pre class="out">{#if command}<span class="line cmd">$ {command}</span>{/if}{#each text.split("\n") as line, i (i)}<span class="line" class:broke={BROKE.test(line)}>{line || " "}</span>{/each}</pre>

<style>
  .out {
    max-height: 220px;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    /* In the dark theme the terminal and the page are close: its edge keeps it a block. */
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--term-bg);
    color: #e4e4ea;
    font: 12px / 1.6 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .line {
    display: block;
  }

  .cmd {
    color: #9a9aab;
  }

  .broke {
    margin: 0 -12px;
    padding: 0 12px;
    background: rgb(255 129 120 / 0.14);
    box-shadow: inset 2px 0 0 #ff8178;
    color: #ffa59f;
  }
</style>
