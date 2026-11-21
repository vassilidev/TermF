//! Entries in the system's scheduler (launchd, a systemd user timer, the Windows Task Scheduler) that run
//! `termf schedule run <id> --scheduled` while TermF is closed. That command checks the schedule itself, so an
//! entry may fire more often than its task runs, never less.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, ensure};

use crate::cron::{Cron, DAY, HOUR, MINUTE, MONTH, WEEKDAY};

/// launchd reads this many calendar entries without trouble; beyond, days and months are left to the command.
const MAX_INTERVALS: usize = 1440;
/// The Task Scheduler keeps at most 48 triggers per task.
const MAX_TRIGGERS: usize = 48;
const WEEKDAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Launchd,
    Systemd,
    TaskScheduler,
}

/// Where this machine keeps scheduler entries.
#[derive(Clone, Debug)]
pub struct System {
    pub platform: Platform,
    pub home: PathBuf,
    /// `~/.config` on Linux (or `$XDG_CONFIG_HOME`).
    pub config: PathBuf,
    /// TermF's data folder, which keeps the Task Scheduler's task files.
    pub data: PathBuf,
    /// The `launchctl` program (a stand-in in tests).
    pub launchctl: String,
}

impl System {
    pub fn current(data_dir: &Path) -> Option<System> {
        let platform = if cfg!(target_os = "macos") {
            Platform::Launchd
        } else if cfg!(windows) {
            Platform::TaskScheduler
        } else if cfg!(target_os = "linux") {
            Platform::Systemd
        } else {
            return None;
        };
        Some(System {
            platform,
            home: dirs::home_dir()?,
            config: dirs::config_dir()?,
            data: data_dir.into(),
            launchctl: "launchctl".into(),
        })
    }

    /// The entry's files; the first one tells whether it is installed.
    pub fn paths(&self, id: &str) -> Vec<PathBuf> {
        match self.platform {
            Platform::Launchd => vec![
                self.home
                    .join("Library/LaunchAgents")
                    .join(format!("{}.plist", label(id))),
            ],
            Platform::Systemd => {
                let dir = self.config.join("systemd/user");
                vec![
                    dir.join(format!("{}.timer", unit(id))),
                    dir.join(format!("{}.service", unit(id))),
                ]
            }
            Platform::TaskScheduler => vec![self.data.join("schedule").join(format!("{id}.xml"))],
        }
    }

    pub fn installed(&self, id: &str) -> bool {
        self.paths(id)[0].is_file()
    }

    /// The tasks this machine has entries for, whether or not they still exist.
    pub fn installed_ids(&self) -> Vec<String> {
        let first = self.paths("x").remove(0);
        let name = first.file_name().unwrap_or_default().to_string_lossy();
        let (prefix, suffix) = name.split_once('x').unwrap_or_default();
        let Ok(entries) = fs::read_dir(first.parent().unwrap_or(Path::new("."))) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let id = name.strip_prefix(prefix)?.strip_suffix(suffix)?;
                (!id.is_empty()).then(|| id.to_owned())
            })
            .collect();
        ids.sort();
        ids
    }

    /// The files of the entry that runs task `id`, in the order of [`System::paths`].
    pub fn files(
        &self,
        id: &str,
        name: &str,
        termf: &Path,
        cron: &Cron,
        catchup: bool,
    ) -> Vec<(PathBuf, Vec<u8>)> {
        let paths = self.paths(id);
        let contents = match self.platform {
            Platform::Launchd => vec![launchd(id, termf, cron).into_bytes()],
            Platform::Systemd => {
                let (timer, service) = systemd(id, name, termf, cron, catchup);
                vec![timer.into_bytes(), service.into_bytes()]
            }
            // The Task Scheduler reads task files in UTF-16.
            Platform::TaskScheduler => {
                let xml = task_xml(name, termf, id, cron, catchup);
                let mut bytes = vec![0xFF, 0xFE];
                bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
                vec![bytes]
            }
        };
        paths.into_iter().zip(contents).collect()
    }

    /// launchd's description of the job, `None` when it is not loaded.
    fn launchd_job(&self, id: &str) -> Option<String> {
        let out = command(&self.launchctl)
            .args(["print", &service_target(id)])
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Writes the entry and has the system load it, replacing what was there. A launchd job that is running
    /// is left alone: unloading it would stop its run, so the update waits for a later refresh.
    pub fn install(&self, id: &str, files: &[(PathBuf, Vec<u8>)]) -> Result<()> {
        if self.platform == Platform::Launchd
            && let Some(job) = self.launchd_job(id)
        {
            if job.contains("state = running") {
                return Ok(());
            }
            let _ = exec(&self.launchctl, &["bootout", &service_target(id)]);
            // Unloading goes on after `bootout` returns; loading again too soon fails (I/O error 5).
            for _ in 0..40 {
                if self.launchd_job(id).is_none() {
                    break;
                }
                sleep(Duration::from_millis(250));
            }
        }
        for (path, bytes) in files {
            fs::create_dir_all(path.parent().context("no parent folder")?)?;
            let tmp = path.with_extension("tmp");
            fs::write(&tmp, bytes)?;
            fs::rename(&tmp, path)?;
        }
        match self.platform {
            Platform::Launchd => {
                let plist = files[0].0.to_string_lossy();
                let mut result = Ok(());
                for _ in 0..5 {
                    result = exec(&self.launchctl, &["bootstrap", &domain(), &plist]);
                    if result.is_ok() {
                        break;
                    }
                    sleep(Duration::from_millis(500));
                }
                result
            }
            Platform::Systemd => {
                let timer = format!("{}.timer", unit(id));
                exec("systemctl", &["--user", "daemon-reload"])?;
                exec("systemctl", &["--user", "enable", "--now", &timer])?;
                exec("systemctl", &["--user", "restart", &timer])
            }
            Platform::TaskScheduler => exec(
                "schtasks",
                &[
                    "/Create",
                    "/TN",
                    &task_name(id),
                    "/XML",
                    &files[0].0.to_string_lossy(),
                    "/F",
                ],
            ),
        }
    }

    pub fn uninstall(&self, id: &str) -> Result<()> {
        if !self.paths(id).iter().any(|p| p.exists()) {
            return Ok(());
        }
        // Unloading fails when the entry was never loaded: removing its files is what matters.
        match self.platform {
            Platform::Launchd => {
                let _ = exec(&self.launchctl, &["bootout", &service_target(id)]);
            }
            Platform::Systemd => {
                let timer = format!("{}.timer", unit(id));
                let _ = exec("systemctl", &["--user", "disable", "--now", &timer]);
            }
            Platform::TaskScheduler => {
                let _ = exec("schtasks", &["/Delete", "/TN", &task_name(id), "/F"]);
            }
        }
        for path in self.paths(id) {
            match fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(e).with_context(|| format!("cannot remove {}", path.display()));
                }
                _ => {}
            }
        }
        if self.platform == Platform::Systemd {
            let _ = exec("systemctl", &["--user", "daemon-reload"]);
        }
        Ok(())
    }
}

fn label(id: &str) -> String {
    format!("dev.termf.schedule.{id}")
}

fn unit(id: &str) -> String {
    format!("termf-schedule-{id}")
}

fn task_name(id: &str) -> String {
    format!(r"TermF\{id}")
}

fn domain() -> String {
    #[cfg(unix)]
    let uid = termf_core::ipc::uid();
    #[cfg(not(unix))]
    let uid = 0;
    format!("gui/{uid}")
}

fn service_target(id: &str) -> String {
    format!("{}/{}", domain(), label(id))
}

/// A command that opens no console window on Windows.
fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn exec(program: &str, args: &[&str]) -> Result<()> {
    let out = command(program)
        .args(args)
        .output()
        .with_context(|| format!("cannot run {program}"))?;
    ensure!(
        out.status.success(),
        "{program} {} failed: {}",
        args[0],
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn args(termf: &Path, id: &str) -> Vec<String> {
    let termf = termf.to_string_lossy().into_owned();
    [termf.as_str(), "schedule", "run", id, "--scheduled"]
        .map(str::to_owned)
        .to_vec()
}

type Interval = Vec<(&'static str, u8)>;

/// launchd's `StartCalendarInterval`: one entry per combination of values, a key left out matching any value.
fn calendar(cron: &Cron) -> Vec<Interval> {
    let options = |field: usize, key: &'static str| -> Vec<Interval> {
        cron.values(field).map(|v| vec![(key, v)]).collect()
    };
    let product = |a: Vec<Interval>, b: Vec<Interval>| -> Vec<Interval> {
        a.iter()
            .flat_map(|x| b.iter().map(move |y| [x.clone(), y.clone()].concat()))
            .collect()
    };
    let mut times = vec![vec![]];
    for (field, key) in [(MINUTE, "Minute"), (HOUR, "Hour")] {
        if !cron.is_full(field) {
            times = product(times, options(field, key));
        }
    }
    let mut days = vec![vec![]];
    if !cron.is_full(MONTH) {
        days = product(days, options(MONTH, "Month"));
    }
    let (day, weekday) = (!cron.is_full(DAY), !cron.is_full(WEEKDAY));
    days = match (cron.days_either(), day, weekday) {
        (true, true, true) => product(
            days,
            [options(DAY, "Day"), options(WEEKDAY, "Weekday")].concat(),
        ),
        // Either field matching and one matches every day: every day runs.
        (true, _, _) => days,
        (false, _, _) => {
            if day {
                days = product(days, options(DAY, "Day"));
            }
            if weekday {
                days = product(days, options(WEEKDAY, "Weekday"));
            }
            days
        }
    };
    if times.len() * days.len() > MAX_INTERVALS {
        return times;
    }
    product(times, days)
}

pub fn launchd(id: &str, termf: &Path, cron: &Cron) -> String {
    let mut text = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
        "<plist version=\"1.0\">\n<dict>\n"
    ));
    text += &format!(
        "\t<key>Label</key>\n\t<string>{}</string>\n",
        xml(&label(id))
    );
    text += "\t<key>ProgramArguments</key>\n\t<array>\n";
    for arg in args(termf, id) {
        text += &format!("\t\t<string>{}</string>\n", xml(&arg));
    }
    text += "\t</array>\n\t<key>StartCalendarInterval</key>\n\t<array>\n";
    for interval in calendar(cron) {
        text += "\t\t<dict>\n";
        for (key, value) in interval {
            text += &format!("\t\t\t<key>{key}</key>\n\t\t\t<integer>{value}</integer>\n");
        }
        text += "\t\t</dict>\n";
    }
    text + "\t</array>\n</dict>\n</plist>\n"
}

/// systemd reads `%` as a specifier and `$` as a variable, even in quotes.
fn systemd_text(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
        .replace('%', "%%")
        .replace('$', "$$")
}

fn on_calendar(cron: &Cron, day: bool, weekday: bool) -> String {
    let list = |field: usize, names: Option<&[&str; 7]>| -> String {
        cron.values(field)
            .map(|v| match names {
                Some(names) => names[v as usize].to_owned(),
                None => format!("{v:02}"),
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    let any = |field: usize, restrict: bool| !restrict || cron.is_full(field);
    let weekdays = if any(WEEKDAY, weekday) {
        String::new()
    } else {
        list(WEEKDAY, Some(&WEEKDAY_NAMES)) + " "
    };
    let part = |field: usize, restrict: bool| {
        if any(field, restrict) {
            "*".to_owned()
        } else {
            list(field, None)
        }
    };
    format!(
        "{weekdays}*-{}-{} {}:{}:00",
        part(MONTH, true),
        part(DAY, day),
        part(HOUR, true),
        part(MINUTE, true)
    )
}

/// The timer and the service it starts.
pub fn systemd(id: &str, name: &str, termf: &Path, cron: &Cron, catchup: bool) -> (String, String) {
    let description = format!("Description=TermF scheduled task: {}\n", systemd_text(name));
    let calendars = if cron.days_either() && !cron.is_full(DAY) && !cron.is_full(WEEKDAY) {
        vec![
            on_calendar(cron, true, false),
            on_calendar(cron, false, true),
        ]
    } else {
        vec![on_calendar(cron, true, true)]
    };
    let mut timer = format!("[Unit]\n{description}\n[Timer]\n");
    for calendar in calendars {
        timer += &format!("OnCalendar={calendar}\n");
    }
    timer +=
        &format!("Persistent={catchup}\nAccuracySec=1s\n\n[Install]\nWantedBy=timers.target\n");
    let command = args(termf, id)
        .iter()
        .map(|a| {
            let a = systemd_text(a);
            if a.contains([' ', '"', '\\', '\'']) {
                format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\""))
            } else {
                a
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let service = format!("[Unit]\n{description}\n[Service]\nType=oneshot\nExecStart={command}\n");
    (timer, service)
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn trigger(minute_of_day: u32, repeat: Option<u32>) -> String {
    let repetition = repeat.map_or(String::new(), |every| {
        format!(
            "      <Repetition>\n        <Interval>PT{every}M</Interval>\n        <Duration>P1D</Duration>\n        <StopAtDurationEnd>false</StopAtDurationEnd>\n      </Repetition>\n"
        )
    });
    format!(
        "    <CalendarTrigger>\n{repetition}      <StartBoundary>2026-01-01T{:02}:{:02}:00</StartBoundary>\n      <ScheduleByDay>\n        <DaysInterval>1</DaysInterval>\n      </ScheduleByDay>\n    </CalendarTrigger>\n",
        minute_of_day / 60,
        minute_of_day % 60
    )
}

/// Daily triggers at each time of the task (the command checks the day); past the Task Scheduler's limit, one
/// trigger repeating at the largest step that lands on all of them.
pub fn task_xml(name: &str, termf: &Path, id: &str, cron: &Cron, catchup: bool) -> String {
    let mut times: Vec<u32> = Vec::new();
    for hour in cron.values(HOUR) {
        for minute in cron.values(MINUTE) {
            times.push(u32::from(hour) * 60 + u32::from(minute));
        }
    }
    let triggers = if times.len() <= MAX_TRIGGERS {
        times.iter().map(|t| trigger(*t, None)).collect::<String>()
    } else {
        let gaps = times
            .windows(2)
            .map(|w| w[1] - w[0])
            .chain([1440 - times[times.len() - 1] + times[0]]);
        trigger(times[0], Some(gaps.fold(0, gcd)))
    };
    let arguments = args(termf, id)[1..].join(" ");
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>TermF scheduled task: {}</Description>
  </RegistrationInfo>
  <Triggers>
{triggers}  </Triggers>
  <Settings>
    <MultipleInstancesPolicy>Parallel</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>{catchup}</StartWhenAvailable>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
  </Settings>
  <Actions>
    <Exec>
      <Command>{}</Command>
      <Arguments>{}</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        xml(name),
        xml(&termf.to_string_lossy()),
        xml(&arguments)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(platform: Platform) -> System {
        System {
            platform,
            home: "/home/alex".into(),
            config: "/home/alex/.config".into(),
            data: "/data/termf".into(),
            launchctl: "launchctl".into(),
        }
    }

    fn cron(expr: &str) -> Cron {
        Cron::parse(expr).unwrap()
    }

    const TERMF: &str = "/Applications/Term F.app/Contents/MacOS/termf";

    #[test]
    fn places_the_entries() {
        assert_eq!(
            system(Platform::Launchd).paths("daily"),
            [PathBuf::from(
                "/home/alex/Library/LaunchAgents/dev.termf.schedule.daily.plist"
            )]
        );
        assert_eq!(
            system(Platform::Systemd).paths("daily"),
            [
                PathBuf::from("/home/alex/.config/systemd/user/termf-schedule-daily.timer"),
                PathBuf::from("/home/alex/.config/systemd/user/termf-schedule-daily.service"),
            ]
        );
        assert_eq!(
            system(Platform::TaskScheduler).paths("daily"),
            [PathBuf::from("/data/termf/schedule/daily.xml")]
        );
        assert!(!system(Platform::Launchd).installed("daily"));
        let files = system(Platform::Systemd).files(
            "daily",
            "Daily",
            Path::new(TERMF),
            &cron("0 9 * * *"),
            true,
        );
        assert_eq!(files.len(), 2);
        assert!(String::from_utf8_lossy(&files[1].1).contains("ExecStart="));
    }

    #[test]
    fn writes_a_launchd_agent() {
        let plist = launchd("daily", Path::new(TERMF), &cron("0 9 * * 1-5"));
        assert!(plist.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist"));
        assert!(plist.contains("<key>Label</key>\n\t<string>dev.termf.schedule.daily</string>"));
        assert!(plist.contains(
            "\t\t<string>/Applications/Term F.app/Contents/MacOS/termf</string>\n\t\t<string>schedule</string>\n\t\t<string>run</string>\n\t\t<string>daily</string>\n\t\t<string>--scheduled</string>\n"
        ));
        assert_eq!(plist.matches("<dict>").count(), 1 + 5, "one per weekday");
        assert!(plist.contains(
            "\t\t<dict>\n\t\t\t<key>Minute</key>\n\t\t\t<integer>0</integer>\n\t\t\t<key>Hour</key>\n\t\t\t<integer>9</integer>\n\t\t\t<key>Weekday</key>\n\t\t\t<integer>1</integer>\n\t\t</dict>\n"
        ));
        assert!(launchd("x", Path::new("/a&b<c"), &cron("* * * * *")).contains("/a&amp;b&lt;c"));
    }

    #[test]
    fn launchd_intervals_follow_the_schedule() {
        let keys = |expr: &str| -> Vec<Vec<(&str, u8)>> { calendar(&cron(expr)) };
        assert_eq!(keys("* * * * *"), [vec![]], "every minute: no key");
        assert_eq!(
            keys("*/30 * * * *"),
            [vec![("Minute", 0)], vec![("Minute", 30)]]
        );
        assert_eq!(
            keys("0 12 1 * 0"),
            [
                vec![("Minute", 0), ("Hour", 12), ("Day", 1)],
                vec![("Minute", 0), ("Hour", 12), ("Weekday", 0)],
            ],
            "the 1st or a Sunday"
        );
        assert_eq!(
            keys("0 12 */2 * 0"),
            [vec![("Minute", 0), ("Hour", 12), ("Day", 1)]]
                .into_iter()
                .chain(
                    (3..=31)
                        .step_by(2)
                        .map(|d| vec![("Minute", 0), ("Hour", 12), ("Day", d)])
                )
                .map(|mut i| {
                    i.push(("Weekday", 0));
                    i
                })
                .collect::<Vec<_>>(),
            "an odd day that is a Sunday"
        );
        assert_eq!(
            keys("0 0 1 6 *"),
            [vec![("Minute", 0), ("Hour", 0), ("Month", 6), ("Day", 1)]]
        );
        // Too many combinations: the times alone, the command checks the days.
        assert_eq!(keys("*/2 */2 1-30 1-11 *").len(), 30 * 12);
    }

    #[test]
    fn writes_a_systemd_timer() {
        let (timer, service) = systemd(
            "daily",
            "Daily review 100% $HOME\nnext",
            Path::new(TERMF),
            &cron("0,30 9 * * mon-fri"),
            true,
        );
        assert_eq!(
            timer,
            "[Unit]\nDescription=TermF scheduled task: Daily review 100%% $$HOME next\n\n[Timer]\nOnCalendar=Mon,Tue,Wed,Thu,Fri *-*-* 09:00,30:00\nPersistent=true\nAccuracySec=1s\n\n[Install]\nWantedBy=timers.target\n"
        );
        assert_eq!(
            service,
            "[Unit]\nDescription=TermF scheduled task: Daily review 100%% $$HOME next\n\n[Service]\nType=oneshot\nExecStart=\"/Applications/Term F.app/Contents/MacOS/termf\" schedule run daily --scheduled\n"
        );
        let (timer, _) = systemd("x", "x", Path::new("/t"), &cron("15 8 1,15 * 1"), false);
        assert!(timer.contains(
            "OnCalendar=*-*-01,15 08:15:00\nOnCalendar=Mon *-*-* 08:15:00\nPersistent=false\n"
        ));
        let (timer, _) = systemd("x", "x", Path::new("/t"), &cron("0 0 * 2 *"), false);
        assert!(timer.contains("OnCalendar=*-02-* 00:00:00\n"));
    }

    #[test]
    fn writes_a_task_scheduler_task() {
        let path = Path::new(r"C:\Program Files\TermF\termf.exe");
        let task = task_xml(
            "Daily <review>",
            path,
            "daily",
            &cron("0 9,17 * * 1-5"),
            true,
        );
        assert!(
            task.starts_with("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n<Task version=\"1.2\"")
        );
        assert!(
            task.contains("<Description>TermF scheduled task: Daily &lt;review&gt;</Description>")
        );
        assert_eq!(task.matches("<CalendarTrigger>").count(), 2);
        assert!(task.contains("<StartBoundary>2026-01-01T09:00:00</StartBoundary>"));
        assert!(task.contains("<StartBoundary>2026-01-01T17:00:00</StartBoundary>"));
        assert!(task.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        assert!(task.contains(
            "<Command>C:\\Program Files\\TermF\\termf.exe</Command>\n      <Arguments>schedule run daily --scheduled</Arguments>"
        ));
        assert!(!task.contains("<Repetition>"));

        let task = task_xml("x", path, "x", &cron("*/10 * * * *"), false);
        assert_eq!(task.matches("<CalendarTrigger>").count(), 1);
        assert!(task.contains("<Interval>PT10M</Interval>"));
        assert!(task.contains("<StartBoundary>2026-01-01T00:00:00</StartBoundary>"));
        let task = task_xml("x", path, "x", &cron("5,20,50 * * * *"), false);
        assert!(
            task.contains("<Interval>PT15M</Interval>"),
            "lands on :05, :20 and :50"
        );
        assert!(task.contains("<StartBoundary>2026-01-01T00:05:00</StartBoundary>"));

        let bytes =
            &system(Platform::TaskScheduler).files("x", "x", path, &cron("0 9 * * *"), true)[0].1;
        assert_eq!(
            &bytes[..4],
            &[0xFF, 0xFE, b'<', 0],
            "UTF-16 with its byte order mark"
        );
    }
}
