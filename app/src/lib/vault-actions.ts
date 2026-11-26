import { invoke } from "@tauri-apps/api/core";
import { newMaster, vaultNeeds, type Args } from "$lib/actions-logic";
import type { Action } from "$lib/actions.svelte";
import { agentEnv, type Grant } from "$lib/agentenv.svelte";
import { deploys } from "$lib/ci.svelte";
import { t } from "$lib/i18n";
import { ops } from "$lib/ops.svelte";
import { app } from "$lib/state.svelte";
import { toast } from "$lib/ui/toast.svelte";
import { refreshVault, setStatus, vault, type VaultStatus } from "$lib/vault.svelte";
import { secretNameOk, type Scope, type SecretInfo } from "$lib/vault-logic";

const SCOPES: Scope[] = ["dev", "staging", "prod"];

const status = () => invoke<VaultStatus>("vault_status");
const open = async () => vaultNeeds(await status());
/** Open, with its secrets' names read for the card's warning when one is replaced, whatever the screen. */
async function opened(_a: Args, project: string) {
  agentEnv.given[project] = (await invoke<{ grants: Grant[] }>("agentenv_grants", { project })).grants;
  const why = await open();
  if (!why && !vault.secrets.length) vault.secrets = await invoke<SecretInfo[]>("vault_list");
  return why;
}

/** The project's agents that get this secret in their environment, by variable (`[agents.secrets]`). */
const givenAs = (a: Args, project: string) =>
  (agentEnv.given[project] ?? []).filter((g) => g.secret === String(a.name ?? "").trim() && g.scope === a.scope).map((g) => g.variable);
const projectName = (id: string) => app.workspace?.projects.find((p) => p.id === id)?.name ?? id;
/** Where a secret goes: the project, or every project (`everywhere`). */
const where = (a: Args, project: string) => (a.everywhere ? t("vaultGlobal") : projectName(project));
const owner = (a: Args, project: string) => (a.everywhere ? null : project);
const prod = (a: Args) => a.scope === "prod";

function named(a: Args): string {
  const name = String(a.name ?? "").trim();
  if (!secretNameOk(name)) throw new Error("name: letters, digits, _ . and -, starting with a letter or _");
  return name;
}

/** Prod asks the master password again: the card asks it with the value, then the vault takes it first. */
async function confirmProd(a: Args, secrets: Record<string, string>) {
  if (prod(a) && secrets.master) setStatus(await invoke<VaultStatus>("vault_reauth", { password: secrets.master }));
}

/** What reads this secret, as TermF knows it here: connections and deploy targets of the project. */
function readers(name: string, project: string): string[] {
  const conns = ops.project === project ? ops.connections.filter((c) => Object.values(c.secrets ?? {}).includes(name)).map((c) => c.name) : [];
  const targets = deploys.targetsOf === project ? (deploys.targets ?? []).filter((x) => x.token === name).map((x) => x.name) : [];
  return [...conns, ...targets];
}

const create: Action = {
  id: "vault.create",
  description:
    "Creates TermF's vault, where the project's passwords and tokens go, when there is none yet. The user types its master password twice in TermF's card: you never see it.",
  fields: {
    password: { type: "string", secret: true, description: "The vault's master password, 8 characters or more." },
    confirm: { type: "string", secret: true, description: "The same, again." },
  },
  label: "actVaultCreate",
  labels: { password: "vaultNewMasterPassword", confirm: "vaultPasswordAgain" },
  describe: () => t("actVaultCreateText"),
  needs: async () => ((await status()).initialized ? "The vault exists already: propose vault.unlock if it is locked." : null),
  async run(_a, secrets) {
    const master = newMaster(secrets);
    if ("why" in master) throw new Error(t(master.why === "short" ? "actVaultShort" : "vaultPasswordsDiffer"));
    setStatus(await invoke<VaultStatus>("vault_init", { password: master.password, remember: false }));
    await refreshVault();
    toast(t("actVaultCreated"));
    return { created: true, next: "Call again the action that needed it." };
  },
};

const unlock: Action = {
  id: "vault.unlock",
  description: "Opens TermF's vault with its master password, which the user types in TermF's card: you never see it.",
  fields: { password: { type: "string", secret: true, description: "The vault's master password." } },
  label: "actVaultUnlock",
  labels: { password: "vaultMasterPassword" },
  describe: () => t("actVaultUnlockText"),
  needs: async () => {
    const s = await status();
    if (!s.initialized) return "There is no vault yet: propose vault.create instead.";
    if (s.unlocked) return "The vault is open already.";
    // A security key asks for a touch, and maybe a PIN, that only the Vault view's screen asks for.
    return s.keys > 0 ? "This vault opens with a security key too: ask the user to open it in the Vault view." : null;
  },
  async run(_a, secrets) {
    setStatus(await invoke<VaultStatus>("vault_unlock", { password: secrets.password ?? "", pin: null }));
    await refreshVault();
    toast(t("actVaultUnlocked"));
    return { unlocked: true };
  },
};

const lock: Action = {
  id: "vault.lock",
  description: "Locks TermF's vault: agents and recipes read nothing from it until the user opens it again.",
  fields: {},
  label: "actVaultLock",
  describe: () => t("actVaultLockText"),
  loses: () => t("actVaultLockLoses"),
  needs: async () => ((await status()).unlocked ? null : "The vault is not open: nothing to lock."),
  async run() {
    setStatus(await invoke<VaultStatus>("vault_lock"));
    toast(t("actVaultLocked"));
    return { locked: true };
  },
};

const setSecret: Action = {
  id: "vault.set_secret",
  description:
    "Adds a secret to the vault, or replaces the value of one, from any screen, as the Vault view does: for the project (or every project with `everywhere`), in an environment. Propose it whenever an action or a test says the vault lacks a secret (a connection's password, a token): the user types its value in TermF's card, you never see it, never ask for it in the conversation, and say in `hints.value` where they find it. With `reference` (op://… of 1Password, a Bitwarden or Keychain reference), the vault keeps where to read it instead. prod asks the master password again, in the same card.",
  fields: {
    name: { type: "string", description: "As the agents' environment names it: STRIPE_SECRET_KEY." },
    scope: { type: "string", description: "The environment it belongs to.", enum: SCOPES },
    everywhere: { type: "boolean", description: "For every project rather than this one." },
    description: { type: "string", description: "One line on what it is." },
    reference: { type: "string", description: "Where another manager keeps it (op://vault/item/field); no value is typed then." },
    value: { type: "string", secret: true, description: "The secret's value." },
    master: { type: "string", secret: true, description: "prod only: the vault's master password, asked again." },
  },
  required: ["name", "scope"],
  asks: (a) => [...(a.reference ? [] : ["value"]), ...(prod(a) ? ["master"] : [])],
  label: "actVaultSet",
  labels: { value: "vaultValue", master: "vaultMasterPassword" },
  describe: (a, project) => {
    const vars = { name: String(a.name), scope: String(a.scope), where: where(a, project) };
    return a.reference ? t("actVaultRefText", { ...vars, uri: String(a.reference) }) : t("actVaultSetText", vars);
  },
  loses: (a, project) =>
    vault.secrets.some((s) => s.name === a.name && s.scope === a.scope && s.project === owner(a, project)) ? t("actVaultReplaceLoses") : "",
  gives: (a, project) => {
    const variables = givenAs(a, project);
    return variables.length ? t("vaultCardsGiven", { variables: variables.join(", ") }) : "";
  },
  needs: opened,
  async run(a, secrets, project) {
    const name = named(a);
    const args = { name, scope: a.scope, project: owner(a, project), description: typeof a.description === "string" ? a.description.trim() || null : null };
    if (!a.reference && !secrets.value) throw new Error(t("actVaultNoValue"));
    await confirmProd(a, secrets);
    const saved = a.reference
      ? await invoke<SecretInfo>("vault_ref", { ...args, uri: String(a.reference).trim() })
      : await invoke<SecretInfo>("vault_set", { ...args, value: secrets.value });
    await refreshVault();
    toast(t("restVaultSecretSaved", { name: saved.name, scope: saved.scope, where: where(a, project) }));
    return { saved: saved.name, scope: saved.scope, project: saved.project, note: "Its value stays in the vault: agents get it in their environment once granted." };
  },
};

const deleteSecret: Action = {
  id: "vault.delete_secret",
  description: "Deletes a secret of the vault, as the Vault view does. prod asks the master password again, in TermF's card.",
  fields: {
    name: { type: "string", description: "The secret's name, as `here` lists it." },
    scope: { type: "string", description: "Its environment.", enum: SCOPES },
    everywhere: { type: "boolean", description: "The one of every project rather than this project's." },
    master: { type: "string", secret: true, description: "prod only: the vault's master password, asked again." },
  },
  required: ["name", "scope"],
  asks: (a) => (prod(a) ? ["master"] : []),
  label: "actVaultDelete",
  labels: { master: "vaultMasterPassword" },
  describe: (a, project) => t("actVaultDeleteText", { name: String(a.name), scope: String(a.scope), where: where(a, project) }),
  loses: (a, project) => {
    const users = readers(String(a.name), project);
    return t("actVaultDeleteLoses", { users: users.length ? users.join(", ") : t("actVaultDeleteAnyone") });
  },
  needs: open,
  async run(a, secrets, project) {
    const name = named(a);
    await confirmProd(a, secrets);
    await invoke("vault_delete", { name, scope: a.scope, project: owner(a, project) });
    await refreshVault();
    toast(t("restVaultSecretDeleted", { name }));
    return { deleted: name };
  },
};

/** The vault's secrets by name, scope and project: never a value. */
export async function vaultHere(project: string) {
  const why = await open();
  if (why) return { vault: why };
  const list = await invoke<SecretInfo[]>("vault_list");
  return {
    secrets: list.filter((s) => s.project === null || s.project === project).map((s) => ({ name: s.name, scope: s.scope, everywhere: s.project === null })),
  };
}

export const vaultActions: Action[] = [deleteSecret];
/** Offered from every screen: an action elsewhere may need the vault first, or a secret it lacks. */
export const vaultAnywhere: Action[] = [create, unlock, lock, setSecret];
