import { invoke } from "@tauri-apps/api/core";
import type { Action } from "$lib/actions.svelte";
import type { Shown } from "$lib/claude.svelte";
import { t } from "$lib/i18n";
import { library, type Kind } from "$lib/library.svelte";
import { app } from "$lib/state.svelte";
import { toast } from "$lib/ui/toast.svelte";

const KINDS: Kind[] = ["skill", "command", "role", "preference", "rule", "workflow"];
const kindName: Record<Kind, Parameters<typeof t>[0]> = {
  skill: "kindSkill",
  command: "kindCommand",
  role: "kindRole",
  preference: "kindPreference",
  rule: "kindRule",
  workflow: "kindWorkflow",
};
const projectName = (id: string) => app.workspace?.projects.find((p) => p.id === id)?.name ?? id;

/** What installing a plugin runs, as Claude Code showed it: the second call accepts exactly that. */
const shown: Record<string, Shown> = {};

const toggle: Action = {
  id: "library.toggle",
  description:
    "Turns an item of the library (skill, command, role, preference, rule, workflow) on or off for the project, or for every project (`everywhere`); `default` gives the choice back to the layer below.",
  fields: {
    kind: { type: "string", description: "The item's kind.", enum: KINDS },
    name: { type: "string", description: "The item's name, as the library lists it." },
    state: { type: "string", description: "on, off, or default.", enum: ["on", "off", "default"] },
    everywhere: { type: "boolean", description: "For every project rather than this one." },
  },
  required: ["kind", "name", "state"],
  label: "actLibToggle",
  describe: (a, project) =>
    t(a.state === "on" ? "actLibOnText" : a.state === "off" ? "actLibOffText" : "actLibDefaultText", {
      kind: t(kindName[a.kind as Kind] ?? "kindSkill"),
      name: String(a.name),
      where: a.everywhere ? t("actLibEverywhere") : projectName(project),
    }),
  async run(a, _secrets, project) {
    const enabled = a.state === "default" ? null : a.state === "on";
    await invoke("library_toggle", { kind: a.kind, name: a.name, project: a.everywhere ? null : project, enabled });
    library.version++;
    const name = String(a.name);
    toast(
      enabled === null
        ? t("lib2ToastDefault", { name })
        : a.everywhere
          ? t(enabled ? "lib2ToastOnEverywhere" : "lib2ToastOffEverywhere", { name })
          : t(enabled ? "lib2ToastOnHere" : "lib2ToastOffHere", { name, project: projectName(project) }),
    );
    return { item: `${a.kind}/${name}`, state: a.state, scope: a.everywhere ? "every project" : "this project" };
  },
};

const installPlugin: Action = {
  id: "library.install_plugin",
  description:
    "Installs a Claude Code plugin (plugin@marketplace) for the user (`user`), the project (`project`, shared in its repository) or this machine's copy of it (`local`). The first call shows the user what the plugin runs and gives you its `sha256`; call again with `accept` set to it once the user agreed.",
  fields: {
    plugin: { type: "string", description: "plugin@marketplace, as the marketplace names it." },
    scope: { type: "string", description: "Where it goes, user by default.", enum: ["user", "project", "local"] },
    accept: { type: "string", description: "The sha256 the first call gave, once the user saw what it runs." },
  },
  required: ["plugin"],
  label: "actLibInstall",
  describe: (a) => {
    const plugin = String(a.plugin);
    const seen = a.accept ? shown[plugin] : undefined;
    if (!a.accept) return t("actLibInstallFirstText", { plugin, scope: String(a.scope ?? "user") });
    return seen?.sha256 === a.accept && seen.command
      ? `${t("actLibInstallText", { plugin, scope: String(a.scope ?? "user") })}\n${seen.command}`
      : t("actLibInstallText", { plugin, scope: String(a.scope ?? "user") });
  },
  async run(a, _secrets, project) {
    const plugin = String(a.plugin);
    const action = { op: "install", plugin, scope: a.scope ?? "user", ...(a.accept ? { accept: a.accept } : {}) };
    // The project's and the local scopes are those of its first repository, as in the Claude view.
    const repo = a.scope && a.scope !== "user" ? (app.workspace?.projects.find((p) => p.id === project)?.repos[0]?.path ?? null) : null;
    const r = await invoke<{ message: string; shown: Shown | null }>("claude_plugin", { repo, action });
    if (r.shown) {
      shown[plugin] = r.shown;
      return {
        installed: false,
        runs: r.shown.command ?? null,
        sha256: r.shown.sha256,
        next: "Tell the user what it runs; once they agree, call again with accept set to this sha256.",
      };
    }
    delete shown[plugin];
    library.version++;
    toast(t("claudeMgmtInstalled", { id: plugin }));
    return { installed: true, said: r.message };
  },
};

export const libraryActions: Action[] = [toggle, installPlugin];
