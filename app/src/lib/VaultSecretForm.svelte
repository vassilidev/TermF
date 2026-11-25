<script lang="ts">
  import Eye from "@lucide/svelte/icons/eye";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import Field from "$lib/ui/Field.svelte";
  import Segments from "$lib/ui/Segments.svelte";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import { failed, needsConfirm, refreshVault, say, setStatus, type VaultStatus } from "$lib/vault.svelte";
  import { secretNameOk, type Kind, type Scope, type SecretInfo } from "$lib/vault-logic";

  /**
   * A secret given to sessions and agents as an environment variable: its value, or a reference its manager's CLI
   * reads at each use. Editing replaces it; its name, environment and project stay.
   */
  let {
    secret,
    project: newProject,
    onsaved,
    oncancel,
    onkind,
  }: {
    secret: SecretInfo | null;
    project: string | null;
    onsaved: (saved: SecretInfo) => void;
    oncancel: () => void;
    /** A new element switched to a login or a note. */
    onkind?: (kind: Kind) => void;
  } = $props();

  // svelte-ignore state_referenced_locally (the form starts from the secret, then is the user's)
  let name = $state(secret?.name ?? "");
  // svelte-ignore state_referenced_locally
  let scope = $state<Scope>(secret?.scope ?? "dev");
  // svelte-ignore state_referenced_locally
  let project = $state<string | null>(secret ? secret.project : newProject);
  // svelte-ignore state_referenced_locally
  let source = $state<"value" | "ref">(secret?.reference ? "ref" : "value");
  // svelte-ignore state_referenced_locally
  let uri = $state(secret?.reference ?? "");
  // svelte-ignore state_referenced_locally
  let description = $state(secret?.description ?? "");
  let value = $state("");
  let show = $state(false);
  let tried = $state(false);
  let saving = $state(false);
  let error = $state("");
  /** Prod refused the write: the master password is asked here, then the save goes again. */
  let reauth = $state(false);
  let master = $state("");

  const editing = $derived(secret !== null);
  const projects = $derived(app.workspace?.projects ?? []);
  const nameError = $derived((tried || name !== "") && !secretNameOk(name) ? t("restVaultSecretNameError") : "");
  const ready = $derived(secretNameOk(name) && (source === "ref" ? !!uri.trim() : !!value) && (!reauth || !!master));

  onDestroy(() => {
    value = master = "";
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    tried = true;
    if (!secretNameOk(name)) return;
    saving = true;
    error = "";
    try {
      if (reauth) {
        setStatus(await invoke<VaultStatus>("vault_reauth", { password: master }));
        reauth = false;
      }
      const args = { name, scope, project, description: description.trim() || null };
      const saved =
        source === "ref"
          ? await invoke<SecretInfo>("vault_ref", { ...args, uri: uri.trim() })
          : await invoke<SecretInfo>("vault_set", { ...args, value });
      value = "";
      await refreshVault();
      const where = saved.project ? (projects.find((p) => p.id === saved.project)?.name ?? saved.project) : t("vaultGlobal");
      say(t("restVaultSecretSaved", { name: saved.name, scope: saved.scope, where }));
      onsaved(saved);
    } catch (e) {
      const text = String(e);
      if (needsConfirm(e)) reauth = true;
      else if (/a reference is/.test(text)) error = t("restVaultSecretRefError");
      else if (/a secret's name/.test(text)) error = t("restVaultSecretNameError");
      else error = failed(e);
    } finally {
      master = "";
      saving = false;
    }
  }
</script>

<form class="form" onsubmit={save}>
  <h2 class:mono={editing}>{editing ? t("restVaultEditSecret", { name: secret?.name ?? "" }) : t("restVaultNewSecret")}</h2>

  {#if !editing && onkind}
    <div class="segmented" role="radiogroup" aria-label={t("vaultKind")}>
      <button type="button" role="radio" aria-checked="false" onclick={() => onkind("login")}>{t("vaultLogin")}</button>
      <button type="button" role="radio" aria-checked="false" onclick={() => onkind("note")}>{t("vaultNote")}</button>
      <button type="button" role="radio" aria-checked="true">{t("vaultSecret")}</button>
    </div>
  {/if}

  <div class="pair">
    <div class="grow">
      <Field label={t("restVaultSecretName")} help={t("restVaultSecretNameHelp")} error={nameError}>
        {#snippet control(attrs)}
          <!-- svelte-ignore a11y_autofocus (the form was just opened for this) -->
          <input
            class="field mono"
            {...attrs}
            bind:value={name}
            disabled={editing}
            maxlength="128"
            autocomplete="off"
            spellcheck="false"
            placeholder="STRIPE_SECRET_KEY"
            autofocus={!editing}
          />
        {/snippet}
      </Field>
    </div>
    <Field label={t("vaultProject")}>
      {#snippet control(attrs)}
        <select class="field" {...attrs} bind:value={project} disabled={editing}>
          <option value={null}>{t("vaultGlobal")}</option>
          {#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </select>
      {/snippet}
    </Field>
  </div>

  <div class="group">
    <span class="label">{t("restVaultSecretScope")}</span>
    {#if editing}
      <span><span class="badge" class:danger={scope === "prod"}>{scope}</span></span>
    {:else}
      <Segments
        label={t("restVaultSecretScope")}
        options={["dev", "staging", "prod"].map((id) => ({ id, label: id }))}
        selected={scope}
        onselect={(id) => (scope = id as Scope)}
      />
    {/if}
    {#if scope === "prod"}<p class="note">{t("restVaultSecretProdHint")}</p>{/if}
  </div>

  <div class="group">
    <span class="label">{t("restVaultSecretSource")}</span>
    <Segments
      label={t("restVaultSecretSource")}
      options={[
        { id: "value", label: t("restVaultSecretByValue") },
        { id: "ref", label: t("restVaultSecretByRef") },
      ]}
      selected={source}
      onselect={(id) => (source = id as "value" | "ref")}
    />
  </div>

  {#if source === "value"}
    <Field label={t("vaultValue")} help={editing ? t("restVaultSecretReplaceHint") : undefined}>
      {#snippet control(attrs)}
        <div class="with-tools">
          <!-- svelte-ignore a11y_autofocus (replacing the value is what the form was opened for) -->
          <input
            class="field mono"
            {...attrs}
            type={show ? "text" : "password"}
            bind:value
            autocomplete="off"
            spellcheck="false"
            autofocus={editing}
          />
          <button
            type="button"
            class="icon-btn"
            aria-label={show ? t("keysHide") : t("vaultShowTyped")}
            title={show ? t("keysHide") : t("vaultShowTyped")}
            aria-pressed={show}
            onclick={() => (show = !show)}
          >
            {#if show}<EyeOff size={15} aria-hidden="true" />{:else}<Eye size={15} aria-hidden="true" />{/if}
          </button>
        </div>
      {/snippet}
    </Field>
  {:else}
    <Field label={t("vaultReference")} help={t("restVaultSecretRefHelp")}>
      {#snippet control(attrs)}
        <input class="field mono" {...attrs} bind:value={uri} autocomplete="off" spellcheck="false" placeholder="op://Acme/Stripe/secret-key" />
      {/snippet}
    </Field>
  {/if}

  <Field label={t("restVaultSecretDescription")} bind:value={description} maxlength={200} placeholder={t("restVaultSecretDescriptionPlaceholder")} />

  {#if reauth}
    <div class="reauth">
      <Field label={t("vaultProdAgain")}>
        {#snippet control(attrs)}
          <!-- svelte-ignore a11y_autofocus (asked for right after the save) -->
          <input class="field" {...attrs} type="password" autocomplete="current-password" bind:value={master} required autofocus />
        {/snippet}
      </Field>
    </div>
  {/if}

  {#if error}<p class="notice error" role="alert">{error}</p>{/if}

  <div class="actions">
    <button type="button" class="btn" onclick={oncancel}>{t("cancel")}</button>
    <button type="submit" class="btn primary" disabled={saving || !ready}>
      {#if saving}<LoaderCircle size={14} class="spin" aria-hidden="true" />{/if}{t("save")}
    </button>
  </div>
</form>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 560px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  h2.mono {
    font-size: var(--text-lg);
    letter-spacing: 0;
  }

  .segmented {
    align-self: flex-start;
  }

  .pair {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 12px;
  }

  .pair .grow {
    flex: 1 1 260px;
  }

  .pair select {
    min-width: 160px;
  }

  .group {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  .label {
    font-weight: 500;
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .with-tools {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .reauth {
    padding: 12px 14px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
