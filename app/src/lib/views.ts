import type { Component } from "svelte";
import Bot from "@lucide/svelte/icons/bot";
import CalendarClock from "@lucide/svelte/icons/calendar-clock";
import FolderTree from "@lucide/svelte/icons/folder-tree";
import GitBranch from "@lucide/svelte/icons/git-branch";
import House from "@lucide/svelte/icons/house";
import KeyRound from "@lucide/svelte/icons/key-round";
import LayoutGrid from "@lucide/svelte/icons/layout-grid";
import LibraryBig from "@lucide/svelte/icons/library-big";
import MessagesSquare from "@lucide/svelte/icons/messages-square";
import SettingsIcon from "@lucide/svelte/icons/settings";
import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
import Ticket from "@lucide/svelte/icons/ticket";
import Workflow from "@lucide/svelte/icons/workflow";
import type { t } from "$lib/i18n";
import { agents, showConversations } from "$lib/agents.svelte";
import { ci, showCi } from "$lib/ci.svelte";
import { claudeView, showClaude } from "$lib/claude.svelte";
import { files, showFiles } from "$lib/files.svelte";
import { git, showGit } from "$lib/git.svelte";
import { projectWaits, showHome } from "$lib/home.svelte";
import { library, showLibrary } from "$lib/library.svelte";
import { showConfig } from "$lib/project.svelte";
import { schedule, showScheduler } from "$lib/schedule.svelte";
import { settings, showSettings } from "$lib/settings.svelte";
import { showSsh, ssh } from "$lib/ssh.svelte";
import { app, ui, type ViewId } from "$lib/state.svelte";
import { showTickets, tickets } from "$lib/tickets.svelte";

type Key = Parameters<typeof t>[0];

export type View = {
  id: ViewId;
  label: Key;
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
    command: "showCi",
    icon: Workflow,
    group: "project",
    show: showCi,
    loaded: () => ci.loaded,
    load: () => import("$lib/CiView.svelte"),
  },
  {
    id: "project",
    label: "ux3t3Config",
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
    command: "claudeViewShow",
    icon: Bot,
    group: "machine",
    show: () => showClaude(claudeView.section === "artifacts" ? "overview" : undefined),
    loaded: () => claudeView.loaded,
    load: () => import("$lib/ClaudeView.svelte"),
  },
  {
    id: "ssh",
    label: "viewSsh",
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
    command: "schedulerShow",
    icon: CalendarClock,
    group: "machine",
    show: showScheduler,
    loaded: () => schedule.loaded,
    load: () => import("$lib/SchedulerView.svelte"),
  },
];

