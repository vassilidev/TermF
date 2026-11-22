<script lang="ts">
  import Copy from "@lucide/svelte/icons/copy";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Lock from "@lucide/svelte/icons/lock";
  import Smartphone from "@lucide/svelte/icons/smartphone";
  import RemoteQr from "$lib/RemoteQr.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import { openUrl } from "$lib/claude.svelte";
  import { t } from "$lib/i18n";
  import { fits } from "$lib/qr-logic";
  import { phase, type RemoteState } from "$lib/remote-logic";
  import { remote, remoteUi, setRemote } from "$lib/remote.svelte";

  /** A tile's Remote Control, once on (or locked): always in sight, with its link and QR code a click away. */
  let { id, rc }: { id: number; rc: RemoteState } = $props();

  let trigger = $state<HTMLButtonElement>();
  let copied = $state(false);
  const now = $derived(phase(rc));
  const words = {
    live: "remoteLive",
    connecting: "remoteConnecting",
    waiting: "remoteAtTurnEnd",
    stopping: "remoteStopping",
    locked: "remoteLocked",
    off: "remoteTurnOn",
  } as const;
  const said = $derived(t(words[now]));
  let open = $state(false);

  // Turned on from the tile's actions: the popover opens, to show the link as soon as it comes.
  $effect(() => {
    if (remoteUi.open === id && trigger) {
      open = true;
      remoteUi.open = null;
    }
  });

  async function copy(url: string) {
    await navigator.clipboard.writeText(url).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<button
  bind:this={trigger}
  class="rc {now}"
  aria-haspopup="dialog"
  aria-expanded={open}
  aria-label="{t('remoteControl')} · {said}"
  title="{t('remoteControl')} · {said}"
  onclick={() => (open = !open)}
>
  {#if now === "locked"}<Lock size={13} aria-hidden="true" />{:else}<Smartphone size={14} aria-hidden="true" />{/if}
  {#if now === "live"}<span class="dot" aria-hidden="true"></span>{/if}
</button>

<Popover bind:open {trigger} label={t("remoteControl")} align="end" width={288}>
  <div class="panel">
    <header>
      <Smartphone size={15} aria-hidden="true" />
      <b>{t("remoteControl")}</b>
      <span class="state {now}">
        {#if now === "connecting" || now === "waiting"}<LoaderCircle class="spin" size={12} aria-hidden="true" />{/if}
        {now === "locked" ? t("remoteModeOff") : said}
      </span>
    </header>
    <p class="name">{rc.name}</p>
    {#if now === "locked"}
      <p class="notice error"><Lock size={14} aria-hidden="true" />{t("remoteLocked")}</p>
      <p class="help">{t("remoteLockedHelp")}</p>
    {:else if rc.url}
      {#if fits(rc.url)}
        <div class="qr"><RemoteQr text={rc.url} label={t("remoteScan")} /></div>
        <p class="help center">{t("remoteScan")}</p>
      {/if}
      <p class="link" title={rc.url}>{rc.url}</p>
      <div class="row">
        <button class="btn sm" onclick={() => copy(rc.url!)}>
          <Copy size={14} aria-hidden="true" />{copied ? t("remoteCopied") : t("remoteCopy")}
        </button>
        <button class="btn sm" onclick={() => openUrl(rc.url!)}>
          <ExternalLink size={14} aria-hidden="true" />{t("remoteOpen")}
        </button>
      </div>
    {:else if now === "connecting"}
      <p class="help">{t("remoteConnectingHelp")}</p>
    {/if}
    {#if remote.error}<p class="notice error" role="alert">{remote.error}</p>{/if}
    {#if now !== "locked"}
      <footer>
        {#if rc.on && now !== "stopping"}
          <button class="btn sm danger" onclick={() => setRemote(id, false)}>{t("remoteTurnOff")}</button>
        {:else}
          <button class="btn sm primary" onclick={() => setRemote(id, true)}>{t("remoteTurnOn")}</button>
        {/if}
      </footer>
    {/if}
  </div>
</Popover>

<style>
  .rc {
    position: relative;
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--accent-ink);
    cursor: pointer;
  }

  .rc:hover,
  .rc[aria-expanded="true"] {
    border-color: var(--border);
    background: var(--hover);
  }

  .rc.connecting,
  .rc.waiting,
  .rc.stopping {
    color: var(--muted);
  }

  .rc.locked {
    color: var(--danger);
  }

  .dot {
    position: absolute;
    top: 3px;
    right: 3px;
    width: 6px;
    height: 6px;
    border: 1px solid var(--header-bg, var(--panel));
    border-radius: 50%;
    background: var(--done);
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
  }

  header {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  header b {
    font-weight: 600;
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .state.live {
    color: var(--done);
    font-weight: 600;
  }

  .state.locked {
    color: var(--danger);
  }

  .name {
    margin: 0;
    overflow: hidden;
    font: var(--text-sm) var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .qr {
    display: grid;
    place-items: center;
    padding: 4px 0;
  }

  .help {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .center {
    text-align: center;
  }

  .link {
    margin: 0;
    overflow: hidden;
    color: var(--muted);
    font: var(--text-xs) var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row,
  footer {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  footer {
    justify-content: flex-end;
    padding-top: 4px;
    border-top: 1px solid var(--border);
  }

  .notice {
    margin: 0;
  }
</style>
