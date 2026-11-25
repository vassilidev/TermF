<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import Bookmark from "@lucide/svelte/icons/bookmark";
  import Check from "@lucide/svelte/icons/check";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import CircleCheckBig from "@lucide/svelte/icons/circle-check-big";
  import Play from "@lucide/svelte/icons/play";
  import ShieldX from "@lucide/svelte/icons/shield-x";
  import SquarePen from "@lucide/svelte/icons/square-pen";
  import SquareArrowOutUpRight from "@lucide/svelte/icons/square-arrow-out-up-right";
  import Conversation from "$lib/Conversation.svelte";
  import { agents, showConversation, upsert, type AgentSession } from "$lib/agents.svelte";
  import type { Image } from "$lib/conversation-logic";
  import { lang, t } from "$lib/i18n";
  import { asked, sqlBlock, type Connection } from "$lib/ops-logic";
  import { ops } from "$lib/ops.svelte";

  /** The SQL console's DBA: a conversation with the `dba` role about one database, in the same conversation as
   * everywhere else, each query checked by the console before it can run. */
  let {
    project,
    conn,
    check,
    ask: askVault,
    onuse,
    onrun,
    onsave,
  }: {
    project: string;
    conn: Connection;
    /** The console's verdict on a query: `null` when it reads, else why it is refused. */
    check: (sql: string) => Promise<string | null>;
    ask: (e: unknown, retry: () => Promise<void>) => boolean;
    onuse: (sql: string) => void;
    onrun: (sql: string) => void;
    onsave: (sql: string) => void;
  } = $props();

  const key = $derived(`${project}/${conn.name}`);
  // An archived one's topic is done: the next question starts a new conversation, with the schema.
  const session = $derived(agents.sessions.find((s) => s.id === ops.studio.dba[key] && !s.archived));
  let conv = $state<ReturnType<typeof Conversation>>();
  let sending = $state(false);
  let error = $state("");
  let draft = $state("");
  const working = $derived(sending || session?.state === "working" || session?.state === "waiting");

  // Asked once per query: the answers are drawn again as the conversation streams.
  const verdicts = new Map<string, Promise<string | null>>();
  function verdict(sql: string) {
    if (!verdicts.has(sql)) verdicts.set(sql, check(sql));
    return verdicts.get(sql)!;
  }

  /** Through `ops_ask`: the first question starts the conversation, with the schema. */
  async function post(kind: "write" | "explain" | "optimize", what: string, images: Image[] = []) {
    const s = await invoke<AgentSession>("ops_ask", {
      project,
      connection: conn.name,
      session: session?.id ?? null,
      kind,
      text: what.trim(),
      lang,
      images,
    });
    upsert(s);
    ops.studio.dba[key] = s.id;
  }

  /** Asks the DBA: a query from words, or what the editor holds explained or made faster. */
  export async function ask(kind: "write" | "explain" | "optimize", what: string) {
    if (!what.trim() || working) return;
    ops.studio.aside = "dba";
    sending = true;
    error = "";
    try {
      await post(kind, what);
      if (kind === "write") draft = "";
    } catch (e) {
      if (!askVault(e, () => ask(kind, what))) error = String(e);
    } finally {
      sending = false;
    }
  }

  function quote(text: string) {
    const said = asked(text);
    return said.kind === "write" ? said.text : t(said.kind === "explain" ? "opsdbExplainAsk" : "opsdbOptimizeAsk");
  }

  const access = $derived(
    conn.ai_access === "full" ? t("opsAccessFull") : conn.ai_access === "redacted" ? t("opsAccessRedacted") : "",
  );
</script>

{#snippet query(text: string)}
  {@const sql = sqlBlock(text)}
  {#if sql}
    {#await verdict(sql) then refused}
      <div class="query">
        {#if refused}
          <p class="verdict refused"><ShieldX size={13} aria-hidden="true" />{t("opsdbAskRefused", { reason: refused })}</p>
        {:else}
          <p class="verdict ok"><Check size={13} aria-hidden="true" />{t("opsdbAskChecked")}</p>
        {/if}
        <div class="actions">
          <button class="btn sm" onclick={() => onuse(sql)}><SquarePen size={13} aria-hidden="true" />{t("opsdbAskUse")}</button>
          <button class="btn sm primary" disabled={!!refused} onclick={() => onrun(sql)}><Play size={13} aria-hidden="true" />{t("opsdbRun")}</button>
          <button class="btn sm" onclick={() => onsave(sql)}><Bookmark size={13} aria-hidden="true" />{t("opsdbSave")}</button>
        </div>
      </div>
    {/await}
  {/if}
{/snippet}

<section class="dba" aria-label={t("opsdbAsk")}>
  <header>
    <p class="muted">
      {t("opsdbAskSees", { name: conn.name })}
      {conn.ai_access === "none" || !conn.ai_access ? t("opsdbAskNoData") : t("opsdbAskData", { access })}
    </p>
    {#if session}
      {@const id = session.id}
      <div class="head-actions">
        <button class="icon-btn" title={t("opsdbAskOpen")} aria-label={t("opsdbAskOpen")} onclick={() => showConversation(id)}>
          <SquareArrowOutUpRight size={14} aria-hidden="true" />
        </button>
        <button class="icon-btn" title={t("topicDoneHelp")} aria-label={t("topicDone")} onclick={() => conv?.endTopic()}>
          <CircleCheckBig size={14} aria-hidden="true" />
        </button>
      </div>
    {/if}
  </header>

  {#if error}<p class="notice error" role="alert">{error}</p>{/if}

  {#if session}
    {#key session.id}
      <Conversation
        bind:this={conv}
        {session}
        bare
        fresh={false}
        placeholder={t("opsdbAskFollow")}
        outgoing={(text, images) => post("write", text, images).then(() => null)}
        after={query}
        {quote}
      />
    {/key}
  {:else}
    <div class="thread">
      <p class="intro">{t("opsdbAskIntro")}</p>
    </div>
    <form
      class="composer"
      onsubmit={(e) => {
        e.preventDefault();
        ask("write", draft);
      }}
    >
      <textarea
        class="field"
        rows="2"
        bind:value={draft}
        placeholder={t("opsdbAskPlaceholder")}
        aria-label={t("opsdbAsk")}
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
            e.preventDefault();
            e.currentTarget.form?.requestSubmit();
          }
        }}
      ></textarea>
      <button type="submit" class="icon-send" aria-label={t("chatSend")} title={t("chatSend")} disabled={working || !draft.trim()}>
        {#if sending}<LoaderCircle size={16} aria-hidden="true" class="spin" />{:else}<ArrowUp size={16} aria-hidden="true" />{/if}
      </button>
    </form>
  {/if}
</section>

<style>
  .dba {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
  }

  header {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 0 12px 10px;
  }

  header p {
    flex: 1;
    margin: 0;
    font-size: var(--text-xs);
    line-height: 1.5;
  }

  .head-actions {
    display: flex;
    gap: 2px;
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }

  .notice {
    margin: 0 12px 10px;
  }

  .thread {
    flex: 1;
    min-height: 0;
    padding: 6px 12px;
  }

  .intro {
    margin: 0;
    padding: 12px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-lg);
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.55;
  }

  .query {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .verdict {
    display: flex;
    align-items: flex-start;
    gap: 5px;
    margin: 0;
    font-size: var(--text-xs);
  }

  .verdict :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .verdict.ok {
    color: var(--done);
  }

  .verdict.refused {
    color: var(--danger);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  /* The field of the conversation's composer, until the conversation starts. */
  .composer {
    position: relative;
    padding: 10px 12px 12px;
    border-top: 1px solid var(--border);
    background: var(--panel);
  }

  .composer textarea {
    width: 100%;
    min-height: 58px;
    max-height: 200px;
    padding: 8px 48px 8px 12px;
    border-radius: var(--radius-lg);
    line-height: 1.5;
    resize: vertical;
  }

  .icon-send {
    position: absolute;
    right: 20px;
    bottom: 20px;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--accent-text);
    cursor: pointer;
    transition: background-color var(--dur-fast) ease-out;
  }

  .icon-send:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .icon-send:disabled {
    background: var(--hover);
    color: var(--muted);
    cursor: default;
  }

  .icon-send:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
