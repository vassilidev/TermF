<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ClipboardPaste from "@lucide/svelte/icons/clipboard-paste";
  import Database from "@lucide/svelte/icons/database";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import OpsChecks from "$lib/OpsChecks.svelte";
  import { t } from "$lib/i18n";
  import {
    ACCESS,
    ENVS,
    FORM,
    KINDS,
    SSLMODES,
    clean,
    isDb,
    isServer,
    missing,
    passwordName,
    target,
    type Candidate,
    type Check,
    type Connection,
    type Field,
    type Kind,
  } from "$lib/ops-logic";
  import { loadOps, ops } from "$lib/ops.svelte";

  /** A connection to add (`previous` null) or to change; `onclose` once saved or cancelled, with its name. A new one
   * may start from a URL pasted or a database the project's repositories declare; a database's password goes to
   * the vault, never into the project's file. */
  let {
    project,
    start,
    previous,
    ask,
    onclose,
  }: {
    project: string;
    start: Connection;
    previous: string | null;
    ask: (e: unknown, retry: () => Promise<void>) => boolean;
    onclose: (saved: string | null) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let c = $state<Connection>(structuredClone(start));
  let busy = $state("");
  let error = $state("");
  let checks = $state<Check[] | null>(null);
  /** The password typed or pasted in a URL, or the rank of the database found whose password goes to the vault. */
  let password = $state("");
  let found = $state<Candidate | null>(null);
  let url = $state("");
  let filled = $state<{ ignored: string[] } | null>(null);

  const kindLabel = (k: Kind) => t(`opsKind${k[0].toUpperCase()}${k.slice(1)}` as Parameters<typeof t>[0]);
  const db = $derived(isDb(c.kind));
  const labels: Record<Field, Parameters<typeof t>[0]> = {
    host: "opsFieldHost",
    port: "opsFieldPort",
    user: "opsFieldUser",
    jump: "opsFieldJump",
    via: "opsFieldVia",
    database: "opsFieldDatabase",
    sslmode: "opsdbSslmode",
    context: "opsFieldContext",
    namespace: "opsFieldNamespace",
    log_group: "opsFieldLogGroup",
    region: "opsFieldRegion",
    profile: "opsFieldProfile",
    gcp_project: "opsFieldGcpProject",
    url: "opsFieldUrl",
    site: "opsFieldSite",
  };
  const label = (f: Field) =>
    t(
      c.kind === "sqlite" && f === "database"
        ? "opsdbSqliteFile"
        : db && f === "host"
          ? "opsFieldDbHost"
          : db && f === "user"
            ? "opsFieldDbUser"
            : labels[f],
    );
  const help = (f: Field) =>
    f === "host"
      ? t(db ? "opsFieldDbHostHelp" : "opsFieldHostHelp")
      : db && f === "user"
        ? t("opsFieldDbUserHelp")
        : c.kind === "sqlite" && f === "database"
          ? t("opsdbSqliteFileHelp")
          : f === "via"
            ? t("opsdbViaHelp")
            : "";
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

  // The databases the project declares, minus those it already has.
  let candidates = $state<Candidate[] | null>(null);
  $effect(() => {
    if (previous !== null) return;
    invoke<Candidate[]>("ops_detect", { project }).then(
      (list) => (candidates = list),
      () => (candidates = []),
    );
  });
  const same = (a: Connection, b: Connection) =>
    a.kind === b.kind && (a.host ?? "") === (b.host ?? "") && (a.port ?? null) === (b.port ?? null) && a.database === b.database;
  const fresh = $derived((candidates ?? []).filter((x) => !x.connection || !ops.connections.some((o) => same(o, x.connection!))));

  function use(x: Candidate) {
    if (!x.connection) return;
    c = { ...structuredClone($state.snapshot(x.connection)), ai_access: "none" };
    found = x.hasPassword ? x : null;
    password = "";
    filled = null;
    checks = null;
    error = "";
  }

  async function fill() {
    error = "";
    try {
      const parsed = await invoke<{ connection: Connection; password: string | null; ignored: string[] }>("ops_parse_url", {
        url: url.trim(),
      });
      c = { ...parsed.connection, ai_access: c.ai_access ?? "none" };
      password = parsed.password ?? "";
      found = null;
      filled = { ignored: parsed.ignored };
      checks = null;
      url = "";
    } catch (e) {
      error = String(e);
    }
  }

  // A password names its secret, after the connection unless the user named it.
  $effect(() => {
    if ((password || found) && FORM[c.kind].secrets.includes("password") && !c.secrets?.password?.trim() && c.name.trim()) {
      c.secrets = { ...(c.secrets ?? {}), password: passwordName(c.name) };
    }
  });

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
      if (!ask(e, test)) error = String(e);
    } finally {
      busy = "";
    }
  }

  async function save() {
    busy = "save";
    error = "";
    const saved = clean(c);
    const hasPassword = FORM[c.kind].secrets.includes("password");
    try {
      ops.connections = await invoke<Connection[]>("ops_connection_save", {
        project,
        connection: saved,
        previous,
        password: hasPassword && password ? password : null,
        found: hasPassword && !password && found ? found.id : null,
      });
      password = "";
      found = null;
      await loadOps(project);
      // A new database: whether its user can write is worth knowing at once.
      if (previous === null && isDb(saved.kind)) {
        busy = "test";
        checks = await invoke<Check[]>("ops_test", { project, connection: saved, previous: saved.name }).catch(() => null);
        if (checks?.some((x) => x.warn || !x.ok)) return;
      }
      onclose(saved.name);
    } catch (e) {
      if (!ask(e, save)) error = String(e);
    } finally {
      busy = "";
    }
  }

  // Where a new database is: pasted or picked, its fields checked, tested, then saved.
  const step = $derived(
    !c.host && !c.database && !c.via ? 0 : lacks.length ? 1 : checks?.length && checks.every((x) => x.ok || x.warn) ? 3 : 2,
  );
  const steps = ["opsdbStepPaste", "opsdbStepCheck", "opsdbStepTest", "opsdbStepSave"] as const;
</script>

<form
  class="form"
  onsubmit={(e) => {
    e.preventDefault();
    save();
  }}
>
  <h2>{previous === null ? t("opsAdd") : previous}</h2>

  {#if previous === null}
    <ol class="steps" aria-label={t("opsdbSteps")}>
      {#each steps as s, i (s)}
        <li class:done={i < step} class:now={i === step} aria-current={i === step ? "step" : undefined}>
          <span class="n">{i + 1}</span>{t(s)}
        </li>
      {/each}
    </ol>

    <div class="start">
      <label class="labelled">
        <span>{t("opsdbUrl")}</span>
        <div class="url">
          <input
            class="field mono"
            bind:value={url}
            placeholder="postgres://reader@db.acme.dev:5432/acme?sslmode=require"
            autocomplete="off"
            spellcheck="false"
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                if (url.trim()) fill();
              }
            }}
          />
          <button type="button" class="btn" disabled={!url.trim()} onclick={fill}>
            <ClipboardPaste size={14} aria-hidden="true" />{t("opsdbUrlFill")}
          </button>
        </div>
        <small class="muted">{t("opsdbUrlHelp")}</small>
      </label>
      {#if filled}
        <p class="filled" role="status">
          {t("opsdbUrlFilled")}{#if filled.ignored.length}
            {" "}{t("opsdbUrlIgnored", { list: filled.ignored.join(", ") })}{/if}
        </p>
      {/if}

      <div class="found">
        <p class="found-head"><Database size={13} aria-hidden="true" />{t("opsdbFound")}</p>
        <small class="muted">{t("opsdbFoundHelp")}</small>
        {#if candidates === null}
          <p class="muted small"><LoaderCircle size={12} aria-hidden="true" class="spin" />{t("opsdbFoundLoading")}</p>
        {:else if !fresh.length}
          <p class="muted small">{t("opsdbFoundNone")}</p>
        {:else}
          <ul>
            {#each fresh as x (x.id)}
              <li class:refused={!x.connection} class:picked={found?.id === x.id}>
                <div class="what">
                  {#if x.connection}
                    <span class="kind">{kindLabel(x.connection.kind)}</span>
                    <span class="mono where">{target(x.connection, ops.connections)}</span>
                  {:else}
                    <span class="muted">{x.refused}</span>
                  {/if}
                  <span class="mono source">{x.source}</span>
                </div>
                {#if x.hasPassword}<span class="badge"><KeyRound size={11} aria-hidden="true" />{t("opsdbFoundPassword")}</span>{/if}
                {#if x.connection}<button type="button" class="btn sm" onclick={() => use(x)}>{t("opsdbFoundUse")}</button>{/if}
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>
  {/if}

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
          {#if isServer(c.kind)}<small class="muted">{help(f)}</small>{/if}
        </label>
      {:else if f === "sslmode"}
        <label class="labelled">
          <span>{label(f)}</span>
          <select class="field" value={c.sslmode ?? ""} onchange={(e) => (c.sslmode = e.currentTarget.value || undefined)}>
            <option value="">{t("opsdbSslDefault")}</option>
            {#each SSLMODES as m (m)}<option value={m}>{m}</option>{/each}
          </select>
        </label>
      {:else}
        <label class="labelled">
          <span>{label(f)}</span>
          <input
            class="field mono"
            inputmode={f === "port" ? "numeric" : undefined}
            value={value(f)}
            placeholder={c.kind === "sqlite" && f === "database"
              ? "/Users/alex/acme/database/database.sqlite"
              : db && f === "host"
                ? "127.0.0.1"
                : db && f === "user"
                  ? "readonly"
                  : (placeholders[f] ?? "")}
            oninput={(e) => set(f, e.currentTarget.value)}
          />
          {#if help(f)}<small class="muted">{help(f)}</small>{/if}
        </label>
      {/if}
    {/each}
    {#each FORM[c.kind].secrets as role (role)}
      {#if role === "password"}
        <label class="labelled">
          <span>{t("opsdbPassword")}</span>
          <input
            class="field"
            type="password"
            bind:value={password}
            autocomplete="new-password"
            placeholder={found ? "••••••••" : ""}
          />
          <small class="muted">
            {found
              ? t("opsdbPasswordFound", { source: found.source, env: c.env, name: c.secrets?.password ?? passwordName(c.name) })
              : t("opsdbPasswordHelp", { env: c.env, name: c.secrets?.password || passwordName(c.name || "db") })}
          </small>
        </label>
      {/if}
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
    max-width: 560px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
  }

  .grid {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  small {
    font-size: var(--text-xs);
    line-height: 1.45;
  }

  .small {
    display: flex;
    align-items: center;
    gap: 6px;
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

  .steps {
    display: flex;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-xs);
    color: var(--muted);
  }

  .steps li {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 6px;
    padding: 6px 8px 6px 6px;
    border-top: 2px solid var(--border);
    transition:
      border-color var(--dur) var(--ease-out),
      color var(--dur) var(--ease-out);
  }

  .steps .n {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--hover);
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }

  .steps li.done {
    border-color: var(--done);
    color: var(--text);
  }

  .steps li.done .n {
    background: var(--done-soft);
    color: var(--done);
  }

  .steps li.now {
    border-color: var(--accent);
    color: var(--text);
    font-weight: 600;
  }

  .steps li.now .n {
    background: var(--accent);
    color: var(--accent-ink);
  }

  .start {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--chrome);
  }

  .url {
    display: flex;
    gap: 8px;
  }

  .url input {
    flex: 1;
    min-width: 0;
  }

  .filled {
    margin: 0;
    padding: 6px 10px;
    border-radius: var(--radius);
    background: var(--done-soft);
    color: var(--text);
    font-size: var(--text-xs);
  }

  .found {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }

  .found-head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .found ul {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
  }

  .found li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel);
    transition: border-color var(--dur-fast) var(--ease-out);
  }

  .found li.picked {
    border-color: var(--accent);
  }

  .found li.refused {
    opacity: 0.7;
  }

  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    font-size: var(--text-sm);
  }

  .kind {
    font-weight: 500;
  }

  .where,
  .source {
    overflow: hidden;
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .source {
    color: var(--muted);
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
</style>
