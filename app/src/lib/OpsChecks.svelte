<script lang="ts">
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { t } from "$lib/i18n";
  import { missingSecret, type Check } from "$lib/ops-logic";

  /**
   * A connection's test: what works, and what is missing or worth knowing, with what the client said. `fill`: a
   * secret the vault lacks is typed here, kept there, and the test runs again.
   */
  let { checks, fill }: { checks: Check[]; fill?: (name: string, value: string) => Promise<void> } = $props();

  const lacking = $derived(fill ? missingSecret(checks) : null);
  let value = $state("");
  let saving = $state(false);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!lacking || !fill) return;
    saving = true;
    try {
      await fill(lacking, value);
    } finally {
      value = "";
      saving = false;
    }
  }

  const labels = {
    tool: "opsCheckTool",
    secret: "opsCheckSecret",
    tunnel: "opsCheckTunnel",
    connect: "opsCheckConnect",
    readonly: "opsCheckReadonly",
    journal: "opsCheckJournal",
  } as const;

  const hints = {
    keyRefused: "opsdbHintKeyRefused",
    hostKey: "opsdbHintHostKey",
    unknownHost: "opsdbHintUnknownHost",
    portClosed: "opsdbHintPortClosed",
    timeout: "opsdbHintTimeout",
    passwordRefused: "opsdbHintPasswordRefused",
    noDatabase: "opsdbHintNoDatabase",
    noFile: "opsdbHintNoFile",
  } as const;

  function text(c: Check): { label: string; detail: string } {
    if (c.warn && c.code === "readonly") return { label: t("opsCheckReadonlyWarn", { subject: c.subject }), detail: "" };
    if (c.warn && c.code === "journal") return { label: t("opsCheckJournalWarn"), detail: "" };
    const detail = c.code === "tool" && !c.ok ? t("opsCheckMissing", { detail: c.detail }) : c.detail;
    return { label: t(labels[c.code], { subject: c.subject }), detail };
  }
</script>

<ul class="checks" aria-label={t("opsTest")}>
  {#each checks as c, i (i)}
    {@const line = text(c)}
    <li class:fail={!c.ok && !c.warn} class:warn={c.warn}>
      {#if c.warn}<TriangleAlert size={14} aria-hidden="true" />{:else if c.ok}<CircleCheck size={14} aria-hidden="true" />{:else}<CircleX
          size={14}
          aria-hidden="true"
        />{/if}
      <span class="t">
        <span>{line.label}</span>
        {#if c.hint}<span class="hint">{t(hints[c.hint])}</span>{/if}
        {#if line.detail}<span class="detail mono">{line.detail}</span>{/if}
        {#if c.code === "secret" && c.subject === lacking}
          <form class="fill" onsubmit={save}>
            <span class="hint">{t("vaultCardsMissing")}</span>
            <span class="row">
              <input
                class="field mono"
                type="password"
                autocomplete="off"
                spellcheck="false"
                aria-label={t("vaultCardsValueOf", { name: c.subject })}
                placeholder={t("vaultCardsValueOf", { name: c.subject })}
                bind:value
              />
              <button class="btn sm primary" disabled={saving || !value}>{t("vaultCardsSaveTest")}</button>
            </span>
          </form>
        {/if}
      </span>
    </li>
  {/each}
</ul>

<style>
  .checks {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    list-style: none;
    font-size: var(--text-sm);
  }

  li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    color: var(--done);
  }

  li :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  li.fail {
    color: var(--danger);
  }

  li.warn {
    color: var(--warning);
  }

  .t {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    color: var(--text);
  }

  .hint {
    color: var(--text);
    font-weight: 500;
  }

  .fill {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 4px;
  }

  .fill .row {
    display: flex;
    gap: 6px;
    max-width: 420px;
  }

  .detail {
    color: var(--muted);
    font-size: var(--text-xs);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
