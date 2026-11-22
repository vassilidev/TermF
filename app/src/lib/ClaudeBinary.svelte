<script lang="ts">
  import Banner from "$lib/ui/Banner.svelte";
  import { t } from "$lib/i18n";
  import { binaryOrigin, type ClaudeBinary, type ClaudeInstall } from "$lib/claude.svelte";

  /** `summary`: also the install TermF runs, for a page that shows it nowhere else. */
  let { b, summary = false }: { b: ClaudeBinary; summary?: boolean } = $props();

  const about = (i: ClaudeInstall) => `${i.version ?? t("claudeViewUnknown")} · ${binaryOrigin(i)}`;
  const old = $derived(b.install && b.tooOld ? b.install : null);
  const others = $derived(b.install ? b.others : []);
</script>

{#if summary || old || others.length}
  <div class="binary">
    {#if summary && b.install}
      <p class="install"><code>{b.install.path}</code><span class="muted">{about(b.install)}</span></p>
    {:else if summary}
      <Banner kind="warn">{t("claudebinNone")}</Banner>
    {/if}
    {#if old}
      <Banner kind="error">{t("claudebinTooOld", { version: old.version ?? "", minimum: b.minimum })}</Banner>
    {/if}
    {#if others.length}
      <Banner kind="warn">
        {t("claudebinOthers")}
        {#each others as o (o.path)}
          <span class="other"><code>{o.path}</code> <span class="muted">{about(o)}</span></span>
        {/each}
        <span class="other">{t("claudebinOthersHelp")}</span>
      </Banner>
    {/if}
  </div>
{/if}

<style>
  .binary {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .install {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
    margin: 0;
  }

  code {
    font: var(--text-sm) var(--font-mono);
    overflow-wrap: anywhere;
  }

  .muted {
    font-size: var(--text-sm);
  }

  .other {
    display: block;
    margin-top: 4px;
  }
</style>
