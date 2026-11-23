import { invoke } from "@tauri-apps/api/core";
import { t } from "$lib/i18n";
import { library, type Item, type Kind } from "$lib/library.svelte";
import { runScout, type ScoutState } from "$lib/scout.svelte";
import type { TaskEntry } from "$lib/schedule.svelte";
import { app } from "$lib/state.svelte";
import { todos, watches, type ConventionLike, type Todo } from "$lib/todo-logic";
import { toast } from "$lib/ui/toast.svelte";

const DISMISSED = "termf.library.todo.dismissed";

function dismissed(): string[] {
  try {
    return JSON.parse(localStorage.getItem(DISMISSED) ?? "[]");
  } catch {
    return [];
  }
}

export const todo = $state({
  /** null while it loads. */
  list: null as Todo[] | null,
  /** The scheduled watch of the current scope, if any. */
  watch: null as TaskEntry | null,
  /** What `claude --version` said, once per launch of TermF. */
  claude: undefined as string | null | undefined,
  error: "",
});

const projectOf = (id: string) => app.workspace?.projects.find((p) => p.id === id) ?? null;

let asked = 0;

/** Everything the to-do lines are made of, for the current project (the global library without one). */
export async function loadTodo() {
  const id = app.project ?? "";
  const project = projectOf(id);
  const ask = ++asked;
  todo.error = "";
  try {
    if (todo.claude === undefined) {
      todo.claude = await invoke<{ install: { version: string | null } | null }>("claude_binary").then(
        (b) => b.install?.version ?? null,
        () => null,
      );
    }
    const [scout, globalScout, conventions, tasks, items] = await Promise.all([
      invoke<ScoutState>("scout_state", { project: id || null }),
      id ? invoke<ScoutState>("scout_state", { project: null }) : Promise.resolve(null),
      id
        ? invoke<{ list: ConventionLike[] }>("conventions_list", { project: id }).then((c) => c.list)
        : Promise.resolve(null),
      invoke<TaskEntry[]>("schedule_tasks").catch(() => [] as TaskEntry[]),
      invoke<Item[]>("library_list", { project: id || null }),
    ]);
    if (ask !== asked) return;
    const repos = project?.repos.map((r) => r.path) ?? [];
    const all = (app.workspace?.projects ?? []).flatMap((p) => p.repos.map((r) => r.path));
    todo.watch = tasks.find((task) => watches(task, repos, all)) ?? null;
    todo.list = todos({
      project: id || null,
      scout,
      globalScout,
      conventions,
      scheduled: !!todo.watch,
      claude: todo.claude ?? null,
      items,
      dismissed: dismissed(),
    });
  } catch (e) {
    if (ask === asked) {
      todo.error = String(e);
      todo.list ??= [];
    }
  }
}

export function later(item: Todo) {
  try {
    localStorage.setItem(DISMISSED, JSON.stringify([...dismissed(), item.id]));
  } catch {
    // Without storage, the line simply comes back next time.
  }
  todo.list = todo.list?.filter((x) => x.id !== item.id) ?? null;
}

/** A watch of the current scope every Monday at 9, as the Scheduler would make it; one undo away. */
export async function scheduleWatch(budget: number | null): Promise<void> {
  const project = projectOf(library.scope);
  // shortcut: a project without repositories has no folder of its own: its watch would be the global one.
  const dir = project?.repos[0]?.path ?? app.workspace?.home ?? "";
  try {
    const task = {
      name: t("watchScheduleName", { scope: project?.name ?? t("watchScheduleGlobal") }),
      prompt: "",
      dir,
      cron: "0 9 * * 1",
      mode: "scout",
      model: null,
      effort: null,
      permissions: null,
      budget,
      machine: await invoke<string>("schedule_machine"),
      enabled: true,
      catchup: true,
    };
    const id = await invoke<string>("schedule_save", { id: null, task });
    toast(t("watchScheduled"), {
      undo: () =>
        void invoke("schedule_delete", { id })
          .catch((e) => toast(String(e), { kind: "error" }))
          .finally(loadTodo),
    });
  } catch (e) {
    toast(String(e), { kind: "error" });
  }
  await loadTodo();
}

/** Switches an item on for the current project (globally without one), one undo away. */
export async function switchOn(item: { kind: string; name: string }) {
  const project = projectOf(app.project ?? "");
  const set = (enabled: boolean | null) =>
    invoke("library_toggle", { kind: item.kind as Kind, name: item.name, project: project?.id ?? null, enabled });
  try {
    await set(true);
    toast(t("watchActivated", { name: item.name, project: project?.name ?? t("watchScheduleGlobal") }), {
      undo: () => void set(null).finally(() => library.version++),
    });
  } catch (e) {
    toast(String(e), { kind: "error" });
  } finally {
    library.version++;
  }
}

/** The watch's tab, at the scope of the line, its launch started when `run`. */
export function openScout(global = false, run = false) {
  library.global = global;
  library.panel = { kind: "scout" };
  if (run) void runScout([]);
}

