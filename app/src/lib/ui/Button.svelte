<script lang="ts">
  import type { Component, Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";

  /**
   * The `.btn` of app.css with its states: `loading` keeps the width and the label (read by a screen reader) under a
   * spinner, and blocks a second click; `kbd` shows the shortcut inside the button. One `primary` per screen.
   */
  let {
    variant = "secondary",
    size = "md",
    loading = false,
    kbd,
    icon: Icon,
    disabled = false,
    type = "button",
    children,
    ...rest
  }: Omit<HTMLButtonAttributes, "children"> & {
    variant?: "primary" | "secondary" | "quiet" | "danger" | "quiet-danger" | "link";
    size?: "sm" | "md" | "lg";
    loading?: boolean;
    kbd?: string;
    icon?: Component;
    children?: Snippet;
  } = $props();

  const classes = $derived(
    ["btn", variant === "quiet-danger" ? "quiet danger" : variant === "secondary" ? "" : variant, size === "md" ? "" : size]
      .filter(Boolean)
      .join(" "),
  );
</script>

<button
  {...rest}
  {type}
  class="{classes} {rest.class ?? ''}"
  class:loading
  disabled={disabled || loading}
  aria-busy={loading || undefined}
>
  <span class="content">
    {#if Icon}<Icon size={size === "sm" ? 13 : 14} aria-hidden="true" />{/if}
    {@render children?.()}
    {#if kbd}<kbd>{kbd}</kbd>{/if}
  </span>
  {#if loading}<LoaderCircle class="spinner spin" size={14} aria-hidden="true" />{/if}
</button>

<style>
  .content {
    display: contents;
  }

  /* The label stays laid out (the button keeps its width) and named for a screen reader; only the spinner shows. */
  .loading .content {
    display: inline-flex;
    align-items: center;
    gap: inherit;
    opacity: 0;
  }

  .loading:disabled {
    opacity: 1;
    cursor: progress;
  }

  .btn :global(.spinner) {
    position: absolute;
    inset: 0;
    margin: auto;
  }
</style>
