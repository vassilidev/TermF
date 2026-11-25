import { invoke } from "@tauri-apps/api/core";
import type { Action } from "$lib/actions.svelte";
import { t } from "$lib/i18n";
import { refresh, schedule, type Task, type TaskEntry } from "$lib/schedule.svelte";
import { app } from "$lib/state.svelte";
import { toast } from "$lib/ui/toast.svelte";

const text = (v: unknown) => (typeof v === "string" && v.trim() ? v.trim() : null);

/** The folder a task runs in: a repository of the project by its name or path, else the project's first. */
function dirOf(project: string, repo: unknown): string {
  const repos = app.workspace?.projects.find((p) => p.id === project)?.repos ?? [];
  const found = text(repo) ? repos.find((r) => r.name === repo || r.path === repo) : repos[0];
  if (!found) throw new Error(text(repo) ? `No repository ${String(repo)} in this project.` : "This project has no repository.");
  return found.path;
}

const createTask: Action = {
  id: "scheduler.create_task",
  description:
    "Adds a task to TermF's scheduler, as its form does: `prompt` run by Claude Code on a cron schedule (local time), headless (`headless`, the default) or in a terminal (`tile`), or the library watch (`scout`), in a repository of the project.",
  fields: {
    name: { type: "string", description: "The task's name." },
    cron: { type: "string", description: "Five cron fields, local time: 0 9 * * 1 is Mondays at 9." },
    prompt: { type: "string", description: "What Claude Code does each time (not for scout)." },
    mode: { type: "string", description: "headless (default), tile or scout.", enum: ["headless", "tile", "scout"] },
    repo: { type: "string", description: "The repository's name in the project, its first by default." },
    model: { type: "string", description: "The model, Claude Code's default otherwise." },
    budget: { type: "integer", description: "The most one headless run may spend, in US dollars." },
  },
  required: ["name", "cron"],
  label: "actSchedulerCreate",
  describe: (a) =>
    `${t("actSchedulerCreateText", { name: String(a.name), cron: String(a.cron), mode: String(a.mode ?? "headless") })}${text(a.prompt) ? `\n${text(a.prompt)}` : ""}`,
  async run(a, _secrets, project) {
    const mode = (text(a.mode) ?? "headless") as Task["mode"];
    const task: Task = {
      name: String(a.name).trim(),
      prompt: mode === "scout" ? "" : (text(a.prompt) ?? ""),
      dir: dirOf(project, a.repo),
      cron: String(a.cron).trim(),
      mode,
      model: text(a.model),
      effort: null,
      permissions: null,
      budget: mode !== "tile" && typeof a.budget === "number" ? a.budget : null,
      machine: schedule.machine || null,
      enabled: true,
      catchup: true,
    };
    const id = await invoke<string>("schedule_save", { id: null, task });
    await refresh();
    schedule.selected = id;
    toast(t("actSchedulerCreated", { name: task.name }));
    return { task: id, name: task.name, note: "The user sees and edits it in the Scheduler." };
  },
};

async function taskOf(task: unknown): Promise<TaskEntry> {
  if (!schedule.tasks) await refresh();
  const found = schedule.tasks?.find((x) => x.id === task) ?? schedule.tasks?.find((x) => x.name === task);
  if (!found) throw new Error(`No task ${String(task)}: ${schedule.tasks?.map((x) => x.name).join(", ") || "none yet"}.`);
  return found;
}

const nameOf = (task: unknown) => schedule.tasks?.find((x) => x.id === task || x.name === task)?.name ?? String(task);
const taskField = { type: "string" as const, description: "The task's id or name, as `here` lists it." };

const editTask: Action = {
  id: "scheduler.edit_task",
  description:
    "Changes a task of the scheduler, as its form and switches do: the fields given (as scheduler.create_task names them) replace its own; `enabled` false pauses it, true resumes it; `installed` has the system's scheduler also run it while TermF is closed.",
  fields: {
    task: taskField,
    ...createTask.fields,
    enabled: { type: "boolean", description: "false pauses it, true resumes it." },
    installed: { type: "boolean", description: "Also run by the system's scheduler while TermF is closed." },
  },
  required: ["task"],
  label: "actSchedulerEdit",
  describe: (a) =>
    `${t("actSchedulerEditText", { name: nameOf(a.task) })}\n${Object.entries(a)
      .filter(([k, v]) => k !== "task" && v !== null && v !== undefined)
      .map(([k, v]) => `${k} → ${String(v)}`)
      .join("\n")}`,
  async run(a, _secrets, project) {
    const found = await taskOf(a.task);
    const { id, installed, next: _n, last: _l, ...was } = found;
    const mode = (text(a.mode) ?? was.mode) as Task["mode"];
    const task: Task = {
      ...was,
      name: text(a.name) ?? was.name,
      cron: text(a.cron) ?? was.cron,
      prompt: mode === "scout" ? "" : typeof a.prompt === "string" ? a.prompt.trim() : was.prompt,
      mode,
      dir: text(a.repo) ? dirOf(project, a.repo) : was.dir,
      model: typeof a.model === "string" ? text(a.model) : was.model,
      budget: mode === "tile" ? null : typeof a.budget === "number" ? a.budget : was.budget,
      enabled: typeof a.enabled === "boolean" ? a.enabled : was.enabled,
    };
    await invoke("schedule_save", { id, task });
    if (typeof a.installed === "boolean" && a.installed !== installed) await invoke<boolean>("schedule_install", { id, on: a.installed });
    await refresh();
    schedule.selected = id;
    toast(t("actSchedulerEdited", { name: task.name }));
    return { task: id, name: task.name, enabled: task.enabled };
  },
};

const deleteTask: Action = {
  id: "scheduler.delete_task",
  description: "Deletes a task of the scheduler, as its view does.",
  fields: { task: taskField },
  required: ["task"],
  label: "actSchedulerDelete",
  describe: (a) => t("actSchedulerDeleteText", { name: nameOf(a.task) }),
  loses: () => t("actSchedulerDeleteLoses"),
  async run(a) {
    const found = await taskOf(a.task);
    await invoke("schedule_delete", { id: found.id });
    if (schedule.selected === found.id) schedule.selected = null;
    await refresh();
    toast(t("actSchedulerDeleted", { name: found.name }));
    return { deleted: found.name };
  },
};

const runNow: Action = {
  id: "scheduler.run_now",
  description: "Runs a task of the scheduler now, out of its schedule, as its Run now button does.",
  fields: { task: taskField },
  required: ["task"],
  label: "actSchedulerRun",
  describe: (a) => t("actSchedulerRunText", { name: nameOf(a.task) }),
  async run(a) {
    const found = await taskOf(a.task);
    await invoke("schedule_run", { id: found.id });
    await refresh();
    schedule.selected = found.id;
    toast(t("actSchedulerRan", { name: found.name }));
    return { started: found.name, note: "Its run shows in the Scheduler's history." };
  },
};

/** The scheduler's tasks, by id and name. */
export async function schedulerHere() {
  if (!schedule.tasks) await refresh();
  return { tasks: (schedule.tasks ?? []).map((x) => ({ id: x.id, name: x.name, cron: x.cron, mode: x.mode, enabled: x.enabled, installed: x.installed })) };
}

export const schedulerActions: Action[] = [createTask, editTask, deleteTask, runNow];
