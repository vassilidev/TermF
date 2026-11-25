import type { Action } from "$lib/actions.svelte";
import type { Args } from "$lib/actions-logic";
import { GROUPS, MANAGERS, blank, clean, keyOf, lines, parseKey, type Entry, type Group, type Key, type Manager } from "$lib/devenv-logic";
import {
  addEntries,
  deleteScript,
  devenv,
  install,
  loadEnv,
  missingHere,
  newFound,
  removeEntries,
  saveScript,
  scan,
  updateEntry,
} from "$lib/devenv.svelte";
import { t } from "$lib/i18n";
import { toast } from "$lib/ui/toast.svelte";

const text = (v: unknown) => (typeof v === "string" && v.trim() ? v.trim() : null);
const texts = (v: unknown) => (v && typeof v === "object" ? (v as Record<string, string>) : {});
const listed = () => devenv.manifest?.entries ?? [];

async function entries(): Promise<Entry[]> {
  return (devenv.manifest ?? (await loadEnv()))?.entries ?? [];
}

function keys(v: unknown): Key[] {
  return (Array.isArray(v) ? v : []).map((k) => {
    const key = parseKey(String(k));
    if (!key) throw new Error(`${String(k)}: manager:name, the manager one of ${MANAGERS.join(", ")}`);
    return key;
  });
}

const same = (k: Key) => (e: Key) => e.manager === k.manager && e.name === k.name;

/** Each entry an `env.add` writes: as the manifest or the detection has it, else new; then what the maps give it. */
function built(a: Args, list: Entry[]): Entry[] {
  const [groups, versions, descriptions, installs, paths, ids] = [a.groups, a.versions, a.descriptions, a.installs, a.paths, a.ids].map(texts);
  return keys(a.entries).map((k) => {
    const key = keyOf(k);
    const known = list.find(same(k)) ?? devenv.found.find(same(k));
    const base: Entry = known ? { ...known } : { ...blank(k.manager), name: k.name };
    const group = text(groups[key]);
    if (group && !GROUPS.includes(group as Group)) throw new Error(`${key}: the group is one of ${GROUPS.join(", ")}`);
    return clean({
      ...base,
      group: (group as Group | null) ?? base.group,
      version: text(versions[key]) ?? base.version,
      description: text(descriptions[key]) ?? base.description,
      install: text(installs[key]) ?? base.install,
      path: text(paths[key]) ?? base.path,
      id: text(ids[key]) ?? base.id,
      active: true,
    });
  });
}

const MAPS = "manager:name →";

const add: Action = {
  id: "env.add",
  description:
    "Writes entries into the user's development environment (env.toml, the My environment screen), or replaces those of the same manager and name; each is taken as the detection found it (`here.new`), else made new. Managers: tap, brew, cask, mas, vscode (Homebrew's Brewfile: taps, formulae, casks, App Store apps, VS Code extensions); mise, asdf, fnm, nvm, volta, rustup (language versions); npm, pnpm, bun, cargo, pipx, uv, gh (an extension's owner/repo), composer (global packages); shell (a command of the user's own: give its install command); manual (installed by hand: give its official install command once you know it). Keep the tools a developer uses; leave out dependencies and libraries other packages pulled in. A version only when it matters (a language's), and a few words on what each one is.",
  fields: {
    entries: { type: "array", description: "Each entry as manager:name, e.g. brew:ripgrep, shell:claude, manual:Herd." },
    groups: { type: "object", description: `${MAPS} languages, cli, apps or editors; by default its manager's.` },
    versions: { type: "object", description: `${MAPS} the version to install; none: the current one.` },
    descriptions: { type: "object", description: `${MAPS} what it is, in a few words.` },
    installs: {
      type: "object",
      description: `${MAPS} the command that installs it (shell, manual), e.g. the official installer: curl -fsSL https://claude.ai/install.sh | bash.`,
    },
    paths: { type: "object", description: `${MAPS} where a shell or manual entry lands (~/… or /…): there, the script skips it.` },
    ids: { type: "object", description: `${MAPS} a mas app's App Store id.` },
  },
  required: ["entries"],
  label: "actEnvAdd",
  describe: (a) => {
    const list = built(a, listed());
    return `${t("actEnvAddText", { n: list.length })}\n${lines(list)}`;
  },
  async run(a) {
    const list = built(a, await entries());
    await addEntries(list);
    devenv.tab = "manifest";
    toast(t("myenvKept", { n: list.length }));
    return { added: list.map(keyOf), file: devenv.manifest?.file };
  },
};

const edit: Action = {
  id: "env.edit",
  description:
    "Changes one entry of env.toml, as My environment's form does: the fields given replace its own (an empty text clears it); `active` false keeps it listed but out of the install script.",
  fields: {
    entry: { type: "string", description: "The entry, as manager:name." },
    name: { type: "string", description: "Its new name." },
    manager: { type: "string", description: "Its new manager.", enum: [...MANAGERS] },
    group: { type: "string", description: "Its group.", enum: [...GROUPS] },
    version: { type: "string", description: "The version to install; empty: the current one." },
    description: { type: "string", description: "What it is, in a few words." },
    install: { type: "string", description: "The command that installs it (shell, manual)." },
    path: { type: "string", description: "Where a shell or manual entry lands." },
    id: { type: "string", description: "A mas app's App Store id." },
    active: { type: "boolean", description: "In the install script." },
  },
  required: ["entry"],
  label: "actEnvEdit",
  describe: (a) =>
    `${t("actEnvEditText", { name: String(a.entry) })}\n${Object.entries(a)
      .filter(([k, v]) => k !== "entry" && v !== null && v !== undefined)
      .map(([k, v]) => `${k} → ${String(v)}`)
      .join("\n")}`,
  async run(a) {
    const [previous] = keys([a.entry]);
    const found = (await entries()).find(same(previous));
    if (!found) throw new Error(`${keyOf(previous)} is not in env.toml: here.entries lists what is.`);
    const pick = (k: string, was: string | null) => (typeof a[k] === "string" ? text(a[k]) : was);
    const entry = clean({
      ...found,
      name: text(a.name) ?? found.name,
      manager: (text(a.manager) as Manager | null) ?? found.manager,
      group: (text(a.group) as Group | null) ?? found.group,
      version: pick("version", found.version),
      description: pick("description", found.description),
      install: pick("install", found.install),
      path: pick("path", found.path),
      id: pick("id", found.id),
      active: typeof a.active === "boolean" ? a.active : found.active,
    });
    await updateEntry(previous, entry);
    toast(t("myenvSaved", { name: entry.name }));
    return { saved: keyOf(entry) };
  },
};

const remove: Action = {
  id: "env.remove",
  description: "Removes entries from env.toml: they leave the install script. To keep one listed but not installed, env.edit with active false.",
  fields: { entries: { type: "array", description: "Each entry as manager:name." } },
  required: ["entries"],
  label: "actEnvRemove",
  describe: (a) => t("actEnvRemoveText", { names: keys(a.entries).map(keyOf).join(", ") }),
  loses: () => t("actEnvRemoveLoses"),
  async run(a) {
    const list = keys(a.entries);
    await removeEntries(list);
    toast(t("myenvRemoved", { name: list.map((k) => k.name).join(", ") }));
    return { removed: list.map(keyOf) };
  },
};

const detect: Action = {
  id: "env.scan",
  description:
    "Reads what this machine has installed, as My environment's Detect does: each package manager present lists what it installed (read only), with the language versions, and the programs and apps installed by hand. Returns what env.toml does not list yet, and what it lists that is missing here.",
  fields: {},
  label: "actEnvScan",
  describe: () => t("actEnvScanText"),
  async run() {
    await entries();
    await scan();
    devenv.tab = "found";
    const errors = Object.values(devenv.sources).flatMap((s) => (s?.error ? [`${s.source}: ${s.error}`] : []));
    return { new: lines(newFound()), missing: missingHere().map(keyOf), ...(errors.length ? { errors } : {}) };
  },
};

const SCRIPT_NAME = "lowercase letters, digits and dashes, e.g. laptop or new-mac";

const save: Action = {
  id: "env.save_script",
  description:
    "Saves an install script beside env.toml, synced with the config, so that My environment runs it on any machine. Before writing one, ask the user a few short questions in one message, with numbered choices: which machine and system; which groups to keep (languages, cli, apps, editors) and what to leave out on a laptop; whether to pin versions; whether it starts by installing TermF (`curl -fsSL https://vassilidev.github.io/TermF/install.sh | sh`) and `termf sync setup --git <url>` to the user's sync remote, for a new machine from scratch. Start from what `termf env script` prints (TermF's script from env.toml): POSIX sh for macOS and Linux, idempotent (skip what is there), stopping on the first error and saying which. Show the script in the conversation too.",
  fields: {
    name: { type: "string", description: `The script's name: ${SCRIPT_NAME}.` },
    script: { type: "string", description: "The whole script, POSIX sh." },
  },
  required: ["name", "script"],
  label: "actEnvSaveScript",
  describe: (a) => `${t("actEnvSaveScriptText", { name: String(a.name) })}\n\n${String(a.script)}`,
  loses: (a) => (devenv.manifest?.scripts.includes(String(a.name)) ? t("actEnvSaveScriptLoses", { name: String(a.name) }) : ""),
  async run(a) {
    const name = String(a.name).trim();
    await saveScript(name, String(a.script));
    devenv.tab = "script";
    toast(t("myenvScriptSaved", { name }));
    return { saved: name, note: "My environment's Script tab shows it, and runs it once the user asks." };
  },
};

const drop: Action = {
  id: "env.delete_script",
  description: "Deletes a named install script kept beside env.toml.",
  fields: { name: { type: "string", description: "The script's name." } },
  required: ["name"],
  label: "actEnvDeleteScript",
  describe: (a) => t("actEnvDeleteScriptText", { name: String(a.name) }),
  loses: (a) => t("actEnvDeleteScriptLoses", { name: String(a.name) }),
  async run(a) {
    await deleteScript(String(a.name));
    toast(t("myenvScriptDeleted", { name: String(a.name) }));
    return { deleted: a.name };
  },
};

const run: Action = {
  id: "env.install",
  description:
    "Installs what is missing on this machine: runs the install script TermF writes from env.toml (or a named one) in a terminal the user sees, from the home folder. It skips what is there, may ask the user's password (Homebrew, casks) and stops on the first error. macOS and Linux.",
  fields: { script: { type: "string", description: `A named script (${SCRIPT_NAME}); none: the one written from env.toml.` } },
  label: "actEnvInstall",
  describe: (a) => {
    const name = text(a.script);
    const missing = missingHere();
    const said = t("actEnvInstallText", { script: name ?? "env.toml" });
    return !name && missing.length ? `${said}\n${t("myenvConfirmMissing", { list: missing.map((e) => e.name).join(", ") })}` : said;
  },
  async run(a) {
    const session = await install(text(a.script));
    toast(t("myenvStarted"));
    return { terminal: session.id, note: "It runs in a terminal of TermF; the user follows it there." };
  },
};

export const envActions: Action[] = [detect, add, edit, remove, save, drop, run];

/** What My environment holds: env.toml's entries, what the detection found that it lacks, and what is missing. */
export async function envHere() {
  const list = await entries();
  return {
    file: devenv.manifest?.file,
    entries: lines(list),
    detected: devenv.scanned,
    new: lines(newFound()),
    missing: missingHere().map(keyOf),
    scripts: devenv.manifest?.scripts ?? [],
  };
}
