<script lang="ts">
  import { t } from "$lib/i18n";
  import { phrase, type Act } from "$lib/live-logic";

  /** What an agent does in plain words, what a call is about as code: « Lance `npm test` ». */
  let { act }: { act: Pick<Act, "kind"> & { name?: string; input?: unknown } } = $props();

  const parts = $derived.by(() => {
    if (act.kind === "thinking") return { before: t("liveThinking"), target: "", after: "" };
    if (act.kind === "writing") return { before: t("liveWriting"), target: "", after: "" };
    const p = phrase(act.name ?? "", act.input);
    // The target goes in as a mark, so the phrase's own word order holds in every language.
    const [before, after = ""] = t(p.key as Parameters<typeof t>[0], { ...p.vars, target: "\u0000" }).split("\u0000");
    return { before, target: p.target ?? "", after };
  });
</script>

<span class="action" title={parts.before + parts.target + parts.after}
  >{parts.before}{#if parts.target}<code>{parts.target}</code>{/if}{parts.after}</span
>

<style>
  .action {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  code {
    padding: 0 4px;
    border-radius: var(--radius-sm);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: 0.92em;
  }
</style>
