<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ShieldAlert from "@lucide/svelte/icons/shield-alert";
  import X from "@lucide/svelte/icons/x";
  import { t } from "$lib/i18n";
  import type { Item } from "$lib/library.svelte";
  import type { RemoteMode, RemotePolicy } from "$lib/remote-logic";
  import { savePolicy } from "$lib/remote.svelte";
  import { app } from "$lib/state.svelte";

  /** A project's Remote Control default, and the roles that follow another: mounted by its Configuration page. */
  let { project }: { project: string } = $props();

  const modes: [RemoteMode, "remoteModeOff" | "remoteModeAsk" | "remoteModeOn"][] = [
    ["off", "remoteModeOff"],
    ["ask", "remoteModeAsk"],
    ["on", "remoteModeOn"],
  ];
  const policy = $derived<RemotePolicy>(
    app.workspace?.projects.find((p) => p.id === project)?.remoteControl ?? { default: "off", roles: {} },
  );
  let roles = $state<string[]>([]);
  let error = $state("");

  // svelte-ignore state_referenced_locally
  invoke<Item[]>("library_list", { kind: "role", project }).then(
    (list) => (roles = list.map((r) => r.name)),
    () => {},
  );

  const others = $derived(roles.filter((r) => !(r in policy.roles)));

  async function save(next: RemotePolicy) {
    error = await savePolicy(project, next);
  }

  const setRole = (role: string, mode: RemoteMode) => save({ ...policy, roles: { ...policy.roles, [role]: mode } });

  function removeRole(role: string) {
    const rest = { ...policy.roles };
    delete rest[role];
    save({ ...policy, roles: rest });
  }
</script>

{#snippet choice(current: RemoteMode, pick: (m: RemoteMode) => void, label: string)}
  <div class="segmented pick" role="radiogroup" aria-label={label}>
    {#each modes as [mode, word] (mode)}
      <button role="radio" aria-checked={current === mode} onclick={() => pick(mode)}>{t(word)}</button>
    {/each}
  </div>
{/snippet}

<h2 id="config-remote">{t("remoteControl")}</h2>
<p class="help">{t("remoteSettingsIntro")}</p>
{@render choice(policy.default, (m) => save({ ...policy, default: m }), t("remoteControl"))}
<p class="help">{t("remoteModeHelp")}</p>

<h3>{t("remoteRoles")}</h3>
<p class="help">{t("remoteRolesHelp")}</p>
{#if Object.keys(policy.roles).length}
  <ul class="roles">
    {#each Object.entries(policy.roles) as [role, mode] (role)}
      <li>
        <span class="role">{role}</span>
        {@render choice(mode, (m) => setRole(role, m), role)}
        <button
          class="icon-btn sm"
          aria-label={t("remoteRoleRemove", { role })}
          title={t("remoteRoleRemove", { role })}
          onclick={() => removeRole(role)}
        >
          <X size={14} aria-hidden="true" />
        </button>
      </li>
    {/each}
  </ul>
{/if}
{#if others.length}
  <select
    class="field add"
    aria-label={t("remoteRoleAdd")}
    onchange={(e) => {
      const role = e.currentTarget.value;
      e.currentTarget.value = "";
      if (role) setRole(role, policy.default === "on" ? "off" : "on");
    }}
  >
    <option value="">{t("remoteRoleAdd")}</option>
    {#each others as role (role)}<option value={role}>{role}</option>{/each}
  </select>
{/if}
<p class="notice"><ShieldAlert size={14} aria-hidden="true" />{t("remoteNeverProd")}</p>
{#if error}<p class="notice error" role="alert">{error}</p>{/if}

<style>
  /* Laid out by the Configuration's section (a column 16px apart), whose titles it matches. */
  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 8px 0 0;
    font-size: var(--text-md);
    font-weight: 650;
  }

  .help {
    margin: -8px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.45;
  }

  .pick {
    align-self: flex-start;
  }

  .roles {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .roles li {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .role {
    min-width: 8em;
    font: 600 var(--text-sm) var(--font-mono);
  }

  .add {
    align-self: flex-start;
    width: auto;
  }

  .notice {
    align-items: center;
  }
</style>
