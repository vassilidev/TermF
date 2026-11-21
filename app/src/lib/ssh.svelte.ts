import { invoke } from "@tauri-apps/api/core";
import { files } from "$lib/files.svelte";

export type KeyType = "ed25519" | "ecdsa" | "rsa";
export type SshKey = {
  path: string;
  name: string;
  publicPath: string | null;
  orphan: boolean;
  algorithm: string;
  bits: number | null;
  fingerprint: string | null;
  comment: string;
  publicKey: string | null;
  encrypted: boolean | null;
  mode: number | null;
  tooOpen: boolean;
  modified: number | null;
  inAgent: boolean;
  hosts: string[];
  repos: string[];
};
export type AgentKey = { bits: number | null; fingerprint: string; comment: string; kind: string };
export type Host = {
  patterns: string[];
  hostName: string | null;
  user: string | null;
  port: string | null;
  identityFiles: string[];
  identitiesOnly: boolean | null;
  file: string;
  line: number;
};
export type HostEntry = {
  alias: string;
  hostName: string | null;
  user: string | null;
  port: number | null;
  identityFile: string | null;
  identitiesOnly: boolean | null;
};
export type RepoKeys = {
  path: string;
  remotes: { name: string; url: string; host: string }[];
  sshCommand: string | null;
  identityFiles: string[];
};
export type Inventory = {
  dir: string;
  keys: SshKey[];
  agent: { error: string | null; keys: AgentKey[] };
  hosts: Host[];
  repos: RepoKeys[];
  problems: { path: string; error: string }[];
};
export type HostTest = { ok: boolean; message: string };
export type Rotation = {
  key: string;
  uploaded: boolean;
  hosts: string[];
  repos: string[];
  warning: string | null;
  errors: string[];
};

type Panel =
  | null
  | { kind: "generate" }
  | { kind: "host"; host: Host | null }
  | { kind: "rotate"; key: SshKey };

export const ssh = $state({
  /** Loaded once first shown, then kept mounted, like the Library view. */
  loaded: false,
  inventory: null as Inventory | null,
  tab: "keys" as "keys" | "agent" | "hosts" | "repos",
  panel: null as Panel,
  /** The key shown in the detail pane, by path. */
  selected: "",
  busy: false,
  error: "",
  /** What the last action did, shown until dismissed or replaced. */
  done: "",
});

export function showSsh() {
  files.view = "ssh";
  ssh.loaded = true;
  refresh();
}

export function generateKey() {
  showSsh();
  ssh.panel = { kind: "generate" };
}

export async function refresh() {
  try {
    ssh.inventory = await invoke<Inventory>("ssh_inventory");
  } catch (e) {
    ssh.error = String(e);
  }
}

/** Runs an action, shows what it says it did, then reloads the inventory. Resolves to false when it failed. */
export async function act(run: () => Promise<string | void>): Promise<boolean> {
  ssh.busy = true;
  ssh.error = "";
  ssh.done = "";
  try {
    ssh.done = (await run()) || "";
    return true;
  } catch (e) {
    ssh.error = String(e);
    return false;
  } finally {
    await refresh();
    ssh.busy = false;
  }
}

/** `ssh-ed25519` → ED25519, `sk-ecdsa-sha2-nistp256@openssh.com` → ECDSA-SK. */
export function algorithm(name: string): string {
  const sk = name.startsWith("sk-") ? "-SK" : "";
  const base = name
    .replace(/^sk-/, "")
    .replace(/^ssh-/, "")
    .replace(/@.*/, "")
    .replace(/-sha2-nistp\d+$/, "");
  return (base === "dss" ? "DSA" : base.toUpperCase()) + sk;
}

/** What `generate` accepts as a key name, and not taken yet. */
export function nameError(name: string, keys: SshKey[]): "sshNameInvalid" | "sshNameTaken" | null {
  if (
    !/^[A-Za-z0-9._@+-]{1,100}$/.test(name) ||
    name.startsWith(".") ||
    name.endsWith(".pub") ||
    ["config", "known_hosts", "known_hosts.old", "authorized_keys", "authorized_keys2", "environment", "rc"].includes(name)
  ) {
    return "sshNameInvalid";
  }
  return keys.some((k) => k.name === name || k.name === `${name}.pub`) ? "sshNameTaken" : null;
}
