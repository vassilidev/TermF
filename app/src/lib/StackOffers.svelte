<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Copy from "@lucide/svelte/icons/copy";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Plug from "@lucide/svelte/icons/plug";
  import Plus from "@lucide/svelte/icons/plus";
  import Store from "@lucide/svelte/icons/store";
  import { t } from "$lib/i18n";
  import type { RepoStack } from "$lib/library.svelte";

  let {
    stacks,
    ondecide,
  }: {
    stacks: RepoStack[];
    /** Records an added server in the assistant's journal. */
    ondecide?: (detail: string) => Promise<void>;
  } = $props();

  let added = $state<string[]>([]);
  let copied = $state("");
  let busy = $state("");
  let error = $state("");

  const many = $derived(stacks.length > 1);
  /** Each vendor's skills once, whichever repository calls for them. */
  const sources = $derived(
    stacks.flatMap((r) => r.sources).filter((s, i, all) => all.findIndex((o) => o.repo === s.repo) === i),
  );
  /** A server goes to one repository: Claude Code keeps its local scope per folder. */
  const servers = $derived(stacks.flatMap((repo) => repo.mcp.map((server) => ({ repo, server }))));

  function target(server: RepoStack["mcp"][number]): string {
    const s = server.server as { url?: string; command?: string; args?: string[] };
    return s.url ?? [s.command, ...(s.args ?? [])].join(" ");
  }

  async function copy(command: string) {
    try {
      await navigator.clipboard.writeText(command);
      copied = command;
    } catch (e) {
      error = String(e);
    }
  }

  async function open(url: string) {
    try {
      await invoke("open_url", { url });
    } catch (e) {
      error = String(e);
    }
  }

  async function add(repo: RepoStack, server: RepoStack["mcp"][number]) {
    const key = `${repo.path}:${server.name}`;
    busy = key;
    error = "";
    try {
      await invoke("claude_mcp", {
        repo: repo.path,
        action: { op: "add", name: server.name, scope: "local", server: server.server },
      });
      added = [...added, key];
      await ondecide?.(`${server.name} (${repo.name})`);
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }
</script>

{#if error}<p class="error" role="alert">{error}</p>{/if}

<article class="proposal">
  <h3><Store size={15} aria-hidden="true" />{t("stackOfficialTitle")}</h3>
  <p class="hint">{t("stackOfficialHint")}</p>
  <ul class="offers">
    {#each sources as source (source.repo)}
      <li>
        <div class="what">
          <button class="link" onclick={() => open(source.url)}>
            {source.repo}<ExternalLink size={12} aria-hidden="true" />
          </button>
          <span class="badge">{source.for}</span>
          <span class="muted">
            {source.license ? t("stackLicense", { license: source.license }) : t("stackNoLicense")}
          </span>
          <code>{source.command}</code>
        </div>
        <button class="btn sm quiet" onclick={() => copy(source.command)}>
          <Copy size={14} aria-hidden="true" />{copied === source.command ? t("stackCopied") : t("stackCopy")}
        </button>
      </li>
    {:else}
      <li class="muted">{t("stackNothing")}</li>
    {/each}
  </ul>
</article>

<article class="proposal">
  <h3><Plug size={15} aria-hidden="true" />{t("stackMcpTitle")}</h3>
  <p class="hint">{t("stackMcpHint")}</p>
  <ul class="offers">
    {#each servers as { repo, server } (`${repo.path}:${server.name}`)}
      {@const key = `${repo.path}:${server.name}`}
      <li>
        <div class="what">
          <b>{server.name}</b>
          <span class="badge">{server.for}</span>
          {#if many}<span class="muted">{repo.name}</span>{/if}
          {#if server.runsCode}<span class="badge warn">{t("stackMcpRunsCode")}</span>{/if}
          <code>{target(server)}</code>
        </div>
        <button class="btn sm quiet" onclick={() => open(server.docs)} title={server.docs}>
          <ExternalLink size={14} aria-hidden="true" />{t("stackDocs")}
        </button>
        {#if added.includes(key)}
          <span class="done">{t("stackMcpAdded")}</span>
        {:else}
          <button class="btn sm" disabled={!!busy} onclick={() => add(repo, server)}>
            <Plus size={14} aria-hidden="true" />{t("stackMcpAdd")}
          </button>
        {/if}
      </li>
    {:else}
      <li class="muted">{t("stackNothing")}</li>
    {/each}
  </ul>
</article>

<style>
  .proposal {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .hint,
  .error {
    margin: 0;
    color: var(--muted);
    line-height: 1.5;
  }

  .error {
    color: var(--danger-text);
  }

  .offers {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .offers li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .offers li.muted {
    border-style: dashed;
  }

  .what {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
    min-width: 0;
  }

  .what b {
    font-weight: 600;
  }

  .link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .link:hover {
    text-decoration: underline;
  }

  .warn {
    border-color: transparent;
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .done {
    color: var(--done);
    font-weight: 600;
  }

  code {
    flex-basis: 100%;
    padding: 1px 6px;
    overflow: hidden;
    border-radius: var(--radius-sm);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
