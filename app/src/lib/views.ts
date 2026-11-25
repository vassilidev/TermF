import type { Component } from "svelte";
import Bot from "@lucide/svelte/icons/bot";
import CalendarClock from "@lucide/svelte/icons/calendar-clock";
import FolderTree from "@lucide/svelte/icons/folder-tree";
import GitBranch from "@lucide/svelte/icons/git-branch";
import House from "@lucide/svelte/icons/house";
import KeyRound from "@lucide/svelte/icons/key-round";
import LayoutGrid from "@lucide/svelte/icons/layout-grid";
import LockKeyhole from "@lucide/svelte/icons/lock-keyhole";
import LibraryBig from "@lucide/svelte/icons/library-big";
import MessagesSquare from "@lucide/svelte/icons/messages-square";
import PackageCheck from "@lucide/svelte/icons/package-check";
import Palette from "@lucide/svelte/icons/palette";
import Server from "@lucide/svelte/icons/server";
import SettingsIcon from "@lucide/svelte/icons/settings";
import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
import Ticket from "@lucide/svelte/icons/ticket";
import Workflow from "@lucide/svelte/icons/workflow";
import type { t } from "$lib/i18n";
import { agents, showConversations } from "$lib/agents.svelte";
import { ci, showCi } from "$lib/ci.svelte";
import { claudeView, showClaude } from "$lib/claude.svelte";
import { design, showDesign } from "$lib/design.svelte";
import { devenv, showEnv } from "$lib/devenv.svelte";
import { files, showFiles } from "$lib/files.svelte";
import { git, showGit } from "$lib/git.svelte";
import { projectWaits, showHome } from "$lib/home.svelte";
import { library, showLibrary } from "$lib/library.svelte";
import { ops, showOps } from "$lib/ops.svelte";
import { showConfig } from "$lib/project.svelte";
import { schedule, showScheduler } from "$lib/schedule.svelte";
import { settings, showSettings } from "$lib/settings.svelte";
import { showSsh, ssh } from "$lib/ssh.svelte";
import { app, ui, type ViewId } from "$lib/state.svelte";
import { showTickets, tickets } from "$lib/tickets.svelte";
import { showVault, showVaultAll, vault } from "$lib/vault.svelte";

type Key = Parameters<typeof t>[0];

export type View = {
  id: ViewId;
  label: Key;
  /** What it is for, in a few words: the navigation's tooltip and the palette's second line. */
  hint?: Key;
  /** Set apart from the views before it: the project's tools after its daily work. */
  apart?: boolean;
  /** The palette's command that shows it. */
  command: Key;
  icon: Component;
  /**
   * "project": under the current project in the navigation; "linked": out of it, reached from the project's pages
   * (Home, Configuration) and the palette; "machine": under "This machine", at the bottom.
   */
  group: "project" | "linked" | "machine";
  show: () => void;
  /** Mounted once first shown, then kept: terminals keep their scrollback, views their state. */
  loaded: () => boolean;
  load: () => Promise<{ default: Component }>;
  count?: () => number | undefined;
  /** The count asks for the user (amber). */
  hot?: boolean;
};

export function showTerminals() {
  ui.view = "terminals";
}

/** Every view, in the navigation's order; a view not listed here appears nowhere. */
export const views: View[] = [
  {
    id: "home",
    label: "ux3t1Home",
    hint: "uxfT1HintHome",
    command: "ux3t1ShowHome",
    icon: House,
    group: "project",
    show: showHome,
    loaded: () => true,
    load: () => import("$lib/HomeView.svelte"),
    count: () => projectWaits(app.project).length || undefined,
    hot: true,
  },
  {
    id: "terminals",
    label: "viewTerminals",
    hint: "uxfT1HintTerminals",
    command: "showTerminals",
    icon: LayoutGrid,
    group: "project",
    show: showTerminals,
    loaded: () => true,
    load: () => import("$lib/SessionGrid.svelte"),
  },
  {
    id: "tickets",
    label: "viewTickets",
    hint: "uxfT1HintTickets",
    command: "showTickets",
    icon: Ticket,
    group: "project",
    show: showTickets,
    loaded: () => tickets.loaded,
    load: () => import("$lib/TicketsView.svelte"),
    count: () => tickets.inbox?.length || undefined,
  },
  {
    id: "files",
    label: "viewFiles",
    hint: "uxfT1HintFiles",
    command: "showFiles",
    icon: FolderTree,
    group: "project",
    show: showFiles,
    loaded: () => files.loaded,
    load: () => import("$lib/FilesView.svelte"),
  },
  {
    id: "git",
    label: "gitView",
    hint: "uxfT1HintGit",
    command: "gitShow",
    icon: GitBranch,
    group: "project",
    show: showGit,
    loaded: () => git.loaded,
    load: () => import("$lib/GitView.svelte"),
  },
  {
    id: "ci",
    label: "viewCi",
    hint: "uxfT1HintCi",
    command: "showCi",
    icon: Workflow,
    group: "project",
    show: showCi,
    loaded: () => ci.loaded,
    load: () => import("$lib/CiView.svelte"),
  },
  {
    id: "design",
    label: "designView",
    hint: "uxfT1HintDesign",
    apart: true,
    command: "designShow",
    icon: Palette,
    group: "project",
    show: showDesign,
    loaded: () => design.loaded,
    load: () => import("$lib/DesignView.svelte"),
  },
  {
    id: "vault",
    label: "vaultTitle",
    hint: "uxfT1HintVault",
    command: "vaultShow",
    icon: LockKeyhole,
    group: "project",
    show: showVault,
    loaded: () => vault.loaded,
    load: () => import("$lib/VaultView.svelte"),
  },
  {
    id: "ops",
    label: "opsView",
    hint: "uxfT1HintOps",
    command: "opsShow",
    icon: Server,
    group: "project",
    show: showOps,
    loaded: () => ops.loaded,
    load: () => import("$lib/OpsView.svelte"),
    count: () => ops.requests.filter((r) => r.project === app.project).length || undefined,
    hot: true,
  },
  {
    id: "project",
    label: "ux3t3Config",
    hint: "uxfT1HintConfig",
    command: "ticketsEditProject",
    icon: SlidersHorizontal,
    group: "project",
    show: showConfig,
    // Mounted apart by +page.svelte, once per project and only while shown: it holds a form.
    loaded: () => false,
    load: () => import("$lib/ProjectView.svelte"),
  },
  {
    id: "conversations",
    label: "chatView",
    hint: "uxfT1HintConversations",
    command: "chatShow",
    icon: MessagesSquare,
    group: "linked",
    show: showConversations,
    loaded: () => agents.loaded,
    load: () => import("$lib/ConversationsView.svelte"),
  },
  {
    id: "library",
    label: "viewLibrary",
    hint: "uxfT1HintLibrary",
    command: "showLibrary",
    icon: LibraryBig,
    group: "linked",
    show: showLibrary,
    loaded: () => library.loaded,
    load: () => import("$lib/LibraryView.svelte"),
  },
  {
    id: "settings",
    label: "settings",
    hint: "uxfT1HintSettings",
    command: "settings",
    icon: SettingsIcon,
    group: "machine",
    show: () => showSettings(),
    loaded: () => settings.loaded,
    load: () => import("$lib/SettingsView.svelte"),
  },
  {
    id: "claude",
    label: "claudeViewTitle",
    hint: "uxfT1HintClaude",
    command: "claudeViewShow",
    icon: Bot,
    group: "machine",
    show: () => showClaude(claudeView.section === "artifacts" ? "overview" : undefined),
    loaded: () => claudeView.loaded,
    load: () => import("$lib/ClaudeView.svelte"),
  },
  {
    id: "env",
    label: "myenvView",
    hint: "myenvHint",
    command: "myenvShow",
    icon: PackageCheck,
    group: "machine",
    show: () => showEnv(),
    loaded: () => devenv.loaded,
    load: () => import("$lib/EnvView.svelte"),
  },
  {
    id: "ssh",
    label: "viewSsh",
    hint: "uxfT1HintSsh",
    command: "showSsh",
    icon: KeyRound,
    group: "machine",
    show: showSsh,
    loaded: () => ssh.loaded,
    load: () => import("$lib/SshView.svelte"),
  },
  {
    id: "scheduler",
    label: "schedulerView",
    hint: "uxfT1HintScheduler",
    command: "schedulerShow",
    icon: CalendarClock,
    group: "machine",
    show: showScheduler,
    loaded: () => schedule.loaded,
    load: () => import("$lib/SchedulerView.svelte"),
  },
  {
    id: "vaultAll",
    label: "uxfT1VaultAll",
    hint: "uxfT1HintVaultAll",
    command: "vaultShowAll",
    icon: LockKeyhole,
    group: "machine",
    show: showVaultAll,
    loaded: () => vault.loadedAll,
    load: () => import("$lib/VaultView.svelte"),
  },
];

