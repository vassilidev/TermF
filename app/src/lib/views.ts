import type { Component } from "svelte";
import Bot from "@lucide/svelte/icons/bot";
import CalendarClock from "@lucide/svelte/icons/calendar-clock";
import FolderTree from "@lucide/svelte/icons/folder-tree";
import KeyRound from "@lucide/svelte/icons/key-round";
import LayoutGrid from "@lucide/svelte/icons/layout-grid";
import LibraryBig from "@lucide/svelte/icons/library-big";
import MessagesSquare from "@lucide/svelte/icons/messages-square";
import SettingsIcon from "@lucide/svelte/icons/settings";
import Ticket from "@lucide/svelte/icons/ticket";
import Users from "@lucide/svelte/icons/users";
import Workflow from "@lucide/svelte/icons/workflow";
import type { t } from "$lib/i18n";
import { agents, showConversations, waitingAgents } from "$lib/agents.svelte";
import { ci, showCi } from "$lib/ci.svelte";
import { claudeView, showClaude } from "$lib/claude.svelte";
import { files, showFiles } from "$lib/files.svelte";
import { library, showLibrary } from "$lib/library.svelte";
import { schedule, showScheduler } from "$lib/schedule.svelte";
import { settings, showSettings } from "$lib/settings.svelte";
import { showSsh, ssh } from "$lib/ssh.svelte";
import { ui, waitingSessions, type ViewId } from "$lib/state.svelte";
import { showTeam, team } from "$lib/team.svelte";
import { showTickets, tickets } from "$lib/tickets.svelte";

type Key = Parameters<typeof t>[0];

export type View = {
  id: ViewId;
  label: Key;
  /** The palette's command that shows it. */
  command: Key;
  icon: Component;
  /** "foot": at the bottom of the navigation, out of the groups. */
  group: "work" | "project" | "machine" | "foot";
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
    id: "terminals",
    label: "viewTerminals",
    command: "showTerminals",
    icon: LayoutGrid,
    group: "work",
    show: showTerminals,
    loaded: () => true,
    load: () => import("$lib/SessionGrid.svelte"),
    count: () => waitingSessions().length || undefined,
    hot: true,
  },
  {
    id: "conversations",
    label: "chatView",
    command: "chatShow",
    icon: MessagesSquare,
    group: "work",
    show: showConversations,
    loaded: () => agents.loaded,
    load: () => import("$lib/ConversationsView.svelte"),
    count: () => waitingAgents().length || undefined,
    hot: true,
  },
  {
    id: "tickets",
    label: "viewTickets",
    command: "showTickets",
    icon: Ticket,
    group: "work",
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
    group: "work",
    show: showFiles,
    loaded: () => files.loaded,
    load: () => import("$lib/FilesView.svelte"),
  },
  {
    id: "team",
    label: "t5Team",
    command: "t5ShowTeam",
    icon: Users,
    group: "project",
    show: showTeam,
    loaded: () => team.loaded,
    load: () => import("$lib/TeamView.svelte"),
    count: () => team.roles.filter((r) => r.active).length || undefined,
  },
  {
    id: "library",
    label: "viewLibrary",
    command: "showLibrary",
    icon: LibraryBig,
    group: "project",
    show: showLibrary,
    loaded: () => library.loaded,
    load: () => import("$lib/LibraryView.svelte"),
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
    id: "scheduler",
    label: "schedulerView",
    command: "schedulerShow",
    icon: CalendarClock,
    group: "project",
    show: showScheduler,
    loaded: () => schedule.loaded,
    load: () => import("$lib/SchedulerView.svelte"),
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
    id: "settings",
    label: "settings",
    command: "settings",
    icon: SettingsIcon,
    group: "foot",
    show: () => showSettings(),
    loaded: () => settings.loaded,
    load: () => import("$lib/SettingsView.svelte"),
  },
];

export const groups = [
  ["work", "navWork"],
  ["project", "navProject"],
  ["machine", "navMachine"],
] as const satisfies [View["group"], Key][];

