<script lang="ts" module>
  export type Family = "cto" | "product" | "build" | "quality" | "creative" | "specialists";

  // The groups of site/roles.toml: a role's color is its family's, as on the site.
  const families: Record<string, Family> = {
    cto: "cto",
    expert: "cto",
    pm: "product",
    po: "product",
    ux: "product",
    architect: "product",
    researcher: "product",
    "competitive-analyst": "product",
    "reverse-engineer": "product",
    lead: "build",
    front: "build",
    back: "build",
    devops: "build",
    seo: "build",
    qa: "quality",
    reviewer: "quality",
    secops: "quality",
    integrator: "quality",
    brand: "creative",
    copy: "creative",
    motion: "creative",
  };
  const marks: Record<string, string> = {
    cto: "CTO",
    researcher: "RES",
    "competitive-analyst": "CMP",
    "reverse-engineer": "REV",
    seo: "SEO",
    secops: "SEC",
    integrator: "IN",
    dba: "DBA",
    docs: "DOC",
    integrations: "INT",
    sre: "SRE",
    a11y: "A11Y",
    ai: "AI",
    analytics: "AN",
    legal: "LAW",
  };

  /** A role unknown to the site (one of the user's) is a specialist. */
  export const familyOf = (role: string): Family => families[role] ?? "specialists";
  export const markOf = (role: string) => marks[role] ?? role.slice(0, 2).toUpperCase();
</script>

<script lang="ts">
  import { toneOf, type Status } from "$lib/ui/StatusPill.svelte";

  /** A role's mark in its family's color; `status` adds the state's dot (`label` names the role and the state for a screen reader). */
  let {
    role,
    family,
    status,
    label,
    size = "md",
  }: { role: string; family?: Family; status?: Status; label?: string; size?: "sm" | "md" | "lg" } = $props();
</script>

<span
  class="avatar {size} {family ?? familyOf(role)}"
  role={label ? "img" : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : "true"}
  title={label}
>
  {markOf(role)}
  {#if status}<span class="dot {toneOf(status)}" data-status={status}></span>{/if}
</span>

<style>
  .avatar {
    position: relative;
    display: inline-grid;
    place-items: center;
    flex-shrink: 0;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--c-build);
    color: var(--c-ink);
    font: 700 10px / 1 var(--font-ui);
    letter-spacing: 0.02em;
  }

  .sm {
    width: 20px;
    height: 20px;
    border-radius: 5px;
    font-size: 8px;
  }

  .lg {
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    font-size: 11px;
  }

  .cto {
    background: var(--c-cto);
  }

  .product {
    background: var(--c-product);
  }

  .quality {
    background: var(--c-quality);
  }

  .creative {
    background: var(--c-creative);
  }

  .specialists {
    background: var(--c-specialists);
  }

  .dot {
    position: absolute;
    right: -3px;
    bottom: -3px;
    width: 10px;
    height: 10px;
    border: 2px solid var(--panel);
    border-radius: 50%;
    background: var(--muted);
  }

  .dot.working {
    background: var(--working);
  }

  .dot.waiting {
    background: var(--waiting);
  }

  .dot.done {
    background: var(--done);
  }

  .dot.failed {
    background: var(--danger);
  }
</style>
