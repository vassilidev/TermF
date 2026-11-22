<script lang="ts">
  import Dices from "@lucide/svelte/icons/dices";
  import Eye from "@lucide/svelte/icons/eye";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import WandSparkles from "@lucide/svelte/icons/wand-sparkles";
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import { failed, refreshVault } from "$lib/vault.svelte";
  import { strength, urlsOf, type ItemInfo, type Kind, type Recipe } from "$lib/vault-logic";

  /** A new item (`item` null) or the changes to one. A value typed or generated lives here until saved or cancelled. */
  let {
    item,
    kind: newKind = "login",
    project: newProject,
    onsaved,
    oncancel,
  }: {
    item: ItemInfo | null;
    kind?: Kind;
    project: string | null;
    onsaved: (saved: ItemInfo) => void;
    oncancel: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally (the form starts from the item, then is the user's)
  let kind = $state<Kind>(item?.kind ?? newKind);
  // svelte-ignore state_referenced_locally
  let title = $state(item?.title ?? "");
  // svelte-ignore state_referenced_locally
  let project = $state<string | null>(item ? item.project : newProject);
  // svelte-ignore state_referenced_locally
  let username = $state(item?.username ?? "");
  // svelte-ignore state_referenced_locally
  let urls = $state(item?.urls.join("\n") ?? "");
  let password = $state("");
  let showPassword = $state(false);
  let totp = $state("");
  let removeTotp = $state(false);
  let notes = $state("");
  /** The notes the item has are in the field (or it has none): what is saved replaces them. */
  // svelte-ignore state_referenced_locally
  let notesReady = $state(!item?.hasNotes);
  // svelte-ignore state_referenced_locally
  let agents = $state(item?.agents ?? false);
  let saving = $state(false);
  let error = $state("");

  let generator = $state(false);
  let recipe = $state<Recipe>({
    kind: "password",
    length: 20,
    lower: true,
    upper: true,
    digits: true,
    symbols: true,
    unambiguous: false,
  });
  let generated = $state("");

  const projects = $derived(app.workspace?.projects ?? []);
  const editing = $derived(item !== null);

  onMount(() => {
    if (!item?.hasNotes) return;
    invoke<string>("vault_item_reveal", { id: item.id, field: "notes" })
      .then((value) => {
        notes = value;
        notesReady = true;
      })
      .catch((e) => (error = failed(e)));
  });
  onDestroy(() => {
    password = totp = notes = generated = "";
  });

  async function generate() {
    try {
      generated = await invoke<string>("vault_generate", { recipe: $state.snapshot(recipe) });
    } catch (e) {
      generated = "";
      error = String(e);
    }
  }

  $effect(() => {
    if (generator) {
      void JSON.stringify(recipe);
      generate();
    }
  });

  function setKind(to: "password" | "passphrase") {
    recipe =
      to === "password"
        ? { kind: "password", length: 20, lower: true, upper: true, digits: true, symbols: true, unambiguous: false }
        : { kind: "passphrase", words: 5, separator: "-" };
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    error = "";
    try {
      const saved = await invoke<ItemInfo>("vault_item_save", {
        item: {
          id: item?.id ?? null,
          kind,
          project,
          title,
          urls: urlsOf(urls),
          username,
          password: password || null,
          totp: removeTotp ? "" : totp.trim() || null,
          notes: notesReady ? notes : null,
          agents,
        },
      });
      password = totp = notes = generated = "";
      await refreshVault();
      onsaved(saved);
    } catch (e) {
      error = failed(e);
    } finally {
      saving = false;
    }
  }
</script>

<form class="form" onsubmit={save}>
  <h2>{editing ? t("vaultEditTitle", { title: item?.title ?? "" }) : kind === "login" ? t("vaultNewLogin") : t("vaultNewNote")}</h2>

  {#if !editing}
    <div class="segmented" role="radiogroup" aria-label={t("vaultKind")}>
      <button type="button" role="radio" aria-checked={kind === "login"} onclick={() => (kind = "login")}>{t("vaultLogin")}</button>
      <button type="button" role="radio" aria-checked={kind === "note"} onclick={() => (kind = "note")}>{t("vaultNote")}</button>
    </div>
  {/if}

  <div class="pair">
    <label class="labelled grow">
      {t("vaultTitleField")}
      <!-- svelte-ignore a11y_autofocus (the form was just opened for this) -->
      <input class="field" bind:value={title} required maxlength="200" autocomplete="off" autofocus placeholder={kind === "login" ? "GitHub" : t("vaultNotePlaceholderTitle")} />
    </label>
    <label class="labelled">
      {t("vaultProject")}
      <select class="field" bind:value={project}>
        <option value={null}>{t("vaultGlobal")}</option>
        {#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
      </select>
    </label>
  </div>

  {#if kind === "login"}
    <label class="labelled">
      {t("vaultUsername")}
      <input class="field" bind:value={username} autocomplete="off" spellcheck="false" placeholder="alex@acme.dev" />
    </label>

    <div class="labelled">
      <label for="vault-password">{t("vaultPassword")}</label>
      <div class="with-tools">
        <input
          id="vault-password"
          class="field mono"
          type={showPassword ? "text" : "password"}
          bind:value={password}
          autocomplete="new-password"
          spellcheck="false"
          placeholder={item?.hasPassword ? t("vaultUnchanged") : ""}
        />
        <button
          type="button"
          class="icon-btn"
          aria-label={showPassword ? t("keysHide") : t("vaultShowTyped")}
          title={showPassword ? t("keysHide") : t("vaultShowTyped")}
          aria-pressed={showPassword}
          onclick={() => (showPassword = !showPassword)}
        >
          {#if showPassword}<EyeOff size={15} aria-hidden="true" />{:else}<Eye size={15} aria-hidden="true" />{/if}
        </button>
        <button type="button" class="btn sm" aria-expanded={generator} onclick={() => (generator = !generator)}>
          <WandSparkles size={14} aria-hidden="true" />{t("vaultGenerate")}
        </button>
      </div>
    </div>

    {#if generator}
      <div class="generator">
        <div class="segmented" role="radiogroup" aria-label={t("vaultGenerate")}>
          <button type="button" role="radio" aria-checked={recipe.kind === "password"} onclick={() => setKind("password")}>{t("vaultGenPassword")}</button>
          <button type="button" role="radio" aria-checked={recipe.kind === "passphrase"} onclick={() => setKind("passphrase")}>{t("vaultGenPassphrase")}</button>
        </div>
        <div class="preview">
          <output class="mono">{generated}</output>
          <button type="button" class="icon-btn" aria-label={t("vaultGenAgain")} title={t("vaultGenAgain")} onclick={generate}>
            <Dices size={15} aria-hidden="true" />
          </button>
        </div>
        {#if recipe.kind === "password"}
          <label class="range">
            <span>{t("vaultGenLength", { n: recipe.length })}</span>
            <input type="range" min="8" max="64" bind:value={recipe.length} />
          </label>
          <div class="checks">
            <label><input type="checkbox" bind:checked={recipe.upper} />A–Z</label>
            <label><input type="checkbox" bind:checked={recipe.digits} />0–9</label>
            <label><input type="checkbox" bind:checked={recipe.symbols} />!#$%</label>
            <label><input type="checkbox" bind:checked={recipe.unambiguous} />{t("vaultGenUnambiguous")}</label>
          </div>
        {:else}
          <label class="range">
            <span>{t("vaultGenWords", { n: recipe.words })}</span>
            <input type="range" min="3" max="10" bind:value={recipe.words} />
          </label>
          <label class="checks">
            {t("vaultGenSeparator")}
            <select class="field sep" bind:value={recipe.separator}>
              <option value="-">-</option>
              <option value=".">.</option>
              <option value="_">_</option>
              <option value=" ">{t("vaultGenSpace")}</option>
            </select>
          </label>
        {/if}
        <div class="gen-foot">
          <span class="muted">{t("vaultGenStrength", { bits: strength(recipe) })}</span>
          <button
            type="button"
            class="btn sm primary"
            disabled={!generated}
            onclick={() => {
              password = generated;
              generator = false;
            }}>{t("vaultGenUse")}</button
          >
        </div>
      </div>
    {/if}

    <div class="labelled">
      <label for="vault-totp">{t("vaultTotp")}</label>
      <div class="with-tools">
        <input
          id="vault-totp"
          class="field mono"
          type="password"
          bind:value={totp}
          autocomplete="off"
          spellcheck="false"
          disabled={removeTotp}
          placeholder={item?.totp ? t("vaultUnchanged") : t("vaultTotpPlaceholder")}
        />
        {#if item?.totp}
          <label class="inline"><input type="checkbox" bind:checked={removeTotp} />{t("vaultTotpRemove")}</label>
        {/if}
      </div>
      <span class="hint muted">{t("vaultTotpHint")}</span>
    </div>

    <label class="labelled">
      {t("vaultUrls")}
      <textarea class="field area" rows="2" bind:value={urls} spellcheck="false" placeholder="https://github.com"></textarea>
    </label>
  {/if}

  <label class="labelled">
    {kind === "note" ? t("vaultNote") : t("vaultNotes")}
    {#if notesReady}
      <textarea class="field area" class:long={kind === "note"} rows={kind === "note" ? 10 : 3} bind:value={notes} spellcheck="false" placeholder={kind === "note" ? t("vaultNotePlaceholder") : ""}></textarea>
    {:else}
      <span class="muted loading"><LoaderCircle size={14} class="spin" aria-hidden="true" />{t("vaultLoadingNotes")}</span>
    {/if}
  </label>

  <label class="agents">
    <input type="checkbox" role="switch" class="switch" bind:checked={agents} />
    <span>
      <span class="name">{t("vaultAgents")}</span>
      <span class="muted hint">{t("vaultAgentsHint")}</span>
    </span>
  </label>

  {#if error}<p class="notice error" role="alert">{error}</p>{/if}

  <div class="actions">
    <button type="button" class="btn" onclick={oncancel}>{t("cancel")}</button>
    <button type="submit" class="btn primary" disabled={saving || !title.trim() || (!notesReady && item?.hasNotes)}>
      {#if saving}<LoaderCircle size={14} class="spin" aria-hidden="true" />{/if}{t("save")}
    </button>
  </div>
</form>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 560px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  .segmented {
    align-self: flex-start;
  }

  .pair {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .pair .grow {
    flex: 1 1 240px;
  }

  .pair select {
    min-width: 160px;
  }

  .with-tools {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .area {
    height: auto;
    padding: 6px 8px;
    font: var(--text-sm) / 1.5 var(--font-mono);
    resize: vertical;
  }

  .area.long {
    min-height: 180px;
  }

  .hint {
    font-size: var(--text-xs);
    font-weight: 400;
  }

  .inline {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-weight: 400;
    white-space: nowrap;
  }

  .loading {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 400;
  }

  .generator {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg);
  }

  .preview {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 4px 4px 4px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel);
  }

  .preview output {
    flex: 1;
    min-width: 0;
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .range {
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr);
    align-items: center;
    gap: 10px;
    font-size: var(--text-sm);
  }

  .checks {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 16px;
    font-size: var(--text-sm);
  }

  .checks label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .sep {
    width: 110px;
  }

  .gen-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: var(--text-sm);
  }

  .agents {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    font-size: var(--text-sm);
  }

  .agents .switch {
    margin-top: 2px;
  }

  .agents > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .agents .name {
    font-weight: 500;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
