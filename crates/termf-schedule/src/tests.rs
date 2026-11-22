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
        scheduler.machine = "alex-mbp".into();
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
            machine: Some("alex-mbp".into()),
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
    cli.machine = "alex-mbp".into();

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
    // 25 min past a slot, so the last hourly run is always past the grace period whatever the clock says.
    let later = upcoming("0 * * * *", now() + 3 * 86_400, 1).unwrap()[0] + 1500;
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

    // Just claimed, no process yet: still starting.
    s.interrupted().unwrap();
    assert_eq!(s.run(third.id).unwrap().status, Status::Running);

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
            "--",
            "Summarize yesterday's commits.\nKeep it short.",
        ]
    );
    let mut dash = task.clone();
    dash.prompt = "- list the open PRs".into();
    assert_eq!(
        tile_args(&dash)[6..],
        ["--", "- list the open PRs"],
        "a prompt starting with - is not an option"
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

/// An executable stand-in for Claude Code that records its arguments, folder and stdin next to itself.
#[cfg(unix)]
fn fake_claude(path: &std::path::Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let log = path.with_extension("log");
    fs::write(
        path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}.args'\npwd > '{0}.cwd'\ncat > '{0}.stdin'\n{body}\n",
            log.display()
        ),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn runs_headless_with_a_fake_claude() {
    let env = Env::new("headless");
    let s = &env.scheduler;
    let id = s.save(None, env.task("Review", "0 9 * * *"), None).unwrap();
    let task = s.task(&id).unwrap();
    let home = env.root.join("home");
    let ok = "echo 'profile noise'\necho '{\"type\":\"result\",\"is_error\":false,\"result\":\"All good.\",\"total_cost_usd\":0.3}'";
    // The agent's command goes through the login shell, as a tile's: `~` and quoted paths with spaces work.
    let tilde = home.join("bin/claude");
    let spaced = env.root.join("my tools/claude");
    fake_claude(&tilde, ok);
    fake_claude(&spaced, ok);
    for (agent, fake) in [
        ("~/bin/claude".to_owned(), &tilde),
        (format!("\"{}\"", spaced.display()), &spaced),
    ] {
        let mut cmd = headless_command(&agent, &task);
        // Never the user's own shell profile.
        cmd.env("HOME", &home);
        let mut pid = None;
        let done = run::headless(cmd, &task, std::time::Duration::from_secs(60), WHO, |p| {
            pid = Some(p)
        });
        assert_eq!(done.status, Status::Succeeded, "{agent}: {:?}", done.error);
        assert_eq!(done.output.as_deref(), Some("All good."));
        assert_eq!(done.cost, Some(0.3));
        assert!(pid.is_some());
        let log = fake.with_extension("log");
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
    }

    fake_claude(&tilde, "echo 'Error: not logged in' >&2\nexit 1");
    let mut cmd = headless_command("~/bin/claude", &task);
    cmd.env("HOME", &home);
    let failed = run::headless(cmd, &task, std::time::Duration::from_secs(60), WHO, |_| {});
    assert_eq!(failed.status, Status::Failed);
    assert_eq!(
        failed.error.as_deref(),
        Some("Claude Code 2.1.330 at ~/bin/claude: Error: not logged in"),
        "what the process said, after which Claude Code said it"
    );

    let started = env.root.join("started");
    fake_claude(
        &tilde,
        &format!("sleep 30 & echo $! > '{}'; wait", started.display()),
    );
    let mut cmd = headless_command("~/bin/claude", &task);
    cmd.env("HOME", &home);
    let slow = run::headless(cmd, &task, std::time::Duration::from_secs(2), WHO, |_| {});
    assert_eq!(slow.status, Status::Failed);
    let error = slow.error.unwrap();
    assert!(error.starts_with("stopped after"), "{error}");
    let pid = fs::read_to_string(&started).unwrap();
    assert!(gone(pid.trim()), "what it started is stopped with it");

    let mut lost = task.clone();
    lost.dir = env.root.join("gone").to_string_lossy().into_owned();
    let missing = run::headless(
        headless_command("~/bin/claude", &lost),
        &lost,
        std::time::Duration::from_secs(2),
        WHO,
        |_| {},
    );
    assert!(
        missing.error.unwrap().starts_with("the folder "),
        "not from Claude Code"
    );
}

const WHO: &str = "Claude Code 2.1.330 at ~/bin/claude";

/// Within 5 s: dead, or a zombie its new parent has not reaped yet.
#[cfg(unix)]
fn gone(pid: &str) -> bool {
    (0..50).any(|_| {
        let out = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", pid])
            .output();
        let stat = String::from_utf8_lossy(&out.unwrap().stdout)
            .trim()
            .to_owned();
        let gone = stat.is_empty() || stat.starts_with('Z');
        if !gone {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        gone
    })
}

#[test]
fn tells_whether_a_process_still_runs() {
    assert!(run::alive(std::process::id()));
    let mut quick = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/c", "exit"])
            .spawn()
            .unwrap()
    } else {
        std::process::Command::new("true").spawn().unwrap()
    };
    let pid = quick.id();
    quick.wait().unwrap();
    assert!(!run::alive(pid), "gone once reaped");
}

#[cfg(unix)]
#[test]
fn fails_only_the_runs_whose_process_is_gone() {
    let env = Env::new("interrupted");
    let s = &env.scheduler;
    let id = s
        .save(None, env.task("Nightly", "0 3 * * *"), None)
        .unwrap();
    let task = s.task(&id).unwrap();
    let claim = || {
        s.claim(&id, &task, Trigger::Manual, None, Host::Cli)
            .unwrap()
            .unwrap()
            .id
    };
    let (running, gone, starting) = (claim(), claim(), claim());
    let mut sleeper = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let mut quick = std::process::Command::new("true").spawn().unwrap();
    let dead = quick.id();
    quick.wait().unwrap();
    let store = s.store().unwrap();
    store.set_pid(running, sleeper.id()).unwrap();
    store.set_pid(gone, dead).unwrap();

    s.interrupted().unwrap();
    let status = |run| s.run(run).unwrap().status;
    assert_eq!(status(running), Status::Running, "its process still runs");
    assert_eq!(status(gone), Status::Failed);
    assert_eq!(
        s.run(gone).unwrap().error.as_deref(),
        Some("interrupted before the end")
    );
    assert_eq!(status(starting), Status::Running, "claimed, about to start");

    sleeper.kill().unwrap();
    sleeper.wait().unwrap();
    s.interrupted().unwrap();
    assert_eq!(status(running), Status::Failed);
}

#[cfg(unix)]
#[test]
fn a_run_never_outlives_its_runner() {
    use std::os::unix::process::CommandExt;
    let env = Env::new("watchdog");
    // A group with a process of its own in the background, whose pid goes in `file`.
    let group = |file: &str| {
        let file = env.root.join(file);
        let child = std::process::Command::new("sh")
            .args(["-c", "sleep 30 & echo $! > \"$1\"; wait", "sh"])
            .arg(&file)
            .process_group(0)
            .spawn()
            .unwrap();
        while !fs::read_to_string(&file).is_ok_and(|p| p.ends_with('\n')) {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        (child, file)
    };
    let background = |file: &PathBuf| fs::read_to_string(file).unwrap().trim().to_owned();
    let ended = |child: &mut std::process::Child| {
        (0..50).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            child.try_wait().unwrap().is_some()
        })
    };
    let mut quick = std::process::Command::new("true").spawn().unwrap();
    let gone_runner = quick.id();
    quick.wait().unwrap();
    let (mut orphan, file) = group("orphan");
    let mut dog = run::watchdog(orphan.id(), gone_runner, now() + 3600).unwrap();
    assert!(ended(&mut orphan), "its runner is gone");
    assert!(gone(&background(&file)), "the whole group");
    dog.wait().unwrap();

    let (mut late, file) = group("late");
    let mut dog = run::watchdog(late.id(), std::process::id(), now() - 1).unwrap();
    assert!(ended(&mut late), "its time is up");
    assert!(gone(&background(&file)), "the whole group");
    dog.wait().unwrap();
}

#[test]
fn runs_only_on_its_machine() {
    let env = Env::new("machine");
    let s = &env.scheduler;
    let mut elsewhere = env.task("Nightly", "0 * * * *");
    elsewhere.machine = Some("office-pc".into());
    elsewhere.dir = env.root.join("not here").to_string_lossy().into_owned();
    let id = s
        .save(None, elsewhere.clone(), None)
        .expect("its folder is on office-pc");
    let task = s.task(&id).unwrap();
    assert!(!s.runs_here(&task));
    assert_eq!(s.due(&id, &task, now() + 3 * 86_400).unwrap(), None);
    assert!(
        s.claim(&id, &task, Trigger::Manual, None, Host::App)
            .is_err()
    );

    let mut anywhere = task.clone();
    anywhere.machine = Some(" ".into());
    assert!(
        s.save(Some(&id), anywhere.clone(), None).is_err(),
        "on every machine, here too: its folder must exist"
    );
    anywhere.dir = env.dir.to_string_lossy().into_owned();
    s.save(Some(&id), anywhere, None).unwrap();
    let task = s.task(&id).unwrap();
    assert_eq!(task.machine, None);
    assert!(s.runs_here(&task));
    assert!(
        s.claim(&id, &task, Trigger::Manual, None, Host::App)
            .unwrap()
            .is_some()
    );
}

#[test]
fn switches_a_task_whose_folder_is_gone() {
    let env = Env::new("gone");
    let s = &env.scheduler;
    let id = s
        .save(None, env.task("Nightly", "0 * * * *"), None)
        .unwrap();
    fs::remove_dir_all(&env.dir).unwrap();
    let mut task = s.task(&id).unwrap();
    task.enabled = false;
    task.catchup = false;
    s.save(Some(&id), task.clone(), None)
        .expect("switching it off needs no folder");
    task.dir = env.root.join("elsewhere").to_string_lossy().into_owned();
    assert!(
        s.save(Some(&id), task, None).is_err(),
        "a new folder must exist"
    );
}

#[cfg(unix)]
#[test]
fn updates_launchd_without_stopping_a_run_and_removes_orphans() {
    use std::os::unix::fs::PermissionsExt;
    let mut env = Env::new("launchd");
    let (states, log) = (env.root.join("states"), env.root.join("launchctl.log"));
    fs::create_dir_all(&states).unwrap();
    // A launchctl that keeps each job's state in a file: `print` fails for a job not loaded.
    let fake = env.root.join("launchctl");
    fs::write(
        &fake,
        format!(
            r#"#!/bin/sh
echo "$1 ${{2##*/}} ${{3##*/}}" >> '{log}'
case "$1" in
  print) [ -f '{states}'/"${{2##*/}}" ] || exit 113; echo "state = $(cat '{states}'/"${{2##*/}}")";;
  bootout) rm -f '{states}'/"${{2##*/}}";;
  bootstrap) b="${{3##*/}}"; echo waiting > '{states}'/"${{b%.plist}}";;
esac
"#,
            log = log.display(),
            states = states.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    env.scheduler.system = Some(os::System {
        platform: os::Platform::Launchd,
        home: env.root.join("home"),
        config: env.root.join("home/.config"),
        data: env.root.join("data"),
        launchctl: fake.to_string_lossy().into_owned(),
    });
    let s = &env.scheduler;
    let termf = std::path::Path::new("/opt/termf");
    let calls = || {
        let text = fs::read_to_string(&log).unwrap_or_default();
        let _ = fs::remove_file(&log);
        text.lines()
            .map(|l| l.trim().to_owned())
            .collect::<Vec<_>>()
    };
    let label = "dev.termf.schedule.nightly";
    let plist = env
        .root
        .join(format!("home/Library/LaunchAgents/{label}.plist"));

    let id = s
        .save(None, env.task("Nightly", "0 3 * * *"), Some(termf))
        .unwrap();
    s.install(&id, termf).unwrap();
    assert_eq!(
        calls(),
        [
            format!("print {label}"),
            format!("bootstrap {} {label}.plist", termf_core::ipc::uid())
        ]
    );
    assert!(plist.is_file());

    // Loaded and waiting: unloaded, gone, loaded again with the new schedule.
    let mut task = s.task(&id).unwrap();
    task.cron = "0 4 * * *".into();
    s.save(Some(&id), task.clone(), Some(termf)).unwrap();
    let seen = calls();
    assert_eq!(
        seen[..3],
        [
            format!("print {label}"),
            format!("bootout {label}"),
            format!("print {label}")
        ]
    );
    assert!(seen[3].starts_with("bootstrap "));
    assert!(
        fs::read_to_string(&plist)
            .unwrap()
            .contains("<integer>4</integer>")
    );

    // Running: left alone until a later refresh.
    fs::write(states.join(label), "running").unwrap();
    task.cron = "0 5 * * *".into();
    s.save(Some(&id), task, Some(termf)).unwrap();
    assert_eq!(calls(), [format!("print {label}")]);
    assert!(
        fs::read_to_string(&plist)
            .unwrap()
            .contains("<integer>4</integer>")
    );
    fs::write(states.join(label), "waiting").unwrap();
    s.refresh_entries(termf);
    assert!(calls().contains(&format!("bootout {label}")));
    assert!(
        fs::read_to_string(&plist)
            .unwrap()
            .contains("<integer>5</integer>")
    );

    // Entries of a deleted task, and of a task now in a tile, are removed.
    let agents = env.root.join("home/Library/LaunchAgents");
    fs::write(agents.join("dev.termf.schedule.deleted.plist"), "x").unwrap();
    let mut tile = env.task("Weekly", "0 9 * * 1");
    tile.mode = Mode::Tile;
    let tile_id = s.save(None, tile, Some(termf)).unwrap();
    fs::write(
        agents.join(format!("dev.termf.schedule.{tile_id}.plist")),
        "x",
    )
    .unwrap();
    assert_eq!(
        s.system.as_ref().unwrap().installed_ids(),
        ["deleted", "nightly", "weekly"]
    );
    s.refresh_entries(termf);
    assert_eq!(s.system.as_ref().unwrap().installed_ids(), ["nightly"]);
    let seen = calls();
    assert!(seen.contains(&"bootout dev.termf.schedule.deleted".to_owned()));
    assert!(seen.contains(&"bootout dev.termf.schedule.weekly".to_owned()));
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
