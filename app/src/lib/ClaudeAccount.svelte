<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Download from "@lucide/svelte/icons/download";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import LogIn from "@lucide/svelte/icons/log-in";
  import LogOut from "@lucide/svelte/icons/log-out";
  import Stethoscope from "@lucide/svelte/icons/stethoscope";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { t } from "$lib/i18n";
  import type { Session } from "$lib/state.svelte";
  import { claudeView, loadOverview, type Overview } from "$lib/claude.svelte";
  import { attempt, openTile, repoArg } from "$lib/claude-native.svelte";

  let { o }: { o: Overview } = $props();

  const loggedIn = $derived(o.account.auth?.loggedIn === true);
  let method = $state<"claudeai" | "console" | "sso">("claudeai");
  let busy = $state("");
  /** The last command's output, shown as it printed it (masked). */
  let output = $state<{ title: string; text: string } | null>(null);
  let confirmLogout = $state(false);
  let confirmUpdate = $state(false);

  async function login() {
    const tile = await attempt(() => invoke<Session>("claude_auth_login", { method }));
    if (tile) {
      claudeView.done = t("claudeDgLoginStarted");
      await openTile(tile);
    }
  }

  async function run(command: string, title: string, args: Record<string, unknown> = {}, reload = false) {
    busy = command;
    const text = await attempt(() => invoke<string>(command, args));
    busy = "";
    if (text === undefined) return;
    output = { title, text };
    if (reload) loadOverview();
  }
</script>

<section class="actions" aria-label={t("claudeDgAccountActions")}>
  <div class="action">
    <div class="about">
      <h3>{loggedIn ? t("claudeDgLogout") : t("claudeDgLogin")}</h3>
      <p class="muted">{loggedIn ? t("claudeDgLogoutHelp") : t("claudeDgLoginHelp")}</p>
    </div>
    {#if loggedIn}
      <button class="btn sm" onclick={() => (confirmLogout = true)} disabled={busy !== ""}>
        <LogOut size={14} aria-hidden="true" />{t("claudeDgLogout")}
      </button>
    {:else}
      <select class="field" bind:value={method} aria-label={t("claudeDgLoginMethod")}>
        <option value="claudeai">{t("claudeDgLoginClaudeAi")}</option>
        <option value="console">{t("claudeDgLoginConsole")}</option>
        <option value="sso">{t("claudeDgLoginSso")}</option>
      </select>
      <button class="btn sm primary" onclick={login}><LogIn size={14} aria-hidden="true" />{t("claudeDgLogin")}</button>
    {/if}
  </div>
  <div class="action">
    <div class="about">
      <h3>{t("claudeDgUpdate")}</h3>
      <p class="muted">{t("claudeDgUpdateHelp")}</p>
    </div>
    <button class="btn sm" onclick={() => (confirmUpdate = true)} disabled={busy !== ""}>
      {#if busy === "claude_update"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Download size={14} aria-hidden="true" />{/if}
      {t("claudeDgUpdate")}
    </button>
  </div>
  <div class="action">
    <div class="about">
      <h3>{t("claudeDgDoctor")}</h3>
      <p class="muted">{t("claudeDgDoctorHelp")}</p>
    </div>
    <button class="btn sm" onclick={() => run("claude_doctor", t("claudeDgDoctor"), { repo: repoArg() })} disabled={busy !== ""}>
      {#if busy === "claude_doctor"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Stethoscope size={14} aria-hidden="true" />{/if}
      {t("claudeDgDoctorRun")}
    </button>
  </div>
</section>

{#if output}
  <section class="output" aria-label={output.title}>
    <header>
      <h3>{output.title}</h3>
      <button class="btn sm quiet" onclick={() => (output = null)}>{t("claudeDgClose")}</button>
    </header>
    <pre>{output.text || t("claudeDgNoOutput")}</pre>
  </section>
{/if}

<ConfirmDialog
  bind:open={confirmLogout}
  title={t("claudeDgLogoutTitle")}
  confirm={t("claudeDgLogout")}
  danger
  onconfirm={() => run("claude_auth_logout", t("claudeDgLogout"), {}, true)}
>
  <p>{t("claudeDgLogoutText")}</p>
</ConfirmDialog>

<ConfirmDialog
  bind:open={confirmUpdate}
  title={t("claudeDgUpdateTitle")}
  confirm={t("claudeDgUpdate")}
  onconfirm={() => run("claude_update", t("claudeDgUpdate"), {}, true)}
>
  <p>{t("claudeDgUpdateText", { version: o.account.version ?? "?", install: o.account.install ?? "?" })}</p>
</ConfirmDialog>

<style>
  .actions {
    margin-top: 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .action {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
  }

  .action + .action {
    border-top: 1px solid var(--border);
  }

  .about {
    flex: 1;
    min-width: 0;
  }

  h3 {
    margin: 0;
    font-size: var(--text-base);
    font-weight: 600;
  }

  .about p {
    margin: 2px 0 0;
    font-size: var(--text-sm);
  }

  select {
    width: 200px;
  }

  .output {
    margin-top: 16px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--chrome);
  }

  .output header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }

  .output pre {
    max-height: 360px;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    font: var(--text-xs) / 1.6 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
