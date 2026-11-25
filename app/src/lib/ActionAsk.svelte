<script lang="ts">
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { refused, secretsOf, type Args } from "$lib/actions-logic";
  import { actionOf, prerequisite } from "$lib/actions.svelte";
  import { t } from "$lib/i18n";

  /**
   * An agent's `do_action` as TermF's card says it: the action, exactly what it does, what it makes lose, and the
   * fields TermF asks the user for itself (a password, a token), with where the agent says to find them. What the user
   * types stays here until the action runs, never in the conversation.
   */
  let {
    session,
    project,
    who,
    input,
    values = $bindable({}),
  }: { session: number; project: string; who: string; input: unknown; values?: Record<string, string> } = $props();

  const call = $derived((input ?? {}) as { action?: unknown; args?: Args; hints?: Record<string, string> });
  const action = $derived(actionOf(session, call.action));
  const args = $derived(call.args ?? {});
  /** What the action waits for (no vault yet…): TermF refuses it, so nothing is asked for nothing. */
  let missing = $state<string | null>(null);
  let read = 0;
  $effect(() => {
    const [a, given, at, n] = [action, args, project, ++read];
    missing = null;
    if (a?.needs) void prerequisite(a, given, at).then((m) => n === read && (missing = m));
  });
  const problem = $derived((action ? refused(action, args, call.hints) : null) ?? missing);
  const said = $derived.by(() => {
    try {
      return action?.describe(args, project) ?? "";
    } catch {
      return "";
    }
  });
  const lost = $derived(action?.loses?.(args, project) ?? "");
  const asked = $derived(action && !problem ? secretsOf(action, args) : []);
</script>

{#if action}
  <p class="ask-what"><b>{t(action.label)}</b></p>
  {#if said}<p class="said">{said}</p>{/if}
  {#if lost}<p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{lost}</p>{/if}
  {#if problem}<p class="notice error">{t("actInvalid", { why: problem })}</p>{/if}
  {#each asked as field (field)}
    {@const label = action.labels?.[field]}
    <label class="labelled secret">
      <span>{label ? t(label) : field}</span>
      <input
        class="field mono"
        type="password"
        autocomplete="off"
        spellcheck="false"
        bind:value={() => values[field] ?? "", (v) => (values = { ...values, [field]: v })}
      />
    </label>
    {#if call.hints?.[field]}<p class="muted small">{t("actHint", { who, hint: call.hints[field] })}</p>{/if}
  {/each}
  {#if asked.length}<p class="muted small">{t("actSecretsKept")}</p>{/if}
{:else}
  <p class="notice error">{t("actUnknown", { action: String(call.action ?? "") })}</p>
{/if}

<style>
  .said {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .secret {
    max-width: 360px;
  }

  .small {
    margin: 0;
    font-size: var(--text-xs);
  }

  .mono {
    font-family: var(--font-mono);
  }
</style>
