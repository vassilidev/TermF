<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import OpsChecks from "$lib/OpsChecks.svelte";
  import { t } from "$lib/i18n";
  import { ACCESS, ENVS, FORM, KINDS, clean, missing, type Check, type Connection, type Field, type Kind } from "$lib/ops-logic";
  import { loadOps, ops } from "$lib/ops.svelte";

  /** A connection to add (`previous` null) or to change; `onclose` once saved or cancelled, with its name. */
  let {
    project,
    start,
    previous,
    onclose,
  }: { project: string; start: Connection; previous: string | null; onclose: (saved: string | null) => void } = $props();

  // svelte-ignore state_referenced_locally
  let c = $state<Connection>(structuredClone(start));
  let busy = $state("");
  let error = $state("");
  let checks = $state<Check[] | null>(null);

  const kindLabel = (k: Kind) => t(`opsKind${k[0].toUpperCase()}${k.slice(1)}` as Parameters<typeof t>[0]);
  const db = $derived(c.kind === "postgres" || c.kind === "mysql");
  const labels: Record<Field, Parameters<typeof t>[0]> = {
    host: "opsFieldHost",
    port: "opsFieldPort",
    user: "opsFieldUser",
    jump: "opsFieldJump",
    via: "opsFieldVia",
    database: "opsFieldDatabase",
    context: "opsFieldContext",
    namespace: "opsFieldNamespace",
    log_group: "opsFieldLogGroup",
    region: "opsFieldRegion",
    profile: "opsFieldProfile",
    gcp_project: "opsFieldGcpProject",
    url: "opsFieldUrl",
    site: "opsFieldSite",
  };
  const label = (f: Field) => t(db && f === "host" ? "opsFieldDbHost" : db && f === "user" ? "opsFieldDbUser" : labels[f]);
  const help = (f: Field) =>
    f === "host" ? t(db ? "opsFieldDbHostHelp" : "opsFieldHostHelp") : db && f === "user" ? t("opsFieldDbUserHelp") : "";
  const placeholders: Partial<Record<Field, string>> = {
    host: "acme-prod",
    user: "deploy",
    jump: "bastion.acme.dev",
    database: "acme",
    context: "acme-prod",
    namespace: "default",
    log_group: "/ecs/acme-api",
    region: "eu-west-3",
    url: "https://loki.acme.dev",
    site: "datadoghq.com",
  };
  const secretLabels: Record<string, Parameters<typeof t>[0]> = {
    password: "opsSecretPassword",
    token: "opsSecretToken",
    api_key: "opsSecretApiKey",
    app_key: "opsSecretAppKey",
  };
  const servers = $derived(ops.connections.filter((x) => x.kind === "ssh" && x.name !== previous));
  const lacks = $derived(missing(c));

  function setSecret(role: string, name: string) {
    c.secrets = { ...(c.secrets ?? {}), [role]: name };
  }

  function value(f: Field): string {
    const v = f === "port" ? c.port : c[f];
    return v === undefined || v === null ? "" : String(v);
  }

  function set(f: Field, v: string) {
    if (f === "port") c.port = v.trim() ? Number(v) : undefined;
    else c[f] = v;
  }

  async function test() {
    busy = "test";
    error = "";
    checks = null;
    try {
      checks = await invoke<Check[]>("ops_test", { project, connection: clean(c), previous });
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  async function save() {
    busy = "save";
    error = "";
    const saved = clean(c);
    try {
      ops.connections = await invoke<Connection[]>("ops_connection_save", { project, connection: saved, previous });
      await loadOps(project);
      // A new database: whether its user can write is worth knowing at once.
      if (previous === null && (saved.kind === "postgres" || saved.kind === "mysql")) {
        busy = "test";
        checks = await invoke<Check[]>("ops_test", { project, connection: saved, previous: saved.name }).catch(() => null);
        if (checks?.some((x) => x.warn || !x.ok)) return;
      }
      onclose(saved.name);
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }
</script>

<form
  class="form"
  onsubmit={(e) => {
    e.preventDefault();
    save();
  }}
>
  <h2>{previous === null ? t("opsAdd") : previous}</h2>
  <div class="grid">
    <label class="labelled">
      <span>{t("opsName")}</span>
      <input class="field mono" bind:value={c.name} placeholder="acme-prod" required />
      <small class="muted">{t("opsNameHelp")}</small>
    </label>
    <label class="labelled">
      <span>{t("opsKind")}</span>
      <select class="field" bind:value={c.kind}>
        {#each KINDS as k (k)}<option value={k}>{kindLabel(k)}</option>{/each}
      </select>
    </label>
  </div>
  <div class="labelled">
    <span id="ops-env">{t("opsEnv")}</span>
    <div class="segmented" role="group" aria-labelledby="ops-env">
      {#each ENVS as env (env)}
        <button type="button" aria-pressed={c.env === env} onclick={() => (c.env = env)}>{env}</button>
      {/each}
    </div>
  </div>
  <div class="grid">
    {#each FORM[c.kind].fields as f (f)}
      {#if f === "via"}
        <label class="labelled">
          <span>{label(f)}</span>
          <select class="field" value={c.via ?? ""} onchange={(e) => (c.via = e.currentTarget.value || undefined)}>
            <option value="">{t("opsFieldViaNone")}</option>
            {#each servers as s (s.name)}<option value={s.name}>{s.name}</option>{/each}
          </select>
        </label>
      {:else}
        <label class="labelled" class:wide={!!help(f)}>
          <span>{label(f)}</span>
          <input
            class="field mono"
            inputmode={f === "port" ? "numeric" : undefined}
            value={value(f)}
            placeholder={placeholders[f] ?? ""}
            oninput={(e) => set(f, e.currentTarget.value)}
          />
          {#if help(f)}<small class="muted">{help(f)}</small>{/if}
        </label>
      {/if}
    {/each}
    {#each FORM[c.kind].secrets as role (role)}
      <label class="labelled">
        <span>{t(secretLabels[role])}</span>
        <input
          class="field mono"
          value={c.secrets?.[role] ?? ""}
          placeholder="ACME_DB_PASSWORD"
          autocomplete="off"
          oninput={(e) => setSecret(role, e.currentTarget.value)}
        />
        <small class="muted">{t("opsSecretHelp", { env: c.env })}</small>
      </label>
    {/each}
  </div>
  <label class="labelled">
    <span>{t("opsAccess")}</span>
    <select class="field" bind:value={c.ai_access}>
      {#each ACCESS as a (a)}
        <option value={a}>{t(a === "none" ? "opsAccessNone" : a === "redacted" ? "opsAccessRedacted" : "opsAccessFull")}</option>
      {/each}
    </select>
    <small class="muted">{t("opsAccessHelp")}</small>
  </label>
  <label class="labelled">
    <span>{t("opsDescription")}</span>
    <input class="field" bind:value={c.description} />
  </label>

  {#if error}<p class="notice error" role="alert">{error}</p>{/if}
  {#if checks}<OpsChecks {checks} />{/if}
  {#if lacks.length}<p class="muted small">{t("opsMissing", { fields: lacks.map((f) => label(f)).join(", ") })}</p>{/if}

  <div class="buttons">
    <button type="button" class="btn" onclick={() => onclose(null)}>{t("cancel")}</button>
    <button type="button" class="btn" disabled={!!busy || !c.name.trim() || lacks.length > 0} onclick={test}>
      {#if busy === "test"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("opsTest")}
    </button>
    <button type="submit" class="btn primary" disabled={!!busy || !c.name.trim() || lacks.length > 0}>
      {#if busy === "save"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("opsSave")}
    </button>
  </div>
</form>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 12px 16px;
  }

  .wide {
    grid-column: 1 / -1;
  }

  small {
    font-size: var(--text-xs);
    line-height: 1.45;
  }

  .small {
    margin: 0;
    font-size: var(--text-sm);
  }

  .segmented {
    align-self: flex-start;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
