import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { ui } from "$lib/state.svelte";
import { lang, t } from "$lib/i18n";

export type Mode = "headless" | "tile" | "scout";
export type Task = {
  name: string;
  prompt: string;
  /** Absolute. */
  dir: string;
  /** Five cron fields, local time. */
  cron: string;
  mode: Mode;
  model: string | null;
  effort: string | null;
  permissions: string | null;
  /** US dollars, headless only. */
  budget: number | null;
  /** The machine it runs on; null: every machine where TermF is open. */
  machine: string | null;
  enabled: boolean;
  catchup: boolean;
};
export type ExecutionStatus = "running" | "succeeded" | "failed" | "opened";
export type Execution = {
  id: number;
  task: string;
  name: string;
  trigger: "schedule" | "catchup" | "manual";
  slot: number | null;
  mode: Mode;
  status: ExecutionStatus;
  /** Seconds since the Unix epoch. */
  started: number;
  finished: number | null;
  cost: number | null;
  output: string | null;
  error: string | null;
};
export type TaskEntry = Task & {
  id: string;
  /** The system's scheduler also runs it while TermF is closed. */
  installed: boolean;
  next: number | null;
  last: Execution | null;
};
export type DesktopTask = { name: string; description: string | null; prompt: string; path: string };

export const schedule = $state({
  /** Loaded once first shown, then kept mounted. */
  loaded: false,
  tab: "tasks" as "tasks" | "others",
  /** null while the first load runs. */
  tasks: null as TaskEntry[] | null,
  loadError: "",
  selected: null as string | null,
  /** The form: the id of the task edited, "" for a new one, null when closed. */
  editing: null as string | null,
  /** What a new task's form starts with, e.g. a session's loop; each one counted, so an open form takes it. */
  draft: null as Partial<Task> | null,
  drafts: 0,
  /** The selected task's runs, newest first. */
  history: [] as Execution[],
  desktop: null as DesktopTask[] | null,
  /** This machine's name. */
  machine: "",
  error: "",
});

/** Whether a task runs on this machine. */
export const runsHere = (task: Task) => !task.machine || task.machine === schedule.machine;

export function showScheduler() {
  ui.view = "scheduler";
  schedule.loaded = true;
  refresh();
}

/** Opens the form of a new task that does what a session's loop does. */
export function scheduleLoop(draft: Pick<Task, "prompt" | "dir" | "cron">) {
  showScheduler();
  schedule.tab = "tasks";
  schedule.draft = draft;
  schedule.drafts++;
  schedule.editing = "";
}

export async function refresh() {
  schedule.loadError = "";
  try {
    if (!schedule.machine) schedule.machine = await invoke<string>("schedule_machine");
    schedule.tasks = await invoke<TaskEntry[]>("schedule_tasks");
  } catch (e) {
    schedule.loadError = String(e);
  }
  await loadHistory();
}

export async function loadHistory() {
  const task = schedule.selected ?? schedule.tasks?.[0]?.id;
  if (!task) return void (schedule.history = []);
  try {
    const runs = await invoke<Execution[]>("schedule_history", { task, limit: 50 });
    if (task === (schedule.selected ?? schedule.tasks?.[0]?.id)) schedule.history = runs;
  } catch (e) {
    schedule.error = String(e);
  }
}

/** Takes a run as `schedule-run` gives it; its task's next run may have changed. */
function upsert(run: Execution) {
  const entry = schedule.tasks?.find((e) => e.id === run.task);
  if (entry && (!entry.last || entry.last.id <= run.id)) entry.last = run;
  if (entry?.enabled && runsHere(entry)) {
    invoke<number[]>("schedule_preview", { cron: entry.cron, count: 1 })
      .then(([next]) => (entry.next = next ?? null))
      .catch(() => {});
  }
  if (run.task !== (schedule.selected ?? schedule.tasks?.[0]?.id)) return;
  const i = schedule.history.findIndex((r) => r.id === run.id);
  if (i >= 0) schedule.history[i] = run;
  else schedule.history.unshift(run);
}

const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD", maximumFractionDigits: 2 });
export const cost = (usd: number) => money.format(usd);

/** Keeps the view current and notifies when a headless run ends, whatever view is shown. */
export function watchSchedule(): () => void {
  const stop = listen<Execution>("schedule-run", async ({ payload: run }) => {
    upsert(run);
    if (run.status !== "succeeded" && run.status !== "failed") return;
    const body =
      run.status === "failed"
        ? t("schedulerNotifyFailed", { error: (run.error ?? "").split("\n")[0] })
        : run.cost !== null
          ? t("schedulerNotifyOkCost", { cost: cost(run.cost) })
          : t("schedulerNotifyOk");
    const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
    if (granted) sendNotification({ title: run.name, body });
  });
  return () => void stop.then((f) => f());
}

const timeFormat = new Intl.DateTimeFormat(lang, { hour: "2-digit", minute: "2-digit" });
const dayFormat = new Intl.DateTimeFormat(lang, { weekday: "long" });
const list = new Intl.ListFormat(lang, { style: "long", type: "conjunction" });

/** "lun. 24 nov., 09:00": a run's date. */
export const when = (epochSeconds: number) =>
  new Date(epochSeconds * 1000).toLocaleString(lang, {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });

const names = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/** The numbers a cron field lists (`1,3`, `1-5`, names), or null when it uses steps or stars. */
function values(field: string, max: number): number[] | null {
  const out: number[] = [];
  for (const part of field.split(",")) {
    const [a, b] = part.split("-").map((v) => (names.includes(v.toLowerCase()) ? names.indexOf(v.toLowerCase()) : Number(v)));
    if (!/^[\w-]+$/.test(part) || !Number.isInteger(a) || (b !== undefined && !Number.isInteger(b))) return null;
    for (let v = a; v <= (b ?? a); v++) if (v <= max) out.push(v);
  }
  return out;
}

/** The common schedules in words; null for the others, shown by their next runs only. */
export function describe(cron: string): string | null {
  const f = cron.trim().split(/\s+/);
  if (f.length !== 5) return null;
  const [minute, hour, day, month, weekday] = f;
  if (day === "*" && month === "*" && weekday === "*" && hour === "*") {
    if (minute === "*") return t("schedulerEveryMinute");
    const step = /^\*\/(\d+)$/.exec(minute);
    if (step) return t("schedulerEveryMinutes", { n: step[1] });
    if (/^\d+$/.test(minute)) return t("schedulerHourly", { minute });
    return null;
  }
  const minutes = values(minute, 59);
  const hours = values(hour, 23);
  if (month !== "*" || !minutes || !hours || minutes.length !== 1) return null;
  const times = list.format(hours.map((h) => timeFormat.format(new Date(2026, 0, 1, h, minutes[0]))));
  if (day === "*" && weekday === "*") return t("schedulerDaily", { times });
  if (day === "*") {
    const days = values(weekday, 7)?.map((d) => d % 7);
    if (!days) return null;
    if (days.join() === "1,2,3,4,5") return t("schedulerWeekdays", { times });
    // 4 January 2026 is a Sunday.
    const dayNames = [...new Set(days)].map((d) => dayFormat.format(new Date(2026, 0, 4 + d)));
    return t("schedulerOnDays", { days: list.format(dayNames), times });
  }
  if (weekday === "*" && /^\d+$/.test(day)) return t("schedulerMonthly", { day, times });
  return null;
}
