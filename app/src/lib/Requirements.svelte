<script lang="ts">
  import { onMount } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import CircleDashed from "@lucide/svelte/icons/circle-dashed";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import Copy from "@lucide/svelte/icons/copy";
  import Download from "@lucide/svelte/icons/download";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import ClaudeBinary from "$lib/ClaudeBinary.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import { openUrl } from "$lib/claude.svelte";
  import { t } from "$lib/i18n";
  import { checkTools, downloadChrome, needs } from "$lib/release.svelte";
  import { found, installs, missingRequired, osOf, required, type ToolId } from "$lib/release-logic";

  /** `ondone`: the Home's first-launch card, which the user can put away once read. */
  let { ondone }: { ondone?: () => void } = $props();

  const os = osOf(navigator.platform);
  const rows: { id: ToolId; name: string; what: Parameters<typeof t>[0] }[] = [
    { id: "git", name: "git", what: "releaseGitWhat" },
    { id: "claude", name: "Claude Code", what: "releaseClaudeWhat" },
    { id: "gh", name: "GitHub CLI (gh)", what: "releaseGhWhat" },
    { id: "chrome", name: "Chrome", what: "releaseChromeWhat" },
  ];

  let copied = $state<ToolId | null>(null);

  onMount(() => {
    if (!needs.tools && !needs.checking) checkTools();
  });

  async function copy(id: ToolId, text: string) {
    await navigator.clipboard.writeText(text);
    copied = id;
    setTimeout(() => copied === id && (copied = null), 1500);
  }

  const tools = $derived(needs.tools);
  const missing = $derived(tools ? missingRequired(tools) : []);
</script>

<section class="needs" aria-labelledby="needs-title">
  <header>
    <div>
      <h2 id="needs-title">{t("releaseNeeds")}</h2>
      <p class="help">{t("releaseNeedsHelp")}</p>
    </div>
    <button class="btn sm" onclick={checkTools} disabled={needs.checking}>
      {#if needs.checking}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<RefreshCw size={14} aria-hidden="true" />{/if}
      {t("releaseRecheck")}
    </button>
  </header>

  {#if needs.error}<Banner kind="error" ondismiss={() => (needs.error = "")}>{needs.error}</Banner>{/if}

  {#if tools}
    <ul class="tools">
      {#each rows as row (row.id)}
        {@const ok = found(tools, row.id)}
        {@const must = required.includes(row.id)}
        {@const how = installs[row.id]}
        <li class:missing={!ok && must}>
          <span class="state" class:ok aria-hidden="true">
            {#if ok}<Check size={14} />{:else if must}<CircleX size={14} />{:else}<CircleDashed size={14} />{/if}
          </span>
          <div class="body">
            <div class="title">
              <b>{row.name}</b>
              <span class="badge">{t(must ? "releaseRequired" : "releaseOptional")}</span>
              {#if !ok}<span class="sr-only">{t("releaseMissing")}</span>{/if}
            </div>
            <p class="what">{t(row.what)}</p>
            {#if row.id === "claude" && tools.claude.install}
              <ClaudeBinary b={tools.claude} summary />
            {:else if row.id === "chrome" && tools.chrome}
              <p class="at">
                <code>{tools.chrome.path}</code>
                {#if tools.chrome.testing}<span class="muted">{t("releaseChromeTesting", { version: tools.chrome.version ?? "" })}</span>{/if}
              </p>
            {:else if ok && (row.id === "git" || row.id === "gh")}
              <p class="at"><code>{tools[row.id].path}</code>{#if tools[row.id].version}<span class="muted">{tools[row.id].version}</span>{/if}</p>
            {:else}
              {#if how.command[os]}
                <p class="how">{t("releaseInstallWith")}</p>
                <div class="command">
                  <code>{how.command[os]}</code>
                  <button class="btn sm quiet" onclick={() => copy(row.id, how.command[os]!)}>
                    {#if copied === row.id}<Check size={14} aria-hidden="true" />{t("releaseCopied")}{:else}<Copy size={14} aria-hidden="true" />{t("releaseCopy")}{/if}
                  </button>
                </div>
              {/if}
              <div class="actions">
                {#if row.id === "chrome"}
                  <button class="btn sm primary" onclick={downloadChrome} disabled={needs.downloading}>
                    {#if needs.downloading}
                      <LoaderCircle size={14} aria-hidden="true" class="spin" />{t("releaseChromeDownloading")}
                    {:else}
                      <Download size={14} aria-hidden="true" />{t("releaseChromeDownload")}
                    {/if}
                  </button>
                {/if}
                <button class="btn sm" onclick={() => openUrl(how.url)}>
                  <ExternalLink size={14} aria-hidden="true" />{t("releaseOfficial")}
                </button>
              </div>
              {#if row.id === "chrome"}<p class="help">{t("releaseChromeHelp")}</p>{/if}
            {/if}
          </div>
        </li>
      {/each}
    </ul>
    {#if ondone}
      <footer>
        {#if missing.length}
          <Banner kind="warn">{t("releaseMissingRequired")}</Banner>
        {:else}
          <Banner kind="ok">{t("releaseAllGood")}</Banner>
          <button class="btn sm" onclick={ondone}>{t("releaseGotIt")}</button>
        {/if}
      </footer>
    {/if}
  {/if}
</section>

<style>
  .needs {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .help {
    margin: 4px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .tools {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  li {
    display: flex;
    gap: 12px;
    padding: 14px 16px;
  }

  li + li {
    border-top: 1px solid var(--border);
  }

  .state {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--hover);
    color: var(--muted);
  }

  .state.ok {
    background: var(--done-soft);
    color: var(--done);
  }

  .missing .state {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .body {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 22px;
  }

  .what,
  .how {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .at {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
    margin: 0;
    font-size: var(--text-sm);
  }

  code {
    font: var(--text-sm) var(--font-mono);
    overflow-wrap: anywhere;
  }

  .command {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 4px 4px 10px;
    border-radius: var(--radius);
    background: var(--hover);
  }

  .command code {
    flex: 1;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  footer :global(.banner) {
    flex: 1;
  }
</style>
