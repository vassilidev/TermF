<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import FlaskConical from "@lucide/svelte/icons/flask-conical";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Minus from "@lucide/svelte/icons/minus";
  import Plug from "@lucide/svelte/icons/plug";
  import X from "@lucide/svelte/icons/x";
  import { t } from "$lib/i18n";
  import { ssh, type Check as KeyCheck, type HostTest, type SshKey } from "$lib/ssh.svelte";

  /** `key`: an inventory key, which can also be tried against an SSH host. */
  let {
    path,
    name,
    encrypted,
    key = null,
  }: { path: string; name: string; encrypted: boolean; key?: SshKey | null } = $props();

  let passphrase = $state("");
  let running = $state(false);
  let checks = $state<KeyCheck[] | null>(null);
  let error = $state("");
  let typed = $state<string | null>(null);
  const host = $derived(typed ?? key?.hosts.find((h) => !/[*?!\s]/.test(h)) ?? "github.com");
  let connection = $state<HostTest | null | undefined>(undefined);

  const labels = {
    read: ["keysCheckRead", "keysCheckReadFailed"],
    passphrase: ["keysCheckPassphrase", "keysCheckPassphraseFailed"],
    pair: ["keysCheckPair", "keysCheckPairFailed"],
    signature: ["keysCheckSignature", "keysCheckSignatureFailed"],
    agreement: ["keysCheckAgreement", "keysCheckAgreementFailed"],
  } as const;

  /** Hosts to suggest: this key's, the config's single hosts, then the usual forges. */
  const suggestions = $derived(
    [
      ...(key?.hosts ?? []),
      ...(ssh.inventory?.hosts ?? []).filter((h) => h.patterns.length === 1).map((h) => h.patterns[0]),
      "github.com",
      "gitlab.com",
    ].filter((h, i, all) => !/[*?!\s]/.test(h) && all.indexOf(h) === i),
  );

  async function run() {
    running = true;
    error = "";
    try {
      checks = await invoke<KeyCheck[]>("keys_test", { path, passphrase: passphrase || null, public: null });
    } catch (e) {
      checks = null;
      error = String(e);
    } finally {
      passphrase = "";
      running = false;
    }
  }

  async function connect() {
    connection = null;
    try {
      connection = await invoke<HostTest>("ssh_test", { host: host.trim(), path });
    } catch (e) {
      connection = { ok: false, message: String(e) };
    }
  }
</script>

<section class="test" aria-label={t("keysTestTitle", { name })}>
  <form
    class="run"
    onsubmit={(e) => {
      e.preventDefault();
      run();
    }}
  >
    {#if encrypted}
      <label class="labelled">
        <span>{t("sshPassphraseFor", { name })}</span>
        <input class="field" type="password" autocomplete="off" bind:value={passphrase} {@attach (el) => el.focus()} />
      </label>
    {/if}
    <button type="submit" class="btn sm primary" disabled={running || (encrypted && !passphrase)}>
      {#if running}<LoaderCircle size={14} aria-hidden="true" class="spin" />{t("keysTesting")}
      {:else}<FlaskConical size={14} aria-hidden="true" />{t("keysTestRun")}{/if}
    </button>
  </form>

  {#if error}<p class="notice error">{error}</p>{/if}
  {#if checks}
    <ul class="checks" role="status">
      {#each checks as check, i (i)}
        <li class:ok={check.ok} class:failed={!check.ok}>
          {#if check.ok}<Check size={14} aria-hidden="true" />{:else}<X size={14} aria-hidden="true" />{/if}
          <span>
            {t(labels[check.kind][check.ok ? 0 : 1], { detail: check.detail ?? "", file: check.detail ?? "" })}
            {#if check.error && check.kind !== "passphrase"}<span class="mono why">{check.error}</span>{/if}
          </span>
        </li>
      {/each}
      {#if checks[0]?.ok && !checks.some((c) => c.kind === "pair")}
        <li class="none"><Minus size={14} aria-hidden="true" />{t("keysNoPublicFile")}</li>
      {/if}
    </ul>
  {/if}

  {#if key}
    <form
      class="connect"
      onsubmit={(e) => {
        e.preventDefault();
        connect();
      }}
    >
      <label class="labelled">
        <span>{t("keysConnection")}</span>
        <input class="field mono-field" list="key-test-hosts" value={host} oninput={(e) => (typed = e.currentTarget.value)} autocomplete="off" spellcheck="false" aria-label={t("keysHost")} />
        <datalist id="key-test-hosts">
          {#each suggestions as suggestion (suggestion)}<option value={suggestion}></option>{/each}
        </datalist>
      </label>
      <button type="submit" class="btn sm" disabled={connection === null || !host.trim()}>
        {#if connection === null}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Plug size={14} aria-hidden="true" />{/if}
        {t("keysConnect")}
      </button>
      {#if key.encrypted && !key.inAgent}<p class="hint">{t("keysAgentNeeded")}</p>{/if}
      {#if connection}
        <p class="result" class:ok={connection.ok} class:failed={!connection.ok} role="status">
          {#if connection.ok}<Check size={13} aria-hidden="true" />{:else}<X size={13} aria-hidden="true" />{/if}
          <span class="mono">{connection.message}</span>
        </p>
      {/if}
    </form>
  {/if}
</section>

<style>
  .test {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .run,
  .connect {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 8px;
  }

  .connect {
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }

  .labelled {
    flex: 0 1 300px;
  }

  .mono-field {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .checks {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .checks li,
  .result {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .checks :global(svg),
  .result :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .ok :global(svg),
  .result.ok {
    color: var(--done);
  }

  .failed {
    color: var(--danger);
  }

  .none {
    color: var(--muted);
  }

  .why {
    display: block;
    color: var(--muted);
  }

  .hint {
    flex-basis: 100%;
    margin: 0;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .result {
    flex-basis: 100%;
  }
</style>
