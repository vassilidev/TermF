<script lang="ts">
  import type { Component, Snippet } from "svelte";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import { t } from "$lib/i18n";

  /**
   * Nothing to show, and what to do about it: a sentence, one `action`, and asking the AI second (`onask`, `askLabel`
   * to name the role). `children` stays for a screen's own content (an error and its retry, a form).
   */
  let {
    icon: Icon,
    title = "",
    text = "",
    action,
    onask,
    askLabel,
    children,
  }: {
    icon?: Component;
    title?: string;
    text?: string;
    action?: { label: string; icon?: Component; run: () => void };
    onask?: () => void;
    askLabel?: string;
    children?: Snippet;
  } = $props();
</script>

<div class="empty">
  {#if Icon}<span class="mark"><Icon size={18} aria-hidden="true" /></span>{/if}
  {#if title}<h2>{title}</h2>{/if}
  {#if text}<p>{text}</p>{/if}
  {@render children?.()}
  {#if action || onask}
    <div class="actions">
      {#if action}
        <button class="btn primary" onclick={action.run}>
          {#if action.icon}<action.icon size={14} aria-hidden="true" />{/if}{action.label}
        </button>
      {/if}
      {#if onask}
        <button class="btn quiet" onclick={onask}><Sparkles size={14} aria-hidden="true" />{askLabel ?? t("uiF1AskAi")}</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    max-width: 480px;
    margin: 48px auto 0;
    color: var(--muted);
    text-align: center;
  }

  .mark {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    margin-bottom: 4px;
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  h2 {
    margin: 8px 0 4px;
    color: var(--text);
    font-size: var(--text-md);
    font-weight: 600;
  }

  p {
    max-width: 44ch;
    margin: 0;
  }

  .empty > :global(.btn),
  .actions {
    margin-top: 16px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
  }
</style>
