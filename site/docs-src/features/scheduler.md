---
title: Scheduler and loops
order: 12
---

The scheduler runs Claude tasks on a cron, even while TermF is closed, and shows the loops your Claude sessions are running.

## The scheduler {#scheduler}

### Add a task

1. Open **Scheduler** and write the task: prompt, cron, model, effort, permissions, budget. It runs headless or in a terminal.
2. TermF installs it with the system's scheduler: launchd on macOS, systemd on Linux, the Task Scheduler on Windows.
3. At its time, `termf schedule run` starts it. One missed slot is caught up, never two at once.

The cron has five fields, in local time, daylight saving time included. A task runs only on the machine that created it.

```sh
# the tasks
termf schedule list | add | edit | rm

# run one now
termf schedule run <id>

# past runs
termf schedule history
```

> [!NOTE]
> A task runs at most two hours.
>
> **Tried so far.** Tried with the real launchd (a throwaway task run at the next minute) and a real headless run on Haiku; the form is driven in the real app on macOS by an end-to-end test.

## The loops of your Claude sessions {#session-loops}

Claude Code's `/loop` and the tasks a session schedules live only as long as that session. TermF shows them where you can see them.

1. A tile running a `/loop` or a scheduled task shows a badge such as *↻ 5 min*. Click it for each loop's prompt, interval and next run.
2. **Make it a Scheduler task** fills the Scheduler's form with the same prompt, folder and schedule, so it keeps running when the session closes, even with the app closed.
3. The Scheduler lists every loop of the open sessions in one place.

Loops are read from the hooks TermF already receives: nothing to install, nothing sent anywhere. The CTO and the team do not use loops, since TermF drives their runs; a loop an agent creates anyway is shown on the run's page.

> [!NOTE]
> A loop can be seen, not created or stopped from outside its session: that is how Claude Code works. The next run of a fixed-interval loop shows only when Claude Code gives it.
>
> **Tried so far.** A real Claude Code session ran `/loop 1m`: TermF saw the loop, then its end.
