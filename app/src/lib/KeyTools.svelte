<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import Copy from "@lucide/svelte/icons/copy";
  import FlaskConical from "@lucide/svelte/icons/flask-conical";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import GlobeLock from "@lucide/svelte/icons/globe-lock";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import LockKeyhole from "@lucide/svelte/icons/lock-keyhole";
  import Signature from "@lucide/svelte/icons/signature";
  import Terminal from "@lucide/svelte/icons/terminal";
  import X from "@lucide/svelte/icons/x";
  import KeyTest from "$lib/KeyTest.svelte";
  import SecretView from "$lib/SecretView.svelte";
  import { t } from "$lib/i18n";
  import { shortPath } from "$lib/state.svelte";
  import { act, nameError, readable, ssh, type Algo, type Format, type Generated, type Usage } from "$lib/ssh.svelte";

  // Keyed on the panel by SshView: a new panel is a new instance.
  const panel = ssh.panel as { kind: "generate"; usage?: Usage } | { kind: "convert"; path?: string };

  const USAGES = {
    ssh: { icon: Terminal, algos: ["ed25519", "p256", "p384", "p521", "rsa2048", "rsa3072", "rsa4096"], formats: ["openssh", "ppk"] },
    sign: { icon: Signature, algos: ["p256", "p384", "p521", "ed25519", "rsa2048", "rsa3072", "rsa4096"], formats: ["jwk", "jwks", "pkcs8", "sec1", "pkcs1"] },
    encrypt: { icon: LockKeyhole, algos: ["x25519", "p256", "p384", "p521", "rsa2048", "rsa3072", "rsa4096"], formats: ["pkcs8", "jwk", "jwks", "sec1", "pkcs1"] },
    tls: { icon: GlobeLock, algos: ["p256", "p384", "rsa2048", "rsa3072", "rsa4096", "ed25519"], formats: ["pkcs8", "sec1", "pkcs1"] },
  } satisfies Record<Usage, { icon: unknown; algos: Algo[]; formats: Format[] }>;
  const USAGE_TEXT = {
    ssh: ["keysUsageSsh", "keysUsageSshHelp"],
    sign: ["keysUsageSign", "keysUsageSignHelp"],
    encrypt: ["keysUsageEncrypt", "keysUsageEncryptHelp"],
    tls: ["keysUsageTls", "keysUsageTlsHelp"],
  } as const;
  const ALGOS: Record<Algo, string> = {
    ed25519: "Ed25519",
    p256: "ECDSA P-256",
    p384: "ECDSA P-384",
    p521: "ECDSA P-521",
    rsa2048: "RSA 2048",
    rsa3072: "RSA 3072",
    rsa4096: "RSA 4096",
    x25519: "X25519",
  };
  const FORMATS = {
    openssh: ["OpenSSH", "keysFormatOpenssh", ["", ".pub"]],
    ppk: ["PuTTY", "keysFormatPpk", [".ppk", ".pub"]],
    jwk: ["JWK", "keysFormatJwk", [".jwk.json", ".pub.jwk.json"]],
    jwks: ["JWKS", "keysFormatJwks", [".jwks.json", ".pub.jwks.json"]],
    pkcs8: ["PKCS#8", "keysFormatPkcs8", [".pem", ".pub.pem"]],
    sec1: ["SEC1", "keysFormatSec1", [".pem", ".pub.pem"]],
    pkcs1: ["PKCS#1", "keysFormatPkcs1", [".pem", ".pub.pem"]],
    spki: ["SPKI", "keysFormatSpki", [".pub.pem"]],
  } as const;
  const ENCRYPTABLE: Format[] = ["openssh", "pkcs8", "ppk"];
  const MODES = [
    ["new", "keysModeNew"],
    ["convert", "keysModeConvert"],
  ] as const;
  const OTHER = "?other";

  const keys = $derived(ssh.inventory?.keys ?? []);
  const sources = $derived(keys.filter((k) => !k.orphan && readable(k.algorithm)));
  const sshDir = $derived(ssh.inventory ? shortPath(ssh.inventory.dir) : "~/.ssh");

  let mode = $state<"new" | "convert">(panel.kind === "convert" ? "convert" : "new");
  let usage = $state<Usage | null>(panel.kind === "generate" ? (panel.usage ?? null) : "ssh");
  let algo = $state<Algo | null>(null);
  let format = $state<Format | null>(null);
  let name = $state("");
  let named = $state(false);
  let comment = $state("");
  let passphrase = $state("");
  let confirm = $state("");
  let save = $state(false);
  let folder = $state("");
  let source = $state(panel.kind === "convert" ? (panel.path ?? "") : "");
  let sourcePassphrase = $state("");
  let result = $state<Generated | null>(null);
  let resultEncrypted = $state(false);
  let testing = $state(false);

  const sourceKey = $derived(keys.find((k) => k.path === source));
  /** What the key is made of: chosen, or for a conversion read from the inventory (unknown for another file). */
  const family = $derived<Algo | null>(mode === "new" ? chosenAlgo() : familyOf(sourceKey?.algorithm));
  const formats = $derived<Format[]>(
    usage
      ? [...USAGES[usage].formats, ...(mode === "convert" ? (["spki"] as Format[]) : [])].filter((f) => fits(f, family))
      : [],
  );
  const chosen = $derived<Format | undefined>(format && formats.includes(format) ? format : formats[0]);
  const toSsh = $derived(usage === "ssh" && chosen === "openssh");
  const encryptable = $derived(!!chosen && ENCRYPTABLE.includes(chosen));
  const suggested = $derived(suggest());
  const fileName = $derived(named ? name : suggested);
  const files = $derived(chosen ? FORMATS[chosen][2].map((s) => fileName + s) : []);
  const nameProblem = $derived(
    toSsh
      ? nameError(fileName, keys)
      : /^[A-Za-z0-9._@+-]{1,100}$/.test(fileName) && !fileName.startsWith(".")
        ? null
        : "sshNameInvalid",
  );
  const mismatch = $derived(encryptable && !!confirm && passphrase !== confirm);
  const ready = $derived(
    !!usage &&
      !!chosen &&
      !nameProblem &&
      (!encryptable || passphrase === confirm) &&
      (toSsh || !save || !!folder) &&
      (mode === "new" || !!source),
  );

  function chosenAlgo(): Algo {
    const list = usage ? (USAGES[usage].algos as Algo[]) : [];
    return algo && list.includes(algo) ? algo : (list[0] ?? "ed25519");
  }

  function familyOf(algorithm?: string): Algo | null {
    if (!algorithm) return null;
    if (algorithm.includes("ed25519")) return "ed25519";
    const curve = algorithm.match(/nistp(\d+)/);
    if (curve) return `p${curve[1]}` as Algo;
    return algorithm.includes("rsa") ? "rsa4096" : null;
  }

  function fits(f: Format, a: Algo | null): boolean {
    if (!a) return true;
    if (f === "pkcs1") return a.startsWith("rsa");
    if (f === "sec1") return /^p\d/.test(a);
    if (f === "openssh" || f === "ppk") return a !== "x25519";
    return true;
  }

  function suggest(): string {
    if (mode === "convert") {
      const base = (source.split(/[\\/]/).pop() ?? "key").replace(/\..*$/, "") || "key";
      return chosen === "openssh" ? `${base}-${new Date().getFullYear()}` : base;
    }
    const a = chosenAlgo();
    if (usage === "ssh") return `id_${a === "ed25519" ? "ed25519" : a.startsWith("rsa") ? "rsa" : "ecdsa"}`;
    return `${usage ?? "key"}-${a}`;
  }

  async function chooseFolder() {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string") {
      folder = picked;
      save = true;
    }
  }

  async function pickSource(list: HTMLSelectElement) {
    if (list.value !== OTHER) {
      source = list.value;
      return;
    }
    // Picked by the backend, which then lets the key commands read this file.
    try {
      const picked = await invoke<string | null>("keys_pick_file");
      if (picked) source = picked;
    } catch (e) {
      ssh.error = String(e);
    }
    list.value = source;
  }

  async function submit() {
    if (!ready || !usage || !chosen) return;
    const spec = {
      usage,
      format: chosen,
      name: fileName,
      comment: comment.trim(),
      passphrase: encryptable && passphrase ? passphrase : null,
      dir: !toSsh && save ? folder : null,
    };
    let made: Generated | null = null;
    const done = await act(async () => {
      made =
        mode === "new"
          ? await invoke<Generated>("keys_generate", { algorithm: chosenAlgo(), spec })
          : await invoke<Generated>("keys_convert", { path: source, passphrase: sourcePassphrase || null, spec });
      if (toSsh) return t("sshGenerated", { path: shortPath(made.files[0].path ?? "") });
    });
    if (!done || !made) return;
    const generated: Generated = made;
    resultEncrypted = !!spec.passphrase;
    passphrase = confirm = sourcePassphrase = "";
    if (toSsh) {
      ssh.selected = generated.files[0].path ?? "";
      ssh.tab = "keys";
      ssh.panel = null;
    } else {
      result = generated;
      testing = false;
    }
  }

  async function copyPublic(file: { name: string; text: string | null }) {
    ssh.error = "";
    try {
      await navigator.clipboard.writeText(file.text ?? "");
      ssh.done = t("sshCopied", { name: file.name });
    } catch (e) {
      ssh.error = String(e);
    }
  }

  function another() {
    result = null;
    named = false;
  }

  const close = () => (ssh.panel = null);
  const writtenPrivate = $derived(result?.files.find((f) => f.secret && f.path));
</script>

<section class="panel">
  <header>
    <h2>{mode === "convert" ? t("keysModeConvert") : t("sshGenerate")}</h2>
    <button class="icon-btn" onclick={close} aria-label={t("close")} title={t("close")}>
      <X size={16} aria-hidden="true" />
    </button>
  </header>

  {#if result}
    <h3 class="done">{mode === "convert" ? t("keysConverted") : t("keysGenerated")}</h3>
    <dl class="meta">
      <dt>{t("sshMetaType")}</dt>
      <dd>{result.algorithm}</dd>
      {#if result.fingerprint}
        <dt>{t("sshMetaFingerprint")}</dt>
        <dd class="mono">{result.fingerprint}</dd>
      {/if}
      <dt>{t("keysKid")}</dt>
      <dd class="mono">{result.kid}</dd>
    </dl>
    {#each result.files as file (file.name)}
      <div class="file">
        <div class="file-head">
          <span class="mono strong">{file.name}</span>
          <span class="badge" class:private={file.secret}>{file.secret ? t("keysPrivate") : t("keysPublic")}</span>
          {#if file.path}<span class="mono muted path">{shortPath(file.path)}</span>{/if}
          {#if !file.secret}
            <button class="btn sm push" onclick={() => copyPublic(file)}>
              <Copy size={14} aria-hidden="true" />{t("keysCopy")}
            </button>
          {/if}
        </div>
        {#if file.secret}
          {#if !file.path}<p class="hint warn-text">{t("keysNotSaved")}</p>{/if}
          <SecretView
            name={file.name}
            encrypted={!!file.path && resultEncrypted}
            load={(pass) =>
              file.path ? invoke<string>("keys_reveal", { path: file.path, passphrase: pass }) : Promise.resolve(file.text ?? "")}
          />
        {:else}
          <pre class="public">{file.text}</pre>
        {/if}
      </div>
    {/each}
    {#if writtenPrivate?.path}
      {#if testing}
        <KeyTest path={writtenPrivate.path} name={writtenPrivate.name} encrypted={resultEncrypted} />
      {/if}
    {/if}
    <footer>
      {#if writtenPrivate?.path && !testing}
        <button class="btn" onclick={() => (testing = true)}><FlaskConical size={14} aria-hidden="true" />{t("keysTest")}</button>
      {/if}
      <button class="btn" onclick={another}>{t("keysAnother")}</button>
      <button class="btn primary" onclick={close}>{t("close")}</button>
    </footer>
  {:else}
    <p class="lead">{t("keysGenerateHelp", { dir: sshDir })}</p>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <div class="segmented mode" role="radiogroup" aria-label={t("sshGenerate")}>
        {#each MODES as [value, label] (value)}
          <button type="button" role="radio" aria-checked={mode === value} onclick={() => ((mode = value), (named = false))}>
            {t(label)}
          </button>
        {/each}
      </div>

      {#if mode === "convert"}
        <div class="row">
          <label class="labelled grow">
            <span>{t("keysSource")}</span>
            <select class="field" value={source} onchange={(e) => pickSource(e.currentTarget)}>
              {#if !source}<option value="" disabled></option>{/if}
              {#each sources as key (key.path)}<option value={key.path}>{key.name}</option>{/each}
              {#if source && !sourceKey}<option value={source}>{shortPath(source)}</option>{/if}
              <option value={OTHER}>{t("keysOtherFile")}</option>
            </select>
          </label>
          {#if !sourceKey || sourceKey.encrypted}
            <label class="labelled">
              <span>{t("keysSourcePassphrase")}</span>
              <input class="field" type="password" autocomplete="off" bind:value={sourcePassphrase} />
            </label>
          {/if}
        </div>
      {/if}

      <div class="labelled">
        <span id="keys-usage">{t("keysUsage")}</span>
        <div class="usages" role="radiogroup" aria-labelledby="keys-usage">
          {#each Object.entries(USAGES) as [value, { icon: Icon }] (value)}
            {@const [title, help] = USAGE_TEXT[value as Usage]}
            <button
              type="button"
              role="radio"
              class="usage"
              aria-checked={usage === value}
              onclick={() => ((usage = value as Usage), (named = false))}
            >
              <Icon size={16} aria-hidden="true" />
              <span class="usage-title">{t(title)}</span>
              <span class="usage-help">{t(help)}</span>
            </button>
          {/each}
        </div>
        {#if !usage}<span class="hint">{t("keysPickUsage")}</span>{/if}
      </div>

      {#if usage}
        {#if mode === "new"}
          <div class="labelled">
            <span id="keys-algo">{t("keysAlgorithm")}</span>
            <div class="segmented wrap" role="radiogroup" aria-labelledby="keys-algo">
              {#each USAGES[usage].algos as value (value)}
                <button type="button" role="radio" aria-checked={chosenAlgo() === value} onclick={() => (algo = value)}>
                  {ALGOS[value]}
                </button>
              {/each}
            </div>
            {#if usage === "ssh"}<span class="hint">{t("sshTypeHelp")}</span>{/if}
          </div>
        {/if}

        <div class="labelled">
          <span id="keys-format">{t("keysFormat")}</span>
          <div class="segmented wrap" role="radiogroup" aria-labelledby="keys-format">
            {#each formats as value (value)}
              <button type="button" role="radio" aria-checked={chosen === value} onclick={() => (format = value)}>
                {FORMATS[value][0]}
              </button>
            {/each}
          </div>
          {#if chosen}<span class="hint">{t(FORMATS[chosen][1])}</span>{/if}
        </div>

        <div class="row">
          <label class="labelled grow">
            <span>{t("sshKeyName")}</span>
            <input
              class="field mono-field"
              value={fileName}
              oninput={(e) => ((name = e.currentTarget.value), (named = true))}
              aria-invalid={!!nameProblem}
              autocomplete="off"
              spellcheck="false"
            />
            {#if nameProblem}
              <span class="hint bad">{t(nameProblem)}</span>
            {:else if toSsh}
              <span class="hint">{t("sshKeyFiles", { path: `${sshDir}/${fileName}` })}</span>
            {:else}
              <span class="hint">{t("keysFiles", { files: files.join(", ") })}</span>
            {/if}
          </label>
          {#if chosen === "openssh" || chosen === "ppk"}
            <label class="labelled grow">
              <span>{t("sshComment")}</span>
              <input class="field" bind:value={comment} placeholder={mode === "convert" && sourceKey?.comment ? sourceKey.comment : t("sshCommentPlaceholder")} autocomplete="off" spellcheck="false" />
            </label>
          {/if}
        </div>

        {#if encryptable}
          <div class="pair">
            <label class="labelled">
              <span>{t("sshPassphrase")}</span>
              <input class="field" type="password" bind:value={passphrase} autocomplete="new-password" />
            </label>
            <label class="labelled">
              <span>{t("sshPassphraseConfirm")}</span>
              <input class="field" type="password" bind:value={confirm} autocomplete="new-password" aria-invalid={mismatch} />
            </label>
          </div>
          <span class="hint" class:bad={mismatch}>{mismatch ? t("sshPassMismatch") : t("sshPassphraseAdvice")}</span>
        {:else if chosen !== "spki"}
          <p class="hint">{t("keysNoPassphrase")}</p>
        {/if}

        {#if !toSsh}
          <fieldset class="labelled dest">
            <legend>{t("keysDestination")}</legend>
            <label class="choice">
              <input type="radio" name="keys-dest" checked={!save} onchange={() => (save = false)} />
              {t("keysShowOnly")}
            </label>
            <div class="choice">
              <label class="choice">
                <input type="radio" name="keys-dest" checked={save} onchange={() => (folder ? (save = true) : chooseFolder())} />
                {t("keysSaveIn")}
              </label>
              <button type="button" class="btn sm" onclick={chooseFolder}>
                <FolderOpen size={14} aria-hidden="true" />{t("keysChooseFolder")}
              </button>
              {#if folder}<span class="mono path">{shortPath(folder)}</span>{/if}
            </div>
          </fieldset>
        {/if}
      {/if}

      <footer>
        <button type="button" class="btn" onclick={close}>{t("cancel")}</button>
        <button type="submit" class="btn primary" disabled={!ready || ssh.busy}>
          {#if ssh.busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />
            {mode === "convert" ? t("keysConverting") : t("sshGenerating")}
          {:else}{mode === "convert" ? t("keysConvert") : t("sshGenerateRun")}{/if}
        </button>
      </footer>
    </form>
  {/if}
</section>

<style>
  .panel {
    flex: 1;
    min-height: 0;
    padding: 20px 28px 32px;
    overflow-y: auto;
  }

  .panel > * {
    max-width: 760px;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .lead {
    margin: 6px 0 16px;
    color: var(--muted);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .mode {
    align-self: flex-start;
  }

  .labelled {
    gap: 6px;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }

  .grow {
    flex: 1 1 260px;
    max-width: 420px;
  }

  .usages {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
  }

  .usage {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    color: var(--text);
    text-align: left;
    transition:
      border-color var(--dur-fast) ease-out,
      background-color var(--dur-fast) ease-out;
  }

  .usage:hover {
    background: var(--hover);
  }

  .usage[aria-checked="true"] {
    border-color: var(--accent);
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .usage :global(svg) {
    color: var(--accent-ink);
  }

  .usage-title {
    font-weight: 600;
  }

  .usage-help {
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 400;
    line-height: 1.35;
  }

  .segmented.wrap {
    flex-wrap: wrap;
    align-self: flex-start;
  }

  .pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
    max-width: 560px;
    margin-bottom: -10px;
  }

  .mono-field {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .hint {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 400;
  }

  .hint.bad {
    color: var(--danger);
  }

  .dest {
    margin: 0;
    padding: 0;
    border: 0;
  }

  .dest legend {
    margin-bottom: 6px;
    padding: 0;
  }

  .choice {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    font-weight: 400;
  }

  .dest > .choice + .choice {
    margin-top: 6px;
  }

  .choice input {
    margin: 0;
  }

  .path {
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .strong {
    font-weight: 600;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .done {
    margin: 16px 0 8px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 16px;
    margin: 0 0 16px;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .file {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 14px;
  }

  .file-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .badge.private {
    border-color: transparent;
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .push {
    margin-left: auto;
  }

  .warn-text {
    color: var(--warning);
    font-weight: 500;
  }

  .public {
    max-height: 200px;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg);
    font: var(--text-xs) / 1.55 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  @media (max-width: 900px) {
    .usages {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
