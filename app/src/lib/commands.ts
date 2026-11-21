import { tick } from "svelte";
import ArrowDownUp from "@lucide/svelte/icons/arrow-down-up";
import ArrowLeftRight from "@lucide/svelte/icons/arrow-left-right";
import ArrowRight from "@lucide/svelte/icons/arrow-right";
import BellRing from "@lucide/svelte/icons/bell-ring";
import Images from "@lucide/svelte/icons/images";
import Keyboard from "@lucide/svelte/icons/keyboard";
import Coffee from "@lucide/svelte/icons/coffee";
import FileDown from "@lucide/svelte/icons/file-down";
import FileSearch from "@lucide/svelte/icons/file-search";
import FileUp from "@lucide/svelte/icons/file-up";
import FolderSync from "@lucide/svelte/icons/folder-sync";
import Import from "@lucide/svelte/icons/import";
import KeyRound from "@lucide/svelte/icons/key-round";
import Layers from "@lucide/svelte/icons/layers";
import LayoutGrid from "@lucide/svelte/icons/layout-grid";
import Maximize2 from "@lucide/svelte/icons/maximize-2";
import Play from "@lucide/svelte/icons/play";
import Plus from "@lucide/svelte/icons/plus";
import RefreshCw from "@lucide/svelte/icons/refresh-cw";
import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
import Sparkles from "@lucide/svelte/icons/sparkles";
import SquareTerminal from "@lucide/svelte/icons/square-terminal";
import Telescope from "@lucide/svelte/icons/telescope";
import Ticket from "@lucide/svelte/icons/ticket";
import Workflow from "@lucide/svelte/icons/workflow";
import { showClaude } from "$lib/claude.svelte";
import { toggleQuickTerminal } from "$lib/QuickTerminal.svelte";
import { fontReady } from "$lib/Terminal.svelte";
import { t } from "$lib/i18n";
import { allRepos, files, scopeRepos, showFiles } from "$lib/files.svelte";
import { keys, pressed } from "$lib/keymap.svelte";
import { library, showLibrary } from "$lib/library.svelte";
import { type Command } from "$lib/nav-logic";
import { launchable, sessionProjects, shown } from "$lib/project.svelte";
import { generateKey, newKey } from "$lib/ssh.svelte";
import {
  app,
  load,
  openLayout,
  openRepos,
  openSession,
  resetContext,
  setKeepAwake,
  setProject,
  shortPath,
  ui,
  waitingSessions,
  type KeepAwakeMode,
} from "$lib/state.svelte";
import { showScout } from "$lib/scout.svelte";
import { exportConfig, importConfig, showSettings } from "$lib/settings.svelte";
import { showSync, summary, syncNow } from "$lib/sync.svelte";
import { launchTicket } from "$lib/tickets.svelte";
import { dispatchWorkflow } from "$lib/ci.svelte";
import { views } from "$lib/views";

export { rank, type Command } from "$lib/nav-logic";
export { keys } from "$lib/keymap.svelte";

/** The session whose tile holds the keyboard focus, else the maximized one. */
function activeSession(): number | null {
  const tile = document.activeElement?.closest<HTMLElement>("[data-session]");
  return tile ? Number(tile.dataset.session) : app.focused;
}

/** Shows a session's tile (switching the maximized one, or the project, if needed) and gives its terminal the keyboard. */
export async function focusSession(id: number) {
  const session = app.sessions.find((s) => s.id === id);
  if (session && !shown(session)) setProject(sessionProjects(session)[0] ?? null);
  ui.view = "terminals";
  if (app.focused !== null) app.focused = id;
  await tick();
  await fontReady;
  document
    .querySelector<HTMLElement>(`[data-session="${id}"] .xterm-helper-textarea`)
    ?.focus({ preventScroll: true });
}

export function nextWaiting() {
  const waiting = waitingSessions();
  if (!waiting.length) return;
  const from = app.sessions.findIndex((s) => s.id === activeSession());
  const next = waiting.find((s) => app.sessions.indexOf(s) > from) ?? waiting[0];
  focusSession(next.id);
}

export function toggleMaximize(id = activeSession()) {
  if (id === null) return;
  app.focused = app.focused === id ? null : id;
  focusSession(id);
}

/** In Terminals: the launcher of the header, or the one in place of the empty grid. */
export async function openLauncher() {
  ui.view = "terminals";
  await tick();
  const inline = document.querySelector<HTMLInputElement>(".empty .launcher input");
  if (inline) inline.focus();
  else if (app.sessions.some(shown)) ui.launcher = !ui.launcher;
}

/** App-wide shortcuts, in the capture phase so a focused terminal does not also get them. */
export function onShortcut(e: KeyboardEvent) {
  const tiles = app.sessions.filter(shown);
  const projects = app.workspace?.projects ?? [];
  const asked = pressed(e, { tile: tiles.length, project: projects.length });
  if (!asked || (ui.palette && asked.id !== "palette" && asked.id !== "quick_terminal")) return;
  const n = asked.n ?? 0;
  const actions: Record<string, () => void> = {
    palette: () => (ui.palette = !ui.palette),
    quick_terminal: toggleQuickTerminal,
    next_waiting: nextWaiting,
    launch_ticket: launchTicket,
    switch_project: () => (ui.switcher = !ui.switcher),
    new_session: openLauncher,
    maximize: () => toggleMaximize(),
    tile: () => focusSession(tiles[n - 1].id),
    project: () => setProject(projects[n - 1].id),
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
          openRepos(
            project.repos.map((r) => r.path),
            agent,
          );
        },
      });
    }
    list.push({
      id: `conventions-${project.id}`,
      label: t("projectConventions", { project: project.name }),
      icon: Sparkles,
      group: "projects",
      run: () => {
        setProject(project.id);
        showLibrary();
        Object.assign(library, { global: false, tab: "conventions", panel: null });
      },
    });
  });

  if (waitingSessions().length) {
    list.push({ id: "next", label: t("nextWaiting"), keys: keys.next, icon: BellRing, group: "sessions", run: nextWaiting });
  }
  const visible = app.sessions.filter(shown);
  for (const s of app.sessions) {
    const i = visible.indexOf(s);
    list.push({
      id: `session-${s.id}`,
      label: s.title,
      hint: !s.running ? t("exitedLabel") : s.status ? t(statusLabel[s.status]) : undefined,
      keys: i >= 0 && i < 9 ? keys.tile(i + 1) : undefined,
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
        run: () => openRepos(ticked, agent),
      });
    }
    list.push({
      id: "open-shell",
      label: t("openShell", { count: ticked.length }),
      icon: SquareTerminal,
      group: "repos",
      run: () => openRepos(ticked, null),
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
    if (view.id !== ui.view) {
      list.push({ id: `view-${view.id}`, label: t(view.command), icon: view.icon, group: "views", run: view.show });
    }
  }
  list.push({
    id: "claude-artifacts",
    label: t("claudeViewShowArtifacts"),
    icon: Images,
    group: "views",
    run: () => showClaude("artifacts"),
  });

  const action = (command: Omit<Command, "group">) => list.push({ ...command, group: "actions" });
  action({ id: "switcher", label: t("navSwitchProject"), keys: keys.switcher, icon: ArrowLeftRight, run: () => (ui.switcher = true) });
  action({ id: "launcher", label: t("navNewSession"), keys: keys.launcher, icon: Plus, run: openLauncher });
  action({ id: "shortcuts", label: t("shortcuts"), icon: Keyboard, run: () => showSettings("keys") });
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
      run: () => openLayout(layout),
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
