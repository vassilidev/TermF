import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { files } from "$lib/files.svelte";
import { t } from "$lib/i18n";

export type Scope = "user" | "project" | "local" | "managed";
export type FileState = "absent" | "invalid" | "ok";
export type ScopeFile = { scope: Scope; path: string; state: FileState; error: string | null; value: unknown };
export type Effective = { key: string; value: unknown; scope: Scope; others: Scope[]; merged: boolean };
export type Settings = { files: ScopeFile[]; effective: Effective[] };
export type Health = "connected" | "failed" | "pending" | "needsauth" | "unknown";
export type McpServer = {
  name: string;
  target: string;
  status: string;
  health: Health;
  scope: string | null;
  details: Record<string, unknown> | null;
};
export type Folder = {
  kind: "skills" | "agents" | "commands" | "output-styles" | "rules";
  scope: Scope;
  path: string;
  items: { name: string; description: string | null; path: string }[];
};
export type Hook = { scope: Scope; event: string; matcher: string | null; kind: string; run: string };
export type FileInfo = { scope: Scope | null; path: string; state: FileState; size: number | null };
export type Overview = {
  claudeDir: string;
  repo: string | null;
  settings: Settings;
  mcp: { servers: McpServer[]; error: string | null };
  plugins: { plugins: Record<string, unknown>[]; marketplaces: Record<string, unknown>[]; error: string | null };
  extensions: Folder[];
  hooks: Hook[];
  memory: { files: FileInfo[]; auto: { project: string; path: string; files: [string, number][] }[] };
  interface: { keybindings: FileInfo; themes: string[] };
  account: {
    auth: Record<string, unknown> | null;
    authError: string | null;
    version: string | null;
    install: string | null;
    path: string | null;
  };
  tools: { name: string; version: string | null; path: string | null }[];
};
export type Preview = { path: string; diff: string };

export type Owner = "me" | "shared";
export type Artifact = {
  id: string;
  url: string;
  title: string | null;
  owner: Owner | null;
  updated: string | null;
  cwd: string | null;
  seen: number;
};
export type Failure =
  | "toolMissing"
  | "notStarted"
  | "timeout"
  | "budget"
  | "needsYou"
  | "closed"
  | "invalid"
  | "spawn"
  | "cancelled";
export type Refresh = {
  running: boolean;
  manual: boolean;
  session: number | null;
  started: number;
  ready: boolean;
  revealed: boolean;
  cost: number | null;
  last: { at: number; count: number | null; failure: Failure | null; detail: string | null; cost: number | null } | null;
};
export type Artifacts = { artifacts: Artifact[]; listed: number | null; refresh: Refresh; refreshHours: number };

export const GALLERY = "https://claude.ai/code/artifacts";

/** The keys the view edits, with the values Claude Code's settings schema accepts. */
export const EDITABLE: Record<string, { kind: "text" | "number" | "bool" | "enum"; values?: string[] }> = {
  model: { kind: "text" },
  effortLevel: { kind: "enum", values: ["low", "medium", "high", "xhigh"] },
  "permissions.defaultMode": {
    kind: "enum",
    values: ["default", "acceptEdits", "plan", "auto", "dontAsk", "bypassPermissions"],
  },
  autoMemoryEnabled: { kind: "bool" },
  cleanupPeriodDays: { kind: "number" },
  autoUpdatesChannel: { kind: "enum", values: ["stable", "latest"] },
};

export const claudeView = $state({
  /** Loaded once first shown, then kept mounted, like the other views. */
  loaded: false,
  tab: "config" as "config" | "artifacts",
  /** The repository whose project and local scopes are read; "" for none. */
  repo: "",
  overview: null as Overview | null,
  loading: false,
  error: "",
  done: "",
  artifacts: null as Artifacts | null,
});

export function showClaude(tab?: "config" | "artifacts") {
  files.view = "claude";
  claudeView.loaded = true;
  if (tab) claudeView.tab = tab;
  if (!claudeView.overview && !claudeView.loading) loadOverview();
  loadArtifacts();
}

let request = 0;

export async function loadOverview() {
  const id = ++request;
  claudeView.loading = true;
  claudeView.error = "";
  try {
    const overview = await invoke<Overview>("claude_overview", { repo: claudeView.repo || null });
    if (id === request) claudeView.overview = overview;
  } catch (e) {
    if (id === request) claudeView.error = String(e);
  } finally {
    if (id === request) claudeView.loading = false;
  }
}

export async function loadArtifacts() {
  try {
    claudeView.artifacts = await invoke<Artifacts>("artifacts_list");
  } catch (e) {
    claudeView.error = String(e);
  }
}

export async function openUrl(url: string) {
  try {
    await invoke("open_url", { url });
  } catch (e) {
    claudeView.error = String(e);
  }
}

export async function copyText(text: string, done: string) {
  try {
    await navigator.clipboard.writeText(text);
    claudeView.done = done;
  } catch (e) {
    claudeView.error = String(e);
  }
}

export const scopeLabel = (scope: string) =>
  ({
    user: t("claudeViewScopeUser"),
    project: t("claudeViewScopeProject"),
    local: t("claudeViewScopeLocal"),
    managed: t("claudeViewScopeManaged"),
  })[scope] ?? scope;

export const stateLabel = (state: FileState) =>
  ({ absent: t("claudeViewAbsent"), invalid: t("claudeViewInvalid"), ok: t("claudeViewOk") })[state];

export const healthLabel = (health: Health) =>
  ({
    connected: t("claudeViewMcpConnected"),
    failed: t("claudeViewMcpFailed"),
    pending: t("claudeViewMcpPending"),
    needsauth: t("claudeViewMcpAuth"),
    unknown: t("claudeViewMcpUnknown"),
  })[health];

export const kindLabel = (kind: Folder["kind"]) =>
  ({
    skills: t("claudeViewSkills"),
    agents: t("claudeViewAgents"),
    commands: t("claudeViewCommands"),
    "output-styles": t("claudeViewStyles"),
    rules: t("claudeViewRules"),
  })[kind];

/** A value as one line of JSON (strings without quotes). */
export function show(value: unknown): string {
  return typeof value === "string" ? value : JSON.stringify(value);
}

export function size(bytes: number | null): string {
  if (bytes === null) return "";
  return bytes < 1024 ? t("claudeViewBytes", { n: bytes }) : t("claudeViewKb", { n: (bytes / 1024).toFixed(1) });
}

export function effective(o: Overview, key: string): Effective | undefined {
  return o.settings.effective.find((e) => e.key === key);
}

export const BLOCKS = ["settings", "mcp", "plugins", "extensions", "hooks", "memory", "interface", "account", "tools"] as const;
export type Block = (typeof BLOCKS)[number];

export const blockTitle = (block: Block) =>
  ({
    settings: t("claudeViewSettings"),
    mcp: t("claudeViewMcp"),
    plugins: t("claudeViewPlugins"),
    extensions: t("claudeViewExtensions"),
    hooks: t("claudeViewHooks"),
    memory: t("claudeViewMemory"),
    interface: t("claudeViewInterface"),
    account: t("claudeViewAccount"),
    tools: t("claudeViewTools"),
  })[block];

const cell = (text: unknown) => show(text ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
const table = (head: string[], rows: unknown[][]) =>
  rows.length
    ? [`| ${head.join(" | ")} |`, `|${head.map(() => " --- |").join("")}`, ...rows.map((r) => `| ${r.map(cell).join(" | ")} |`)].join(
        "\n",
      )
    : `_${t("claudeViewNone")}_`;

/** One block as Markdown: what the copy button puts on the clipboard, and a section of the export. */
export function blockMarkdown(o: Overview, block: Block): string {
  const lines: string[] = [`## ${blockTitle(block)}`, ""];
  const push = (...more: string[]) => lines.push(...more, "");
  switch (block) {
    case "settings":
      push(table([t("claudeViewScope"), t("claudeViewFile"), t("claudeViewState")], o.settings.files.map((f) => [scopeLabel(f.scope), f.path, f.error ? `${stateLabel(f.state)} (${f.error})` : stateLabel(f.state)])));
      push(
        table(
          [t("claudeViewKey"), t("claudeViewValue"), t("claudeViewDecidedBy"), t("claudeViewAlsoIn")],
          o.settings.effective.map((e) => [e.key, show(e.value), scopeLabel(e.scope), e.others.map(scopeLabel).join(", ")]),
        ),
      );
      break;
    case "mcp":
      if (o.mcp.error) push(`> ${o.mcp.error}`);
      push(table([t("claudeViewName"), t("claudeViewState"), t("claudeViewScope"), t("claudeViewTarget")], o.mcp.servers.map((s) => [s.name, healthLabel(s.health), s.scope ? scopeLabel(s.scope) : "", s.target])));
      for (const s of o.mcp.servers.filter((s) => s.details)) {
        push(`### ${s.name}`, "```", ...Object.entries(s.details ?? {}).map(([k, v]) => `${k}: ${show(v)}`), "```");
      }
      break;
    case "plugins":
      if (o.plugins.error) push(`> ${o.plugins.error}`);
      push(`### ${t("claudeViewPluginsInstalled")}`, "", table(["id", t("claudeViewVersion"), t("claudeViewScope"), t("claudeViewEnabled")], o.plugins.plugins.map((p) => [p.id, p.version, p.scope, p.enabled])));
      push(`### ${t("claudeViewMarketplaces")}`, "", table([t("claudeViewName"), t("claudeViewSource")], o.plugins.marketplaces.map((m) => [m.name, m.repo ?? m.url ?? m.path ?? m.source])));
      break;
    case "extensions":
      if (!o.extensions.length) push(`_${t("claudeViewNone")}_`);
      for (const f of o.extensions) {
        push(`### ${kindLabel(f.kind)} · ${scopeLabel(f.scope)}`, `\`${f.path}\``, "", table([t("claudeViewName"), t("claudeViewDescription")], f.items.map((i) => [i.name, i.description ?? ""])));
      }
      break;
    case "hooks":
      push(table([t("claudeViewEvent"), "matcher", t("claudeViewScope"), t("claudeViewRun")], o.hooks.map((h) => [h.event, h.matcher ?? "", scopeLabel(h.scope), `${h.kind}: ${h.run}`])));
      break;
    case "memory":
      push(table([t("claudeViewScope"), t("claudeViewFile"), t("claudeViewState"), t("claudeViewSize")], o.memory.files.map((f) => [f.scope ? scopeLabel(f.scope) : "", f.path, stateLabel(f.state), size(f.size)])));
      push(`### ${t("claudeViewAutoMemory")}`, "", table([t("claudeViewFolder"), t("claudeViewFiles"), t("claudeViewSize")], o.memory.auto.map((m) => [m.path, m.files.map(([n]) => n).join(", "), size(m.files.reduce((s, [, b]) => s + b, 0))])));
      break;
    case "interface": {
      const status = effective(o, "statusLine.command");
      const theme = effective(o, "theme");
      push(
        `- ${t("claudeViewStatusLine")}: ${status ? `\`${show(status.value)}\` (${scopeLabel(status.scope)})` : t("claudeViewDefault")}`,
        `- ${t("claudeViewTheme")}: ${theme ? `${show(theme.value)} (${scopeLabel(theme.scope)})` : t("claudeViewDefault")}`,
        `- ${t("claudeViewCustomThemes")}: ${o.interface.themes.join(", ") || t("claudeViewNone")}`,
        `- ${t("claudeViewKeybindings")}: \`${o.interface.keybindings.path}\` (${stateLabel(o.interface.keybindings.state)})`,
      );
      break;
    }
    case "account": {
      const channel = effective(o, "autoUpdatesChannel");
      if (o.account.authError) push(`> ${o.account.authError}`);
      push(
        ...Object.entries(o.account.auth ?? {})
          .filter(([, v]) => v === null || typeof v !== "object")
          .map(([k, v]) => `- ${k}: ${show(v)}`),
        `- ${t("claudeViewVersion")}: ${o.account.version ?? t("claudeViewUnknown")}`,
        `- ${t("claudeViewChannel")}: ${channel ? show(channel.value) : `latest (${t("claudeViewDefault")})`}`,
        `- ${t("claudeViewInstall")}: ${o.account.install ?? t("claudeViewUnknown")}${o.account.path ? ` (\`${o.account.path}\`)` : ""}`,
      );
      break;
    }
    case "tools":
      push(table([t("claudeViewName"), t("claudeViewVersion"), t("claudeViewPath")], o.tools.map((x) => [x.name, x.version ?? t("claudeViewMissing"), x.path ?? ""])));
      break;
  }
  return lines.join("\n").trimEnd() + "\n";
}

export function overviewMarkdown(o: Overview): string {
  const head = [`# ${t("claudeViewExportTitle")}`, "", `${t("claudeViewFolderClaude")}: \`${o.claudeDir}\``];
  if (o.repo) head.push(`${t("claudeViewRepo")}: \`${o.repo}\``);
  head.push(`${t("claudeViewMasked")}`, "");
  return head.join("\n\n") + BLOCKS.map((b) => blockMarkdown(o, b)).join("\n");
}

/** Saves the whole overview with the native dialog. The backend masks the text once more. */
export async function exportOverview(format: "md" | "json") {
  const o = claudeView.overview;
  if (!o) return;
  const path = await save({
    defaultPath: `claude-config.${format}`,
    filters: [{ name: format === "md" ? "Markdown" : "JSON", extensions: [format] }],
  });
  if (!path) return;
  const text = format === "md" ? overviewMarkdown(o) : JSON.stringify(o, null, 2);
  try {
    await invoke("claude_export", { path, text });
    claudeView.done = t("claudeViewExported", { path });
  } catch (e) {
    claudeView.error = String(e);
  }
}
