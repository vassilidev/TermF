<script lang="ts">
  import { onMount } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import Copy from "@lucide/svelte/icons/copy";
  import Plus from "@lucide/svelte/icons/plus";
  import Banner from "$lib/ui/Banner.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { t } from "$lib/i18n";
  import { osOf } from "$lib/release-logic";
  import { loadCli, needs, setCli } from "$lib/release.svelte";

  const windows = osOf(navigator.platform) === "windows";
  const line = 'export PATH="$HOME/.local/bin:$PATH"';
  let busy = $state(false);
  let copied = $state(false);

  onMount(loadCli);

  async function set(on: boolean) {
    busy = true;
    await setCli(on);
    busy = false;
  }

  async function copy() {
    await navigator.clipboard.writeText(line);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  const cli = $derived(needs.cli);
</script>

<section class="cli" aria-labelledby="cli-title">
  <h3 id="cli-title">{t("releaseCli")}</h3>
  <p class="help">{t("releaseCliHelp")}</p>
  {#if cli}
    {#if !cli.bundled}
      <Banner>{t("releaseCliNone")}</Banner>
    {:else if cli.managed === "package"}
      <Banner kind="ok">{t("releaseCliPackage", { path: cli.bundled })}</Banner>
    {:else if cli.managed === "appimage"}
      <Banner kind="warn">{t("releaseCliAppImage")}</Banner>
    {:else}
      {#if cli.installed}
        <p class="on"><Check size={14} aria-hidden="true" />{t("releaseCliOn", { path: cli.target })} <span class="muted">· {cli.version}</span></p>
      {:else if cli.taken}
        <Banner kind="warn">{t("releaseCliTaken", { path: cli.target })}</Banner>
      {/if}
      {#if cli.installed && !cli.onPath}
        <Banner kind="warn">
          {t("releaseCliNotOnPath")}
          <span class="line">
            <code>{line}</code>
            <button class="btn sm quiet" onclick={copy}>
              {#if copied}<Check size={14} aria-hidden="true" />{t("releaseCopied")}{:else}<Copy size={14} aria-hidden="true" />{t("releaseCopy")}{/if}
            </button>
          </span>
        </Banner>
      {/if}
      <div class="buttons">
        {#if cli.installed}
          <button class="btn sm" onclick={() => set(false)} disabled={busy}>{t("releaseCliRemove")}</button>
        {:else if !cli.taken}
          <button class="btn sm primary" onclick={() => set(true)} disabled={busy}>
            <Plus size={14} aria-hidden="true" />{t("releaseCliAdd")}
          </button>
        {/if}
      </div>
      <p class="help">{t(windows ? "releaseCliHowWindows" : "releaseCliHowUnix")}</p>
    {/if}
  {:else if !needs.error}
    <Skeleton rows={2} />
  {/if}
</section>

<style>
  .cli {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 28px;
  }

  h3 {
    margin: 0;
    font-size: var(--text-base);
    font-weight: 600;
  }

  .help {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .on {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--done);
    font-size: var(--text-sm);
  }

  .on .muted {
    color: var(--muted);
  }

  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
  }

  code {
    font: var(--text-sm) var(--font-mono);
    overflow-wrap: anywhere;
  }

  .buttons {
    display: flex;
    gap: 6px;
  }

  .buttons:empty {
    display: none;
  }
</style>
