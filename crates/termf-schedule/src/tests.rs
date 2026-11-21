use std::fs;
use std::path::PathBuf;

use super::*;

/// A throwaway config folder, data folder and working folder.
struct Env {
    root: PathBuf,
    scheduler: Scheduler,
    dir: PathBuf,
}

impl Env {
    fn new(name: &str) -> Env {
        let root =
            std::env::temp_dir().join(format!("termf-schedule-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = root.join("repo");
        fs::create_dir_all(&dir).unwrap();
        let mut scheduler = Scheduler::new(root.join("config"), root.join("data"));
        // Never the real launchd, systemd or Task Scheduler.
        scheduler.system = None;
        Env {
            root,
            scheduler,
            dir,
        }
    }

    fn task(&self, name: &str, cron: &str) -> Task {
        Task {
            name: name.into(),
            prompt: "Summarize yesterday's commits.\nKeep it short.".into(),
            dir: self.dir.to_string_lossy().into_owned(),
            cron: cron.into(),
            mode: Mode::Headless,
            model: Some("sonnet".into()),
            effort: Some("high".into()),
            permissions: Some("dontAsk".into()),
            budget: Some(2.5),
            enabled: true,
            catchup: true,
        }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn saves_edits_and_deletes_tasks() {
    let env = Env::new("save");
    let s = &env.scheduler;
    let task = env.task("  Daily review ", "0  9 * * 1-5");
    let id = s.save(None, task.clone(), None).unwrap();
    assert_eq!(id, "daily-review");
    assert_eq!(s.save(None, task.clone(), None).unwrap(), "daily-review-2");

    let saved = s.task(&id).unwrap();
    assert_eq!(saved.name, "Daily review", "trimmed");
    assert_eq!(saved.cron, "0 9 * * 1-5", "spaces normalized");
    assert_eq!(saved.dir, task.dir, "absolute again once read");
    assert_eq!(saved.budget, Some(2.5));

    let text = fs::read_to_string(env.root.join("config").join(FILE)).unwrap();
    assert!(text.contains("[tasks.daily-review]\n"));
    assert!(text.contains("permissions = \"dontAsk\""));
    assert!(text.contains("catchup = true"));

    let mut edited = saved.clone();
    edited.model = Some("".into());
    edited.mode = Mode::Tile;
    s.save(Some(&id), edited, None).unwrap();
    let edited = s.task(&id).unwrap();
    assert_eq!(
        edited.model, None,
        "an empty value means Claude Code's default"
    );
    assert_eq!(edited.mode, Mode::Tile);
    let text = fs::read_to_string(env.root.join("config").join(FILE)).unwrap();
    let first = text.split("[tasks.daily-review-2]").next().unwrap();
    assert!(
        !first.contains("model"),
        "unset options are left out:\n{first}"
    );

    assert!(s.save(Some("missing"), task.clone(), None).is_err());
    assert!(s.save(Some("../x"), task.clone(), None).is_err());
    s.delete("daily-review-2").unwrap();
    assert_eq!(
        s.tasks().unwrap().keys().collect::<Vec<_>>(),
        ["daily-review"]
    );
    assert!(s.delete("daily-review-2").is_err());
}

#[test]
fn refuses_what_cannot_run() {
    let env = Env::new("refuse");
    let s = &env.scheduler;
    let ok = env.task("Review", "0 9 * * *");
    let refused = |change: fn(&mut Task)| {
        let mut task = ok.clone();
        change(&mut task);
        s.save(None, task, None).is_err()
    };
    assert!(refused(|t| t.name = " ".into()));
    assert!(refused(|t| t.prompt = "\n".into()));
    assert!(refused(|t| t.cron = "0 9 * *".into()));
    assert!(refused(|t| t.cron = "0 0 31 2 *".into()), "never comes");
    assert!(refused(|t| t.dir.push_str("/missing")));
    assert!(refused(
        |t| t.model = Some("--dangerously-skip-permissions".into())
    ));
    assert!(refused(|t| t.effort = Some("high; rm".into())));
    assert!(refused(|t| t.budget = Some(0.0)));
    assert!(refused(|t| t.budget = Some(f64::NAN)));
    assert!(s.tasks().unwrap().is_empty());
}

#[test]
fn due_runs_on_time_and_catches_up_once() {
    let utc = TimeZone::UTC;
    let cron = Cron::parse("0 9 * * *").unwrap();
    let nine = |day: i64| 1_795_251_600 + day * 86_400; // 2026-11-21 09:00 UTC
    assert_eq!(
        due(&cron, &utc, nine(-1), nine(0) + 30, false),
        Some((nine(0), Trigger::Schedule))
    );
    assert_eq!(
        due(&cron, &utc, nine(0) + 60, nine(0) + 90, true),
        None,
        "a time before the task was on is never due"
    );
    // Off for three days: one catch-up, of the last time only.
    assert_eq!(
        due(&cron, &utc, nine(-4), nine(0) + 3600, true),
        Some((nine(0), Trigger::Catchup))
    );
    assert_eq!(due(&cron, &utc, nine(-4), nine(0) + 3600, false), None);
    assert_eq!(
        due(&cron, &utc, nine(-4), nine(0) + GRACE, false),
        Some((nine(0), Trigger::Schedule)),
        "a little late is still on time"
    );
}

#[test]
fn a_scheduled_time_runs_once_across_the_app_and_the_cli() {
    let env = Env::new("lock");
    let id = env
        .scheduler
        .save(None, env.task("Nightly", "0 * * * *"), None)
        .unwrap();
    let task = env.scheduler.task(&id).unwrap();
    // The CLI the system's scheduler started, on the same database.
    let mut cli = Scheduler::new(env.root.join("config"), env.root.join("data"));
    cli.system = None;

    let at = 1_795_251_600;
    let slot = Some(at);
    let app_run = env
        .scheduler
        .claim(&id, &task, Trigger::Schedule, slot, Host::App)
        .unwrap()
        .expect("first claim wins");
    assert_eq!(app_run.status, Status::Running);
    assert_eq!(app_run.slot, slot);
    assert!(
        cli.claim(&id, &task, Trigger::Schedule, slot, Host::Cli)
            .unwrap()
            .is_none()
    );
    assert!(
        cli.claim(&id, &task, Trigger::Catchup, slot, Host::Cli)
            .unwrap()
            .is_none(),
        "nor as a catch-up"
    );
    let other = Some(at + 3600);
    assert!(
        cli.claim(&id, &task, Trigger::Schedule, other, Host::Cli)
            .unwrap()
            .is_some()
    );
    // Asked by hand: always runs.
    for _ in 0..2 {
        assert!(
            cli.claim(&id, &task, Trigger::Manual, None, Host::Cli)
                .unwrap()
                .is_some()
        );
    }
}

#[test]
fn catches_up_one_missed_run_after_a_long_stop() {
    let env = Env::new("catchup");
    let s = &env.scheduler;
    let id = s.save(None, env.task("Hourly", "0 * * * *"), None).unwrap();
    let task = s.task(&id).unwrap();
    let later = now() + 3 * 86_400 + 1800;
    let (slot, trigger) = s.due(&id, &task, later).unwrap().expect("missed runs");
    assert!(later - slot > GRACE && slot <= later);
    assert_eq!(trigger, Trigger::Catchup);
    assert!(
        s.claim(&id, &task, trigger, Some(slot), Host::App)
            .unwrap()
            .is_some()
    );
    let again = s.due(&id, &task, later + 60).unwrap().unwrap();
    assert_eq!(again.0, slot, "still the same time");
    assert!(
        s.claim(&id, &task, again.1, Some(again.0), Host::Cli)
            .unwrap()
            .is_none(),
        "only one catch-up"
    );

    let mut off = task.clone();
    off.catchup = false;
    s.save(Some(&id), off.clone(), None).unwrap();
    assert_eq!(s.due(&id, &off, later).unwrap(), None);

    // A new schedule, or switching the task back on, never catches up what came before.
    let mut changed = task.clone();
    changed.cron = "30 * * * *".into();
    s.save(Some(&id), changed.clone(), None).unwrap();
    changed.enabled = false;
    assert_eq!(s.due(&id, &changed, later).unwrap(), None);
    changed.enabled = true;
    assert_eq!(s.due(&id, &changed, later).unwrap(), None);
}

#[test]
fn keeps_the_history() {
    let env = Env::new("history");
    let s = &env.scheduler;
    let a = s.save(None, env.task("Alpha", "0 9 * * *"), None).unwrap();
    let b = s.save(None, env.task("Beta", "0 10 * * *"), None).unwrap();
    let (ta, tb) = (s.task(&a).unwrap(), s.task(&b).unwrap());
    let first = s
        .claim(&a, &ta, Trigger::Manual, None, Host::App)
        .unwrap()
        .unwrap();
    s.claim(&b, &tb, Trigger::Schedule, Some(100), Host::Cli)
        .unwrap();
    let third = s
        .claim(&a, &ta, Trigger::Catchup, Some(200), Host::App)
        .unwrap()
        .unwrap();

    let done = s
        .finish(
            first.id,
            &Outcome {
                status: Status::Succeeded,
                cost: Some(0.42),
                output: Some("3 commits, all green.".into()),
                error: None,
            },
        )
        .unwrap();
    assert_eq!(done.status, Status::Succeeded);
    assert_eq!(done.cost, Some(0.42));
    assert!(done.finished.is_some());

    let ids = |runs: Vec<Execution>| runs.iter().map(|r| r.id).collect::<Vec<_>>();
    assert_eq!(ids(s.history(Some(&a), 10).unwrap()), [third.id, first.id]);
    assert_eq!(s.history(None, 10).unwrap().len(), 3);
    assert_eq!(s.history(None, 1).unwrap()[0].id, third.id);
    assert_eq!(
        s.run(first.id).unwrap().output.as_deref(),
        Some("3 commits, all green.")
    );

    // The app stopped during `third`; the CLI's run may still be going.
    s.interrupted(Some(Host::App)).unwrap();
    let runs = s.history(None, 10).unwrap();
    assert_eq!(runs[0].status, Status::Failed);
    assert_eq!(runs[0].error.as_deref(), Some("interrupted before the end"));
    assert_eq!(runs[1].status, Status::Running);

    // History outlives its task.
    s.delete(&a).unwrap();
    assert_eq!(s.history(Some(&a), 10).unwrap().len(), 2);
}

#[test]
fn passes_the_task_to_claude_code() {
    let env = Env::new("args");
    let task = env.task("Review", "0 9 * * *");
    assert_eq!(
        headless_args(&task),
        [
            "-p",
            "--output-format",
            "json",
            "--model",
            "sonnet",
            "--effort",
            "high",
            "--permission-mode",
            "dontAsk",
            "--max-budget-usd",
            "2.5",
        ]
    );
    assert_eq!(
        tile_args(&task),
        [
            "--model",
            "sonnet",
            "--effort",
            "high",
            "--permission-mode",
            "dontAsk",
            "Summarize yesterday's commits.\nKeep it short.",
        ]
    );
    let mut bare = task.clone();
    (bare.model, bare.effort, bare.permissions, bare.budget) = (None, None, None, None);
    assert_eq!(headless_args(&bare), ["-p", "--output-format", "json"]);
}

#[test]
fn reads_claude_code_answers() {
    let ok = run::outcome(
        true,
        "profile noise\n{\"type\":\"result\",\"is_error\":false,\"result\":\"Done.\",\"total_cost_usd\":0.12}\n",
        "",
    );
    assert_eq!(
        (ok.status, ok.cost, ok.output.as_deref()),
        (Status::Succeeded, Some(0.12), Some("Done."))
    );
    let over = run::outcome(
        false,
        "{\"type\":\"result\",\"is_error\":true,\"subtype\":\"error_max_budget_usd\",\"total_cost_usd\":2.51}",
        "",
    );
    assert_eq!(over.status, Status::Failed);
    assert_eq!(over.error.as_deref(), Some("error_max_budget_usd"));
    assert_eq!(over.cost, Some(2.51), "what it spent before stopping");
    let crash = run::outcome(false, "", "Error: not logged in\n");
    assert_eq!(crash.error.as_deref(), Some("Error: not logged in"));
}

#[cfg(unix)]
#[test]
fn runs_headless_with_a_fake_claude() {
    let env = Env::new("headless");
    let s = &env.scheduler;
    let id = s.save(None, env.task("Review", "0 9 * * *"), None).unwrap();
    let task = s.task(&id).unwrap();
    let fake = env.root.join("fake-claude.sh");
    let log = env.root.join("log");
    fs::write(
        &fake,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}.args'\npwd > '{0}.cwd'\ncat > '{0}.stdin'\necho 'profile noise'\necho '{{\"type\":\"result\",\"is_error\":false,\"result\":\"All good.\",\"total_cost_usd\":0.3}}'\n",
            log.display()
        ),
    )
    .unwrap();
    let claude = vec!["/bin/sh".to_owned(), fake.to_string_lossy().into_owned()];

    let run = s
        .claim(&id, &task, Trigger::Manual, None, Host::Cli)
        .unwrap()
        .unwrap();
    let done = s.run_headless(&run, &task, &claude).unwrap();
    assert_eq!(done.status, Status::Succeeded);
    assert_eq!(done.output.as_deref(), Some("All good."));
    assert_eq!(done.cost, Some(0.3));
    let read = |ext: &str| fs::read_to_string(format!("{}.{ext}", log.display())).unwrap();
    assert_eq!(
        read("args"),
        "-p\n--output-format\njson\n--model\nsonnet\n--effort\nhigh\n--permission-mode\ndontAsk\n--max-budget-usd\n2.5\n"
    );
    assert_eq!(read("stdin"), task.prompt);
    assert_eq!(
        fs::canonicalize(read("cwd").trim()).unwrap(),
        fs::canonicalize(&env.dir).unwrap()
    );

    fs::write(
        &fake,
        "#!/bin/sh\necho 'Error: not logged in' >&2\nexit 1\n",
    )
    .unwrap();
    let run = s
        .claim(&id, &task, Trigger::Manual, None, Host::Cli)
        .unwrap()
        .unwrap();
    let failed = s.run_headless(&run, &task, &claude).unwrap();
    assert_eq!(failed.status, Status::Failed);
    assert_eq!(failed.error.as_deref(), Some("Error: not logged in"));

    fs::write(&fake, "#!/bin/sh\nsleep 30\n").unwrap();
    let slow = run::headless(&claude, &task, std::time::Duration::from_millis(300));
    assert_eq!(slow.status, Status::Failed);
    assert!(slow.error.unwrap().starts_with("stopped after"));
}

#[test]
fn reads_claude_desktop_tasks() {
    let env = Env::new("desktop");
    let claude = env.root.join("claude");
    assert!(desktop_tasks(&claude).is_empty());
    let put = |name: &str, text: &str| {
        let dir = claude.join("scheduled-tasks").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), text).unwrap();
    };
    put(
        "morning-brief",
        "---\nname: Morning brief\ndescription: >\n  Reads my inbox\n  and the calendar\n---\n\nSummarize today's meetings.\n",
    );
    put("inbox-zero", "Archive newsletters older than a week.\n");
    fs::create_dir_all(claude.join("scheduled-tasks/empty")).unwrap();
    let tasks = desktop_tasks(&claude);
    assert_eq!(tasks.len(), 2, "a folder without SKILL.md is not a task");
    assert_eq!(tasks[0].name, "inbox-zero");
    assert_eq!(tasks[0].description, None);
    assert_eq!(tasks[0].prompt, "Archive newsletters older than a week.");
    assert_eq!(tasks[1].name, "Morning brief");
    assert_eq!(
        tasks[1].description.as_deref(),
        Some("Reads my inbox and the calendar")
    );
    assert_eq!(tasks[1].prompt, "Summarize today's meetings.");
    assert!(tasks[1].path.ends_with("SKILL.md"));
}
