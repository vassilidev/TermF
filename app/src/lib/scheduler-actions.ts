import { invoke } from "@tauri-apps/api/core";
import type { Action } from "$lib/actions.svelte";
import { t } from "$lib/i18n";
import { refresh, schedule, type Task } from "$lib/schedule.svelte";
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

export const schedulerActions: Action[] = [createTask];
