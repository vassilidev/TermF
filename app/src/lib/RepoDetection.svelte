<script lang="ts" module>
  import type { RepoRecipe } from "$lib/onboarding-logic";
  import type { Remote } from "$lib/setup-logic";

  export type Detected = {
    facts: {
      stack: string[];
      workflows: string[];
      instructions: string[];
      claude: string[];
      workspace: string | null;
      parts: { path: string; name: string; role: string; web: boolean }[];
      tests: string[];
      infra: string[];
      signals: Record<string, string[]>;
    };
    tags: { name: string; version: string | null; source: string }[];
    scripts: { kind: string; run: string }[];
    recipe: RepoRecipe;
    remote: Remote | null;
    keys: string[];
  };
</script>

<script lang="ts">
  import Bot from "@lucide/svelte/icons/bot";
  import BookOpen from "@lucide/svelte/icons/book-open";
  import Boxes from "@lucide/svelte/icons/boxes";
  import Database from "@lucide/svelte/icons/database";
  import FlaskConical from "@lucide/svelte/icons/flask-conical";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import Globe from "@lucide/svelte/icons/globe";
  import Layers from "@lucide/svelte/icons/layers";
  import Package from "@lucide/svelte/icons/package";
  import Plug from "@lucide/svelte/icons/plug";
  import Server from "@lucide/svelte/icons/server";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import Ticket from "@lucide/svelte/icons/ticket";
  import Workflow from "@lucide/svelte/icons/workflow";
  import type { Component } from "svelte";
  import ForgeIcon from "$lib/ui/ForgeIcon.svelte";
  import { t } from "$lib/i18n";
  import type { Card } from "$lib/setup-logic";
  import { shortPath } from "$lib/state.svelte";

  /** What TermF found in one repository, only the rows it found something for; skeleton lines while it reads. */
  let { card, found }: { card: Card; found: Detected | string | undefined } = $props();

  type Key = Parameters<typeof t>[0];
  type Row = { icon: Component; label: Key; chips?: string[]; codes?: [string, string][]; text?: string[] };

  const rows = $derived.by((): Row[] => {
    if (!found || typeof found === "string") return [];
    const f = found.facts;
    const s = f.signals;
    const managers = [...new Set(f.stack.filter((x) => x.includes(":")).map((x) => x.split(":")[1]))];
    const sdk = [
      s.llm?.length ? t("setupSdkLlm", { list: s.llm.join(", ") }) : "",
      s.analytics?.length ? t("setupSdkAnalytics", { list: s.analytics.join(", ") }) : "",
      s.integrations?.length ? t("setupSdkIntegrations", { list: s.integrations.join(", ") }) : "",
    ].filter(Boolean);
    const web = f.parts.filter((p) => p.web).map((p) => p.path || card.name);
    const tickets = [
      found.remote?.host === "github.com" ? t("setupIssues", { repo: found.remote.slug }) : "",
      found.keys.length ? t("setupKeys", { keys: found.keys.map((k) => `${k}-…`).join(", ") }) : "",
    ].filter(Boolean);
    const list: (Row | false)[] = [
      { icon: Layers, label: "setupDetStack", chips: found.tags.map((g) => (g.version ? `${g.name} ${g.version}` : g.name)) },
      (managers.length > 0 || !!f.workspace) && {
        icon: Package,
        label: "setupDetPackages",
        text: [managers.join(", "), f.workspace ? t("setupWorkspace", { tool: f.workspace }) : ""].filter(Boolean),
      },
      found.scripts.length > 0 && { icon: SquareTerminal, label: "setupDetCommands", codes: found.scripts.map((x) => [x.kind, x.run]) },
      f.workflows.length > 0 && { icon: Workflow, label: "setupDetCi", text: [t("setupGithubActions", { list: f.workflows.join(", ") })] },
      tickets.length > 0 && { icon: Ticket, label: "setupDetTickets", text: tickets },
      f.parts.length > 1 && {
        icon: Boxes,
        label: "setupDetParts",
        chips: f.parts.map((p) => `${p.path || card.name} · ${p.role}`),
      },
      web.length > 0 && { icon: Globe, label: "setupDetWeb", text: [web.join(", ")] },
      !!s.data?.length && { icon: Database, label: "setupDetData", text: [s.data.join(", ")] },
      sdk.length > 0 && { icon: Plug, label: "setupDetSdk", text: sdk },
      f.tests.length > 0 && { icon: FlaskConical, label: "setupDetTests", text: [f.tests.join(", ")] },
      f.infra.length > 0 && { icon: Server, label: "setupDetInfra", text: [f.infra.join(", ")] },
      !!s.docs?.length && { icon: BookOpen, label: "setupDetDocs", text: [s.docs.join(", ")] },
      f.instructions.length + f.claude.length > 0 && { icon: Bot, label: "setupDetAgents", text: [[...f.instructions, ...f.claude].join(", ")] },
    ];
    return list.filter((r): r is Row => !!r);
  });
</script>

<article class="detect" aria-busy={found === undefined}>
  <header>
    <span class="forge"><ForgeIcon host={card.remote?.host} size={15} /></span>
    <b>{card.name}</b>
    <span class="where mono" title={card.path}>{card.remote?.slug ?? shortPath(card.path)}</span>
    {#if card.branch}
      <span class="branch"><GitBranch size={12} aria-hidden="true" /><span class="mono">{card.branch}</span></span>
    {/if}
  </header>
  {#if found === undefined}
    <p class="sr-only" role="status">{t("setupDetecting", { name: card.name })}</p>
    <div class="skeleton" aria-hidden="true">
      {#each [72, 56, 64, 40] as w, i (i)}<span class="bar" style:width="{w}%"></span>{/each}
    </div>
  {:else if typeof found === "string"}
    <p class="notice error" role="alert">{found}</p>
  {:else}
    <dl>
      {#each rows as row (row.label)}
        <dt><row.icon size={14} aria-hidden="true" />{t(row.label)}</dt>
        <dd>
          {#if row.chips}
            {#each row.chips as chip (chip)}<span class="badge">{chip}</span>{:else}<span class="muted">{t("obNothing")}</span>{/each}
          {:else if row.codes}
            {#each row.codes as [kind, run] (kind)}<span class="cmd"><span class="kind">{kind}</span><code>{run}</code></span>{/each}
          {:else if row.text}
            {#each row.text as line, i (i)}<span>{line}</span>{/each}
          {/if}
        </dd>
      {/each}
    </dl>
  {/if}
</article>

<style>
  .detect {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    animation: arrive var(--dur-slow) var(--ease-out);
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .forge {
    display: inline-flex;
    color: var(--text);
  }

  header b {
    font-size: var(--text-md);
    font-weight: 600;
    white-space: nowrap;
  }

  .where {
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .branch {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
    color: var(--muted);
  }

  dl {
    display: grid;
    grid-template-columns: 168px minmax(0, 1fr);
    gap: 10px 16px;
    margin: 0;
    padding: 14px 16px 16px;
  }

  dt {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 20px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  dd {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 6px;
    min-width: 0;
    margin: 0;
    line-height: 20px;
    overflow-wrap: anywhere;
  }

  dd > span:not(.badge, .cmd) + span:not(.badge, .cmd)::before {
    content: "·";
    margin-right: 6px;
    color: var(--muted);
  }

  .cmd {
    display: inline-flex;
    align-items: center;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
  }

  .kind {
    padding: 1px 6px;
    background: var(--hover);
    color: var(--muted);
  }

  code {
    padding: 1px 6px;
    font-family: var(--font-mono);
  }

  .skeleton {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
  }

  .bar {
    height: 12px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    animation: pulse 1.4s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .detect,
    .bar {
      animation: none;
    }
  }
</style>
