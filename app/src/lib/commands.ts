import { tick } from "svelte";
import ArrowDownUp from "@lucide/svelte/icons/arrow-down-up";
import ArrowLeftRight from "@lucide/svelte/icons/arrow-left-right";
import ArrowRight from "@lucide/svelte/icons/arrow-right";
import BellRing from "@lucide/svelte/icons/bell-ring";
import Gauge from "@lucide/svelte/icons/gauge";
import History from "@lucide/svelte/icons/history";
import CircleArrowUp from "@lucide/svelte/icons/circle-arrow-up";
import CircleHelp from "@lucide/svelte/icons/circle-help";
import Compass from "@lucide/svelte/icons/compass";
import Images from "@lucide/svelte/icons/images";
import Keyboard from "@lucide/svelte/icons/keyboard";
import Coffee from "@lucide/svelte/icons/coffee";
import Columns2 from "@lucide/svelte/icons/columns-2";
import Equal from "@lucide/svelte/icons/equal";
import FileDown from "@lucide/svelte/icons/file-down";
import FileSearch from "@lucide/svelte/icons/file-search";
import FileUp from "@lucide/svelte/icons/file-up";
import FolderSync from "@lucide/svelte/icons/folder-sync";
import Import from "@lucide/svelte/icons/import";
import KeyRound from "@lucide/svelte/icons/key-round";
import Layers from "@lucide/svelte/icons/layers";
import LayoutGrid from "@lucide/svelte/icons/layout-grid";
import ListChecks from "@lucide/svelte/icons/list-checks";
import Maximize2 from "@lucide/svelte/icons/maximize-2";
import Megaphone from "@lucide/svelte/icons/megaphone";
import BrainCircuit from "@lucide/svelte/icons/brain-circuit";
import MessageSquareText from "@lucide/svelte/icons/message-square-text";
import Play from "@lucide/svelte/icons/play";
import Plus from "@lucide/svelte/icons/plus";
import RefreshCw from "@lucide/svelte/icons/refresh-cw";
import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
import Rows2 from "@lucide/svelte/icons/rows-2";
import Sparkles from "@lucide/svelte/icons/sparkles";
import SquareTerminal from "@lucide/svelte/icons/square-terminal";
import StickyNote from "@lucide/svelte/icons/sticky-note";
import Telescope from "@lucide/svelte/icons/telescope";
import WandSparkles from "@lucide/svelte/icons/wand-sparkles";
import Ticket from "@lucide/svelte/icons/ticket";
import Workflow from "@lucide/svelte/icons/workflow";
import { agents, conversation, showConversation, showExpert, waitingAgents } from "$lib/agents.svelte";
import { showClaude } from "$lib/claude.svelte";
import { goToWait, home, projectWaits, talkCto } from "$lib/home.svelte";
import { toggleQuickTerminal } from "$lib/QuickTerminal.svelte";
import { fontReady } from "$lib/Terminal.svelte";
import { t } from "$lib/i18n";
import { allRepos, files, scopeRepos, showFiles } from "$lib/files.svelte";
import { display, keys, pressed } from "$lib/keymap.svelte";
import { library, showLibrary } from "$lib/library.svelte";
import { nextAfter } from "$lib/conversation-logic";
import { type Command } from "$lib/nav-logic";
import { newProject } from "$lib/newproject.svelte";
import { configSections, editProject, launchable, sessionProjects, shown } from "$lib/project.svelte";
import { openSetup, usable } from "$lib/onboarding.svelte";
import { leave } from "$lib/panes-logic";
import { closePanel, openBeside, otherPanel, toggleSplit } from "$lib/panes.svelte";
import type { Side } from "$lib/split-logic";
import * as tabs from "$lib/tabs.svelte";
import { generateKey, newKey } from "$lib/ssh.svelte";
import { openEntry, vault } from "$lib/vault.svelte";
import {
  app,
  grid,
  launch,
  load,
  onScreen,
  openSession,
  panes,
  resetContext,
  setKeepAwake,
  setProject,
  shortPath,
  ui,
  waitingSessions,
  type KeepAwakeMode,
  type ViewId,
} from "$lib/state.svelte";
import { showScout } from "$lib/scout.svelte";
import { checkUpdates } from "$lib/release.svelte";
import { exportConfig, importConfig, showSettings } from "$lib/settings.svelte";
import { showSync, summary, syncNow } from "$lib/sync.svelte";
import { launchTicket, tickets } from "$lib/tickets.svelte";
import { team, waitingRuns } from "$lib/team.svelte";
import { startTour } from "$lib/tour.svelte";
import { dispatchWorkflow } from "$lib/ci.svelte";
import { views } from "$lib/views";
import { visual } from "$lib/visual.svelte";
import { waiting as reviewsWaiting } from "$lib/visual-logic";

export { rank, type Command } from "$lib/nav-logic";
export { keys } from "$lib/keymap.svelte";

/** The session whose split holds the keyboard focus, else the current tab's active one. */
function activeSession(): number | null {
  const tile = document.activeElement?.closest<HTMLElement>("[data-session]");
  return tile ? Number(tile.dataset.session) : (tabs.currentTab()?.active ?? null);
}

/** Shows a session's split (switching the tab or the project if needed) and gives its terminal the keyboard. */
export async function focusSession(id: number) {
  const session = app.sessions.find((s) => s.id === id);
  if (session && !shown(session)) setProject(sessionProjects(session)[0] ?? null);
  ui.view = "terminals";
  tabs.showSession(id);
  await tick();
  await fontReady;
  document
    .querySelector<HTMLElement>(`[data-session="${id}"] .xterm-helper-textarea`)
    ?.focus({ preventScroll: true });
}

/**
 * Tiles first, then headless sessions, then team runs; from the conversation or team run shown, or the active tile.
 * On the Home, its own list instead, from the wait it last went to.
 */
export function nextWaiting() {
  const list = onScreen("home") ? projectWaits(app.project) : [];
  if (list.length) {
    const keys = list.map((w) => w.key);
    const next = nextAfter(keys, keys, home.shown);
    const wait = list.find((w) => w.key === next);
    if (wait) return void goToWait(wait);
  }
  const tile = (id: number) => `tile:${id}`;
  const agent = (id: number) => `agent:${id}`;
  const run = (id: number) => `team:${id}`;
  const order = [
    ...app.sessions.map((s) => tile(s.id)),
    ...agents.sessions.map((a) => agent(a.id)),
    ...team.runs.map((r) => run(r.run)),
  ];
  const waiting = [
    ...waitingSessions().map((s) => tile(s.id)),
    ...waitingAgents().map((a) => agent(a.id)),
    ...waitingRuns().map((r) => run(r.run)),
  ];
  const shown = onScreen("conversations") && !agents.creating ? (conversation()?.id ?? null) : null;
  const shownTeam = onScreen("run") ? tickets.selected : null;
  const active = activeSession();
  const from = shown !== null ? agent(shown) : shownTeam !== null ? run(shownTeam) : active !== null ? tile(active) : null;
  const next = nextAfter(order, waiting, from);
  if (!next) return;
  const id = Number(next.split(":")[1]);
  if (next.startsWith("agent:")) showConversation(id);
  else if (next.startsWith("team:")) showRun(id);
  else focusSession(id);
}

let runBack: ViewId = "tickets";

/** Opens a run's page, switching to its project if the current one is another; going back returns where it was opened from. */
export function showRun(id: number) {
  const project = tickets.runs.find((r) => r.id === id)?.project ?? team.runs.find((r) => r.run === id)?.target.project;
  if (project && app.project !== null && project !== app.project) setProject(project);
  if (ui.view !== "run") runBack = ui.view;
  ui.switcher = false;
  tickets.selected = id;
  ui.view = "run";
}

/** Back where the run was opened from, in the run's own panel; beside that view already, its panel closes. */
export function closeRun() {
  const back = usable(runBack);
  const how = leave({ ...panes, view: ui.view }, "run", back);
  if (how === "close") closePanel("run");
  else if (how === "back") ui.view = back;
}

let reviewBack: ViewId = "run";

/** Opens a visual review's page, in its project; going back returns where it was opened from. */
export function showReview(id: number) {
  const project = visual.reviews.find((r) => r.id === id)?.project;
  if (project && app.project !== null && project !== app.project) setProject(project);
  if (ui.view !== "review") reviewBack = ui.view;
  ui.switcher = false;
  visual.selected = id;
  ui.view = "review";
}

export function closeReview() {
  const back = usable(reviewBack === "run" && tickets.selected === null ? "home" : reviewBack);
  const how = leave({ ...panes, view: ui.view }, "review", back);
  if (how === "close") closePanel("review");
  else if (how === "back") ui.view = back;
}

export function toggleMaximize(id = activeSession()) {
  if (id === null) return;
  tabs.toggleZoom(id);
  focusSession(id);
}

/** Closes a split, asking first while its session runs, and hands the keyboard to the next one. */
export async function closeSplit(id = activeSession(), sure = false) {
  if (id === null) return;
  const next = await tabs.close(id, sure);
  if (next !== null) focusSession(next);
}

/**
 * In Terminals: the launcher of the header, or the one in place of the empty grid. `side`: what it opens goes beside
 * the active split instead of in a new tab.
 */
export async function openLauncher(side: Side | null = null) {
  const same = ui.launcher && launch.side === side;
  ui.view = "terminals";
  launch.side = side;
  await tick();
  const inline = document.querySelector<HTMLInputElement>(".empty .launcher input");
  if (inline) inline.focus();
  else if (app.sessions.some(shown)) ui.launcher = !same;
}

const goTo = (tab: tabs.Tab | undefined) => tab && focusSession(tabs.entryOf(tab));

function moveFocus(side: Side) {
  const id = activeSession();
  const next = id === null ? null : tabs.neighbourOf(id, side);
  if (next !== null) focusSession(next);
}

function resize(side: Side) {
  const id = activeSession();
  if (id !== null) tabs.resizeSplit(id, side);
}

/** App-wide shortcuts, in the capture phase so a focused terminal does not also get them. */
export function onShortcut(e: KeyboardEvent) {
  const shownTabs = tabs.visibleTabs();
  const projects = app.workspace?.projects ?? [];
  const asked = pressed(e, { tile: shownTabs.length, project: projects.length });
  if (!asked || (ui.palette && asked.id !== "palette" && asked.id !== "quick_terminal")) return;
  if (ui.help && asked.id !== "help") return;
  const n = asked.n ?? 0;
  const actions: Record<string, (() => void) | undefined> = {
    palette: () => (ui.palette = !ui.palette),
    // Held down, the key repeats: the help would open and close in a loop.
    help: () => e.repeat || (ui.help = !ui.help),
    split_view: toggleSplit,
    // With one panel, the keys stay with the terminal or the field.
    other_panel: panes.other ? otherPanel : undefined,
    quick_terminal: toggleQuickTerminal,
    next_waiting: nextWaiting,
    launch_ticket: launchTicket,
    switch_project: () => (ui.switcher = !ui.switcher),
    talk_cto: app.project === null ? undefined : talkCto,
    new_session: () => openLauncher(),
    maximize: () => toggleMaximize(),
    tile: () => goTo(shownTabs[n - 1]),
    project: () => setProject(projects[n - 1].id),
    prev_tab: () => goTo(tabs.stepTab(-1)),
    next_tab: () => goTo(tabs.stepTab(1)),
    split_right: () => openLauncher("right"),
    split_down: () => openLauncher("down"),
    close_split: () => closeSplit(),
    equalize_splits: () => tabs.equalize(),
    focus_left: () => moveFocus("left"),
    focus_right: () => moveFocus("right"),
    focus_up: () => moveFocus("up"),
    focus_down: () => moveFocus("down"),
    resize_left: () => resize("left"),
    resize_right: () => resize("right"),
    resize_up: () => resize("up"),
    resize_down: () => resize("down"),
  };
  const action = actions[asked.id];
  if (!action) return;
  e.preventDefault();
  e.stopPropagation();
  action();
}

const statusLabel = {
  starting: "statusStarting",
  working: "statusWorking",
  waiting: "statusWaiting",
  idle: "statusIdle",
  done: "statusDone",
} as const;

/** Every action the palette offers, by group, in the order shown before typing. */
export function commands(): Command[] {
  const list: Command[] = [];
  const agents = app.workspace?.agents ?? [];
  const projects = app.workspace?.projects ?? [];

  list.push({
    id: "project-all",
    label: t("ticketsAll"),
    icon: Layers,
    group: "projects",
    checked: app.project === null,
    run: () => setProject(null),
  });
  projects.forEach((project, i) => {
    const current = project.id === app.project;
    list.push({
      id: `project-${project.id}`,
      label: t("navSwitchTo", { name: project.name }),
      keys: i < 9 ? keys.project(i + 1) : undefined,
      icon: ArrowLeftRight,
      group: "projects",
      checked: current,
      run: () => setProject(project.id),
    });
    for (const agent of agents) {
      list.push({
        id: `project-open-${agent}-${project.id}`,
        label: t("navOpenInProject", { agent, project: project.name }),
        icon: Play,
        group: "projects",
        searchOnly: !current,
        run: () => {
          setProject(project.id);
          ui.view = "terminals";
          tabs.openFrom(
            project.repos.map((r) => r.path),
            agent,
            null,
          );
        },
      });
    }
    list.push({
      id: `conventions-${project.id}`,
      label: t("projectConventions", { project: project.name }),
      icon: Sparkles,
      group: "projects",
      searchOnly: true,
      run: () => {
        setProject(project.id);
        showLibrary();
        Object.assign(library, { global: false, tab: "conventions", panel: null });
      },
    });
  });
  list.push({ id: "project-new", label: t("newProject"), icon: Plus, group: "projects", run: newProject });
  if (app.project !== null) {
    const id = app.project;
    for (const section of configSections) {
      list.push({
        id: `config-${section.id}`,
        label: t(section.label),
        hint: t("ux3t3Config"),
        icon: section.icon,
        group: "projects",
        searchOnly: true,
        run: () => editProject(id, section.id),
      });
    }
    const name = app.workspace?.projects.find((p) => p.id === id)?.name ?? id;
    list.push({ id: "project-setup", label: t("obPalette", { project: name }), icon: WandSparkles, group: "projects", run: () => openSetup(id) });
    list.push({ id: "cto", label: t("ctoTalkTo", { project: name }), keys: keys.cto, icon: MessageSquareText, group: "projects", run: talkCto });
    list.push({ id: "expert", label: t("expertTalkTo", { project: name }), icon: BrainCircuit, group: "projects", run: showExpert });
  }
  for (const r of reviewsWaiting(visual.reviews, app.project)) {
    list.push({ id: `review-${r.id}`, label: t("review5bCommand", { title: r.title }), icon: Images, group: "projects", run: () => showReview(r.id) });
  }

  if (waitingSessions().length || waitingAgents().length || waitingRuns().length) {
    list.push({ id: "next", label: t("nextWaiting"), keys: keys.next, icon: BellRing, group: "sessions", run: nextWaiting });
  }
  const visible = app.sessions.filter(shown);
  const shownTabs = tabs.visibleTabs();
  for (const s of app.sessions) {
    const tab = tabs.tabOf(s.id);
    const i = tab ? shownTabs.indexOf(tab) : -1;
    list.push({
      id: `session-${s.id}`,
      label: s.title,
      hint: !s.running ? t("exitedLabel") : s.status ? t(statusLabel[s.status]) : undefined,
      // ⌘n goes to the tab's entry split: only that session shows it.
      keys: tab && i >= 0 && i < 9 && tabs.entryOf(tab) === s.id ? keys.tile(i + 1) : undefined,
      icon: ArrowRight,
      group: "sessions",
      run: () => focusSession(s.id),
    });
  }
  if (visible.length) {
    list.push({
      id: "maximize",
      label: t("maximizeActive"),
      keys: keys.maximize,
      icon: Maximize2,
      group: "sessions",
      run: () => toggleMaximize(),
    });
    for (const [side, label, icon] of [
      ["right", "t6aSplitRight", Columns2],
      ["down", "t6aSplitDown", Rows2],
    ] as const) {
      list.push({
        id: `split-${side}`,
        label: t(label),
        keys: display(`split_${side}`),
        icon,
        group: "sessions",
        run: () => openLauncher(side),
      });
    }
    list.push({
      id: "equalize",
      label: t("t6aEqualize"),
      keys: display("equalize_splits"),
      icon: Equal,
      group: "sessions",
      run: () => tabs.equalize(),
    });
    list.push({
      id: "broadcast",
      label: t("broadcast"),
      icon: Megaphone,
      group: "sessions",
      checked: grid.broadcast,
      run: () => ((ui.view = "terminals"), (grid.broadcast = !grid.broadcast)),
    });
  }
  for (const s of app.sessions) {
    if (s.running && s.context?.phase === "idle") {
      list.push({
        id: `reset-${s.id}`,
        label: t("contextResetSession", { session: s.title }),
        icon: RotateCcw,
        group: "sessions",
        run: () => resetContext(s.id),
      });
    }
  }

  const ticked = launchable();
  if (ticked.length) {
    for (const agent of agents) {
      list.push({
        id: `open-${agent}`,
        label: t("open", { agent, count: ticked.length }),
        icon: Play,
        group: "repos",
        run: () => tabs.openFrom(ticked, agent, null),
      });
    }
    list.push({
      id: "open-shell",
      label: t("openShell", { count: ticked.length }),
      icon: SquareTerminal,
      group: "repos",
      run: () => tabs.openFrom(ticked, null, null),
    });
  }
  const own = new Set(scopeRepos().map((r) => r.path));
  const repos = [...allRepos()].sort((a, b) => (b.modified ?? 0) - (a.modified ?? 0));
  for (const repo of repos) {
    const searchOnly = !own.has(repo.path);
    const group = searchOnly ? "others" : "repos";
    for (const agent of agents) {
      list.push({
        id: `repo-${agent}-${repo.path}`,
        label: t("openIn", { agent, repo: repo.name }),
        hint: shortPath(repo.path),
        icon: Play,
        group,
        searchOnly,
        run: () => openSession(repo.path, agent),
      });
    }
    list.push({
      id: `repo-shell-${repo.path}`,
      label: t("openShellIn", { repo: repo.name }),
      hint: shortPath(repo.path),
      icon: SquareTerminal,
      group,
      searchOnly,
      run: () => openSession(repo.path, null),
    });
  }

  for (const view of views) {
    const group = view.group === "machine" ? "machine" : "views";
    // "Edit the project" names no project under every project.
    if (view.id !== ui.view && (view.id !== "project" || app.project !== null)) {
      list.push({ id: `view-${view.id}`, label: t(view.command), icon: view.icon, group, run: view.show });
    }
    if (!onScreen(view.id)) {
      list.push({
        id: `beside-${view.id}`,
        label: t("t6bOpenBesideView", { view: t(view.label) }),
        icon: Columns2,
        group,
        searchOnly: true,
        run: () => openBeside(view.id),
      });
    }
  }
  // Titles only, found by typing: never a value, the store has none.
  for (const item of vault.status?.unlocked ? vault.items : []) {
    list.push({
      id: `vault-${item.id}`,
      label: item.title,
      hint: item.username || undefined,
      icon: item.kind === "login" ? KeyRound : StickyNote,
      group: "vault",
      searchOnly: true,
      run: () => openEntry(item.id, item.project),
    });
  }
  list.push(
    { id: "claude-artifacts", label: t("claudeViewShowArtifacts"), icon: Images, group: "machine", run: () => showClaude("artifacts") },
    { id: "claude-sessions", label: t("claudeDgShowSessions"), icon: History, group: "machine", run: () => showClaude("sessions") },
    { id: "claude-usage", label: t("claudeDgShowUsage"), icon: Gauge, group: "machine", run: () => showClaude("usage") },
  );

  const action = (command: Omit<Command, "group">) => list.push({ ...command, group: "actions" });
  action({ id: "switcher", label: t("navSwitchProject"), keys: keys.switcher, icon: ArrowLeftRight, run: () => (ui.switcher = true) });
  action({ id: "launcher", label: t("navNewSession"), keys: keys.launcher, icon: Plus, run: openLauncher });
  action({
    id: "split-view",
    label: t("t6bSplitView"),
    keys: display("split_view"),
    icon: Columns2,
    checked: panes.other !== null,
    run: toggleSplit,
  });
  if (panes.other) {
    action({ id: "other-panel", label: t("t6bOtherPanel"), keys: display("other_panel"), icon: ArrowLeftRight, run: otherPanel });
  }
  action({ id: "help", label: t("t6bHelp"), keys: keys.help, icon: CircleHelp, run: () => (ui.help = true) });
  action({ id: "tour", label: t("t6bTour"), icon: Compass, run: startTour });
  action({ id: "shortcuts", label: t("shortcuts"), icon: Keyboard, run: () => showSettings("keys") });
  action({ id: "release-check", label: t("releaseCmdCheck"), icon: CircleArrowUp, run: () => (showSettings("updates"), checkUpdates()) });
  action({ id: "release-needs", label: t("releaseCmdNeeds"), icon: ListChecks, run: () => showSettings("tools") });
  action({ id: "quick", label: t("quickTerminal"), keys: keys.quick, icon: SquareTerminal, run: toggleQuickTerminal });
  action({ id: "ticket-launch", label: t("ticketLaunchLong"), keys: keys.ticket, icon: Ticket, run: launchTicket });
  action({ id: "ci-dispatch", label: t("ciDispatchLong"), icon: Workflow, run: dispatchWorkflow });
  action({
    id: "file",
    label: t("filesFind"),
    keys: keys.file,
    icon: FileSearch,
    run: () => {
      showFiles();
      if (files.repo) files.finder = true;
    },
  });
  for (const layout of app.layouts) {
    action({
      id: `layout-${layout.name}`,
      label: t("openLayout", { name: layout.name }),
      icon: LayoutGrid,
      run: () => tabs.openLayout(layout),
    });
  }
  const awake = app.keepAwake;
  if (awake) {
    const modes: [KeepAwakeMode, "keepAwakeAuto" | "keepAwakeAlways" | "keepAwakeOff"][] = [
      ["auto", "keepAwakeAuto"],
      ["always", "keepAwakeAlways"],
      ["off", "keepAwakeOff"],
    ];
    for (const [mode, label] of modes) {
      action({
        id: `awake-${mode}`,
        label: t("keepAwakeSet", { mode: t(label) }),
        icon: Coffee,
        checked: awake.mode === mode,
        run: () => setKeepAwake(mode, awake.displayOn).then((error) => error && (app.error = error)),
      });
    }
  }
  if (app.sync) {
    action({ id: "sync", label: t("syncTitle"), hint: summary(app.sync).text, icon: FolderSync, run: showSync });
    if (app.sync.enabled && app.sync.unlocked) {
      action({ id: "sync-now", label: t("syncNow"), icon: ArrowDownUp, run: () => (showSync(), syncNow()) });
    }
  }
  action({
    id: "claude-import",
    label: t("claudeImport"),
    icon: Import,
    run: () => {
      showLibrary();
      library.panel = { kind: "claude" };
    },
  });
  action({ id: "library-scout", label: t("scoutLaunchLong"), icon: Telescope, run: showScout });
  action({ id: "config-export", label: t("configExportLong"), icon: FileDown, run: exportConfig });
  action({ id: "config-import", label: t("configImportLong"), icon: FileUp, run: importConfig });
  action({ id: "ssh-generate", label: t("sshGenerateLong"), icon: KeyRound, run: generateKey });
  action({ id: "keys-generate", label: t("keysGenerateLong"), icon: KeyRound, run: newKey });
  action({ id: "reload", label: t("reload"), icon: RefreshCw, run: load });
  return list;
}
