<script lang="ts">
  import PhoneOff from "@lucide/svelte/icons/phone-off";
  import Smartphone from "@lucide/svelte/icons/smartphone";
  import Popover from "$lib/ui/Popover.svelte";
  import { focusSession } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { phase } from "$lib/remote-logic";
  import { cutAll, remote, remoteSessions } from "$lib/remote.svelte";
  import { shortPath } from "$lib/state.svelte";

  /** In the navigation while a tile is open to remote control: the list, and "cut all". */
  const tiles = $derived(remoteSessions());
  let trigger = $state<HTMLButtonElement>();
  let open = $state(false);
  let cutting = $state(false);

  const words = {
    live: "remoteLive",
    connecting: "remoteConnecting",
    waiting: "remoteAtTurnEnd",
    stopping: "remoteStopping",
    locked: "remoteLocked",
    off: "remoteConnecting",
  } as const;

  function go(id: number) {
    open = false;
    focusSession(id);
  }

  async function cut() {
    cutting = true;
    await cutAll();
    cutting = false;
    if (!remote.error) open = false;
  }
</script>

{#if tiles.length}
  <button
    bind:this={trigger}
    class="remote"
    aria-haspopup="dialog"
    aria-expanded={open}
    title={t("remoteListTitle")}
    onclick={() => (open = !open)}
  >
    <Smartphone size={14} aria-hidden="true" /><span>{t("remoteCount", { count: tiles.length })}</span>
    {#if tiles.some((s) => !s.remote || s.remote.url)}<span class="live" aria-hidden="true"></span>{/if}
  </button>

  <Popover bind:open {trigger} label={t("remoteListTitle")} width={300}>
    <div class="panel">
      <h3>{t("remoteListTitle")}</h3>
      <ul>
        {#each tiles as s (s.id)}
          {@const now = s.remote ? phase(s.remote) : "live"}
          <li>
            <button class="row" title={t("remoteShow")} onclick={() => go(s.id)}>
              <span class="dot {now}" aria-hidden="true"></span>
              <span class="t">
                <b>{s.remote?.name || s.title}</b>
                <span>{now === "live" ? shortPath(s.cwd) : t(words[now])}</span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
      {#if remote.error}<p class="notice error" role="alert">{remote.error}</p>{/if}
      <button class="btn sm danger cut" disabled={cutting} onclick={cut}>
        <PhoneOff size={14} aria-hidden="true" />{t("remoteCutAll")}
      </button>
    </div>
  </Popover>
{/if}

<style>
  .remote {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 8px;
    height: 30px;
    margin-top: 4px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: var(--text-sm);
    font-weight: 600;
    text-align: left;
  }

  .remote span:not(.live) {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .remote:hover {
    border-color: var(--border-strong);
  }

  .live {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--done);
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
  }

  h3 {
    margin: 0 2px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    text-align: left;
  }

  .row:hover {
    background: var(--hover);
  }

  .t {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .t b,
  .t span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .t b {
    font: 600 var(--text-sm) var(--font-mono);
  }

  .t span {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .dot {
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border: 1.5px solid var(--muted);
    border-radius: 50%;
  }

  .dot.live {
    border-color: var(--done);
    background: var(--done);
  }

  .cut {
    align-self: flex-end;
  }

  .notice {
    margin: 0;
  }
</style>
