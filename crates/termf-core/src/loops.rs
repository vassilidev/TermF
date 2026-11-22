//! The `/loop`s and scheduled tasks of a Claude Code session (docs/en/scheduled-tasks), read from its hooks. They
//! live in the session's memory only: TermF sees the tools that create and cancel them (`CronCreate`,
//! `CronDelete`, `ScheduleWakeup`, `Monitor`), a typed `/loop`, and the `session_crons` and `background_tasks`
//! every `Stop` lists, which have the last word.

use serde::{Deserialize, Serialize};
use serde_json::Value;

const DAY_MS: u64 = 24 * 3600 * 1000;
/// Recurring tasks end 7 days after their creation, unless `CronCreate` says otherwise.
const RECURRING_MAX_AGE_MS: u64 = 7 * DAY_MS;
/// A Monitor watch's deadline: 5 minutes by default, at most 30.
const MONITOR_DEFAULT_MS: u64 = 5 * 60 * 1000;
const MONITOR_MAX_MS: u64 = 30 * 60 * 1000;
/// Kept of a step's scheduled prompt, in a team run's report.
const PROMPT_CHARS: usize = 300;
/// The prompts `/loop` schedules for its built-in maintenance prompt (or `loop.md`), with an interval and without.
const DEFAULT_PROMPT: &str = "<<autonomous-loop>>";
const DEFAULT_DYNAMIC: &str = "<<autonomous-loop-dynamic>>";
/// Ids of what Claude Code has not named yet: a typed `/loop` before its first tool call, and the next turn of a
/// self-paced loop until a `Stop` lists it (real ids have 8 characters).
const TYPED: &str = "loop";
const ARMED: &str = "wakeup";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `CronCreate`, which a `/loop` with an interval calls.
    Cron,
    /// A `/loop` that paces itself (`ScheduleWakeup`).
    Wakeup,
    /// A `Monitor` watch, which ends at its deadline.
    Monitor,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Loop {
    pub id: String,
    pub kind: Kind,
    /// Five cron fields, local time.
    pub cron: Option<String>,
    /// Seconds between two runs when no cron says it: the interval typed after `/loop`, or the last delay a
    /// self-paced loop chose.
    pub every: Option<u64>,
    pub recurring: bool,
    pub prompt: String,
    /// Typed as `/loop`, rather than asked in words.
    pub from_loop: bool,
    /// Epoch milliseconds.
    pub created: u64,
    pub next_run: Option<u64>,
    pub ends: Option<u64>,
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct Loops(Vec<Loop>);

impl Loops {
    /// Reads a hook at `now` (epoch milliseconds); returns whether the list changed.
    pub fn hook(&mut self, event: &str, payload: &Value, now: u64) -> bool {
        let before = self.0.clone();
        match event {
            "UserPromptSubmit" => {
                if let Some((every, prompt)) = typed_loop(str_of(&payload["prompt"])) {
                    self.0.retain(|l| l.id != TYPED);
                    self.0.push(Loop {
                        id: TYPED.into(),
                        kind: if every.is_some() {
                            Kind::Cron
                        } else {
                            Kind::Wakeup
                        },
                        every,
                        recurring: true,
                        prompt: prompt_of(prompt),
                        from_loop: true,
                        ..fresh(now)
                    });
                }
            }
            "PostToolUse" => self.tool(payload, now),
            "Stop" => self.stop(payload, now),
            "SessionEnd" => self.0.clear(),
            _ => {}
        }
        self.0 != before
    }

    /// The loops still on at `now`.
    pub fn active(&self, now: u64) -> Vec<Loop> {
        self.0
            .iter()
            .filter(|l| l.ends.is_none_or(|end| end > now))
            .cloned()
            .collect()
    }

    pub fn clear(&mut self) -> bool {
        let had = !self.0.is_empty();
        self.0.clear();
        had
    }

    fn take(&mut self, id: &str) -> Option<Loop> {
        let i = self.0.iter().position(|l| l.id == id)?;
        Some(self.0.remove(i))
    }

    fn tool(&mut self, payload: &Value, now: u64) {
        let (input, output) = (&payload["tool_input"], &payload["tool_response"]);
        match str_of(&payload["tool_name"]) {
            "CronCreate" => {
                let Some(id) = output["id"].as_str() else {
                    return;
                };
                let typed = self.take(TYPED);
                self.take(id);
                let recurring = output["recurring"]
                    .as_bool()
                    .or(input["recurring"].as_bool())
                    .unwrap_or(true);
                let ends = match output["maxAgeMs"].as_u64() {
                    Some(0) => None,
                    Some(age) => Some(now.saturating_add(age)),
                    None => Some(now.saturating_add(RECURRING_MAX_AGE_MS)),
                };
                self.0.push(Loop {
                    id: id.into(),
                    cron: input["cron"].as_str().map(Into::into),
                    every: typed.as_ref().and_then(|t| t.every),
                    recurring,
                    prompt: prompt_of(str_of(&input["prompt"])),
                    from_loop: typed.is_some() || input["prompt"] == DEFAULT_PROMPT,
                    next_run: output["nextFireAt"].as_u64(),
                    ends: ends.filter(|_| recurring),
                    ..fresh(now)
                });
            }
            "CronDelete" => {
                self.take(str_of(&input["id"]));
                if output["endedLoopWakeup"] == true {
                    self.0.retain(|l| l.kind != Kind::Wakeup);
                }
            }
            "ScheduleWakeup" => {
                let typed = self.take(TYPED);
                let started = self
                    .0
                    .iter()
                    .chain(&typed)
                    .filter(|l| l.kind == Kind::Wakeup)
                    .map(|l| l.created)
                    .min();
                self.0.retain(|l| l.kind != Kind::Wakeup);
                if input["stop"] == true || output["stopped"] == true {
                    return;
                }
                let delay = output["clampedDelaySeconds"]
                    .as_u64()
                    .or(input["delaySeconds"].as_u64());
                let created = started.unwrap_or(now);
                self.0.push(Loop {
                    id: ARMED.into(),
                    kind: Kind::Wakeup,
                    every: delay,
                    recurring: true,
                    prompt: prompt_of(str_of(&input["prompt"])),
                    from_loop: true,
                    created,
                    next_run: output["scheduledFor"]
                        .as_u64()
                        .or(delay.map(|d| now.saturating_add(d.saturating_mul(1000)))),
                    ends: Some(created.saturating_add(RECURRING_MAX_AGE_MS)),
                    ..fresh(now)
                });
            }
            "Monitor" => {
                let id = [
                    &output["taskId"],
                    &output["task_id"],
                    &payload["tool_use_id"],
                ]
                .into_iter()
                .find_map(Value::as_str)
                .unwrap_or("monitor");
                let what = [
                    &input["description"],
                    &input["command"],
                    &input["ws"]["url"],
                ]
                .into_iter()
                .find_map(Value::as_str)
                .unwrap_or_default();
                let timeout = input["timeout_ms"]
                    .as_u64()
                    .unwrap_or(MONITOR_DEFAULT_MS)
                    .min(MONITOR_MAX_MS);
                self.take(id);
                self.0.push(Loop {
                    id: id.into(),
                    kind: Kind::Monitor,
                    prompt: prompt_of(what),
                    ends: Some(now.saturating_add(timeout)),
                    ..fresh(now)
                });
            }
            _ => {}
        }
    }

    /// `session_crons` lists every cron and wakeup still scheduled; versions without it keep what the tools said.
    fn stop(&mut self, payload: &Value, now: u64) {
        let Some(crons) = payload["session_crons"].as_array() else {
            return;
        };
        let mut known = std::mem::take(&mut self.0);
        let watching = payload["background_tasks"]
            .as_array()
            .is_none_or(|tasks| tasks.iter().any(|t| t["type"] == "monitor"));
        // shortcut: a monitor stays until its deadline while any monitor runs; match them by task id once the
        // hooks give `Monitor`'s.
        self.0
            .extend(known.extract_if(.., |l| l.kind == Kind::Monitor && watching));
        for cron in crons {
            let id = str_of(&cron["id"]);
            let recurring = cron["recurring"].as_bool().unwrap_or(true);
            // Capped at 1000 characters there: the tools' own prompt is kept.
            let prompt = str_of(&cron["prompt"]);
            let seen = known.iter().position(|l| l.id == id).or_else(|| {
                (!recurring)
                    .then(|| known.iter().position(|l| l.id == ARMED))
                    .flatten()
            });
            let mut l = match seen {
                Some(i) => known.remove(i),
                None if recurring => Loop {
                    recurring,
                    prompt: prompt_of(prompt),
                    from_loop: prompt == DEFAULT_PROMPT,
                    ends: Some(now.saturating_add(RECURRING_MAX_AGE_MS)),
                    ..fresh(now)
                },
                // Nothing but its prompt tells a self-paced loop's next turn from a one-time reminder.
                None if prompt == DEFAULT_DYNAMIC => Loop {
                    kind: Kind::Wakeup,
                    recurring: true,
                    from_loop: true,
                    ends: Some(now.saturating_add(RECURRING_MAX_AGE_MS)),
                    ..fresh(now)
                },
                None => continue,
            };
            l.id = id.into();
            if l.kind == Kind::Cron {
                l.cron = cron["schedule"].as_str().map(Into::into).or(l.cron);
                l.recurring = recurring;
            }
            self.0.push(l);
        }
    }
}

/// The time the hooks are read at, in epoch milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn fresh(now: u64) -> Loop {
    Loop {
        id: String::new(),
        kind: Kind::Cron,
        cron: None,
        every: None,
        recurring: false,
        prompt: String::new(),
        from_loop: false,
        created: now,
        next_run: None,
        ends: None,
    }
}

fn str_of(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}

/// The prompt as Claude Code runs it, empty for `/loop`'s built-in one.
fn prompt_of(text: &str) -> String {
    let text = text.trim();
    if [DEFAULT_PROMPT, DEFAULT_DYNAMIC].contains(&text) {
        String::new()
    } else {
        text.into()
    }
}

fn cap(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(PROMPT_CHARS) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.into(),
    }
}

/// `/loop [interval] [prompt]`: the interval in seconds when it leads, and the prompt.
// shortcut: a trailing clause such as "every 2 hours" reads as no interval until `CronCreate` gives the cron.
fn typed_loop(prompt: &str) -> Option<(Option<u64>, &str)> {
    let rest = prompt.trim_start().strip_prefix("/loop")?;
    if rest.starts_with(|c: char| !c.is_whitespace()) {
        return None;
    }
    let rest = rest.trim();
    let (first, after) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    Some(match interval(first) {
        Some(secs) => (Some(secs), after.trim()),
        None => (None, rest),
    })
}

/// `30s`, `5m`, `2h`, `1d`.
fn interval(token: &str) -> Option<u64> {
    let (n, unit) = token.split_at(token.find(|c: char| !c.is_ascii_digit())?);
    let n: u64 = n.parse().ok()?;
    let secs = match unit {
        "s" => n,
        "m" => n.checked_mul(60)?,
        "h" => n.checked_mul(3600)?,
        "d" => n.checked_mul(86400)?,
        _ => return None,
    };
    (secs > 0).then_some(secs)
}

/// What a headless agent's tool call schedules in its own session, if anything.
pub fn scheduled(tool: &str, input: &Value) -> Option<String> {
    match tool {
        "CronCreate" => Some(format!(
            "{} · {}",
            input["cron"].as_str()?,
            cap(str_of(&input["prompt"]))
        )),
        "ScheduleWakeup" if input["stop"] != true => {
            Some(format!("/loop · {}", cap(str_of(&input["prompt"]))))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const T0: u64 = 1_800_000_000_000;

    fn tool(name: &str, input: Value, output: Value) -> Value {
        json!({"tool_name": name, "tool_input": input, "tool_response": output, "tool_use_id": "toolu_1"})
    }

    #[test]
    fn a_typed_loop_shows_at_once_then_takes_the_cron_claude_creates() {
        let mut loops = Loops::default();
        let typed = json!({"prompt": "/loop 5m check the deploy"});
        assert!(loops.hook("UserPromptSubmit", &typed, T0));
        let pending = &loops.active(T0)[0];
        assert_eq!(
            (pending.kind, pending.every, pending.prompt.as_str()),
            (Kind::Cron, Some(300), "check the deploy")
        );

        let created = tool(
            "CronCreate",
            json!({"cron": "*/5 * * * *", "prompt": "check the deploy", "recurring": true}),
            json!({"id": "ab12cd34", "humanSchedule": "every 5 minutes", "recurring": true, "nextFireAt": T0 + 285_000}),
        );
        assert!(loops.hook("PostToolUse", &created, T0 + 1000));
        let [l] = &loops.active(T0 + 1000)[..] else {
            panic!("{loops:?}")
        };
        assert_eq!(l.id, "ab12cd34");
        assert_eq!(l.cron.as_deref(), Some("*/5 * * * *"));
        assert!(l.from_loop && l.recurring);
        assert_eq!(l.next_run, Some(T0 + 285_000));
        assert_eq!(l.ends, Some(T0 + 1000 + RECURRING_MAX_AGE_MS));
        assert!(
            loops.active(T0 + 1001 + RECURRING_MAX_AGE_MS).is_empty(),
            "expires after 7 days"
        );

        let deleted = tool(
            "CronDelete",
            json!({"id": "ab12cd34"}),
            json!({"id": "ab12cd34"}),
        );
        assert!(loops.hook("PostToolUse", &deleted, T0 + 2000));
        assert!(loops.active(T0 + 2000).is_empty());
    }

    #[test]
    fn only_a_loop_command_counts() {
        let mut loops = Loops::default();
        for prompt in ["/loops", "please /loop 5m x", "/loopy 5m"] {
            assert!(!loops.hook("UserPromptSubmit", &json!({ "prompt": prompt }), T0));
        }
        assert!(loops.hook("UserPromptSubmit", &json!({"prompt": "/loop"}), T0));
        assert_eq!(loops.active(T0)[0].kind, Kind::Wakeup);
        assert_eq!(
            typed_loop("/loop 90s /review-pr 12"),
            Some((Some(90), "/review-pr 12"))
        );
        assert_eq!(
            typed_loop("/loop check CI every 2 hours"),
            Some((None, "check CI every 2 hours"))
        );
        assert_eq!(interval("0m"), None);
        assert_eq!(interval("10x"), None);
    }

    #[test]
    fn a_self_paced_loop_follows_its_wakeups_until_it_stops() {
        let mut loops = Loops::default();
        loops.hook(
            "UserPromptSubmit",
            &json!({"prompt": "/loop babysit the PR"}),
            T0,
        );
        let wake = |delay: u64, at: u64| {
            tool(
                "ScheduleWakeup",
                json!({"delaySeconds": delay, "reason": "CI running", "prompt": "babysit the PR"}),
                json!({"scheduledFor": at + delay * 1000, "clampedDelaySeconds": delay}),
            )
        };
        loops.hook("PostToolUse", &wake(600, T0 + 5000), T0 + 5000);
        // The Stop of that turn names it.
        let stop = json!({"session_crons": [{"id": "w1", "schedule": "35 10 22 11 *", "recurring": false, "prompt": "babysit the PR"}], "background_tasks": []});
        loops.hook("Stop", &stop, T0 + 6000);
        let [l] = &loops.active(T0 + 6000)[..] else {
            panic!("{loops:?}")
        };
        assert_eq!(
            (l.id.as_str(), l.kind, l.every),
            ("w1", Kind::Wakeup, Some(600))
        );
        assert_eq!(
            (l.created, l.next_run, l.cron.as_deref()),
            (T0, Some(T0 + 605_000), None)
        );
        assert!(l.recurring && l.from_loop);

        loops.hook("PostToolUse", &wake(1200, T0 + 700_000), T0 + 700_000);
        let l = &loops.active(T0 + 700_000)[0];
        assert_eq!(
            (l.created, l.every),
            (T0, Some(1200)),
            "the same loop goes on"
        );

        let stopped = tool(
            "ScheduleWakeup",
            json!({"stop": true}),
            json!({"stopped": true}),
        );
        assert!(loops.hook("PostToolUse", &stopped, T0 + 800_000));
        assert!(loops.active(T0 + 800_000).is_empty());
    }

    #[test]
    fn each_stop_has_the_last_word_and_the_session_end_clears_all() {
        let mut loops = Loops::default();
        let created = tool(
            "CronCreate",
            json!({"cron": "0 9 * * *", "prompt": "standup notes"}),
            json!({"id": "aaaa1111", "recurring": true}),
        );
        loops.hook("PostToolUse", &created, T0);
        // Restored by `--resume`, or fired once and gone: only the Stop tells.
        let stop = json!({"session_crons": [{"id": "bbbb2222", "schedule": "*/10 * * * *", "recurring": true, "prompt": "poll"}]});
        assert!(loops.hook("Stop", &stop, T0 + 1000));
        let [l] = &loops.active(T0 + 1000)[..] else {
            panic!("{loops:?}")
        };
        assert_eq!(
            (l.id.as_str(), l.cron.as_deref(), l.prompt.as_str()),
            ("bbbb2222", Some("*/10 * * * *"), "poll")
        );
        assert!(!l.from_loop);

        assert!(!loops.hook(
            "Stop",
            &json!({"last_assistant_message": "old version"}),
            T0 + 2000
        ));
        assert!(loops.hook(
            "SessionEnd",
            &json!({"reason": "prompt_input_exit"}),
            T0 + 3000
        ));
        assert!(loops.active(T0 + 3000).is_empty());
    }

    #[test]
    fn a_failed_call_or_a_one_shot_adds_no_loop_that_outlives_it() {
        let mut loops = Loops::default();
        let failed = json!({"tool_name": "CronCreate", "tool_input": {"cron": "bad"}, "tool_response": "Invalid cron expression"});
        assert!(!loops.hook("PostToolUse", &failed, T0));
        let once = tool(
            "CronCreate",
            json!({"cron": "45 15 22 11 *", "prompt": "push the branch", "recurring": false}),
            json!({"id": "cccc3333", "recurring": false}),
        );
        loops.hook("PostToolUse", &once, T0);
        assert_eq!(loops.active(T0)[0].ends, None);
        loops.hook("Stop", &json!({"session_crons": []}), T0 + 1000);
        assert!(loops.active(T0 + 1000).is_empty(), "fired and gone");
    }

    #[test]
    fn a_monitor_ends_at_its_deadline_or_when_none_runs() {
        let mut loops = Loops::default();
        let watch = tool(
            "Monitor",
            json!({"command": "tail -f log", "description": "watch the log", "timeout_ms": 3_600_000}),
            json!({"taskId": "bg1"}),
        );
        loops.hook("PostToolUse", &watch, T0);
        let l = &loops.active(T0)[0];
        assert_eq!(
            (l.kind, l.prompt.as_str(), l.ends),
            (Kind::Monitor, "watch the log", Some(T0 + MONITOR_MAX_MS))
        );
        let busy =
            json!({"session_crons": [], "background_tasks": [{"id": "bg1", "type": "monitor"}]});
        loops.hook("Stop", &busy, T0 + 1000);
        assert_eq!(loops.active(T0 + 1000).len(), 1);
        assert!(loops.active(T0 + MONITOR_MAX_MS).is_empty());
        let idle = json!({"session_crons": [], "background_tasks": []});
        loops.hook("Stop", &idle, T0 + 2000);
        assert!(loops.active(T0 + 2000).is_empty());
    }

    #[test]
    fn what_a_headless_agent_schedules() {
        assert_eq!(
            scheduled(
                "CronCreate",
                &json!({"cron": "*/5 * * * *", "prompt": "check"})
            )
            .as_deref(),
            Some("*/5 * * * * · check")
        );
        assert!(
            scheduled(
                "ScheduleWakeup",
                &json!({"prompt": "go on", "delaySeconds": 60})
            )
            .is_some()
        );
        assert!(scheduled("ScheduleWakeup", &json!({"stop": true})).is_none());
        assert!(scheduled("Bash", &json!({"command": "ls"})).is_none());
    }

    #[test]
    fn the_prompt_is_kept_whole_and_the_built_in_one_is_empty() {
        let mut loops = Loops::default();
        let long = "check the deploy ".repeat(100);
        let created = tool(
            "CronCreate",
            json!({"cron": "*/5 * * * *", "prompt": long}),
            json!({"id": "aaaa1111", "recurring": true}),
        );
        loops.hook("PostToolUse", &created, T0);
        let clipped = format!("{}… [+700 chars]", &long[..1000]);
        let stop = json!({"session_crons": [{"id": "aaaa1111", "schedule": "*/5 * * * *", "recurring": true, "prompt": clipped}]});
        loops.hook("Stop", &stop, T0 + 1000);
        assert_eq!(loops.active(T0 + 1000)[0].prompt, long.trim());

        let mut loops = Loops::default();
        loops.hook("UserPromptSubmit", &json!({"prompt": "/loop 10m"}), T0);
        let bare = tool(
            "CronCreate",
            json!({"cron": "*/10 * * * *", "prompt": DEFAULT_PROMPT}),
            json!({"id": "bbbb2222", "recurring": true}),
        );
        loops.hook("PostToolUse", &bare, T0);
        let l = &loops.active(T0)[0];
        assert_eq!((l.prompt.as_str(), l.from_loop), ("", true));
    }

    #[test]
    fn a_huge_interval_or_delay_never_overflows() {
        let mut loops = Loops::default();
        let typed = json!({"prompt": "/loop 999999999999999999d check"});
        loops.hook("UserPromptSubmit", &typed, T0);
        assert_eq!(loops.active(T0)[0].every, None);
        let wake = tool(
            "ScheduleWakeup",
            json!({"delaySeconds": u64::MAX, "prompt": "go on"}),
            json!({}),
        );
        loops.hook("PostToolUse", &wake, u64::MAX - 1);
        let created = tool(
            "CronCreate",
            json!({"cron": "* * * * *", "prompt": "x"}),
            json!({"id": "cccc3333", "recurring": true, "maxAgeMs": u64::MAX}),
        );
        loops.hook("PostToolUse", &created, T0);
        let watch = tool("Monitor", json!({"command": "tail -f log"}), json!({}));
        loops.hook("PostToolUse", &watch, u64::MAX);
        assert_eq!(loops.0.len(), 3);
    }

    #[test]
    fn after_a_resume_only_a_marked_wakeup_comes_back_as_a_loop() {
        let mut loops = Loops::default();
        let stop = json!({"session_crons": [
            {"id": "dddd4444", "schedule": "12 10 23 11 *", "recurring": false, "prompt": "babysit the PR"},
            {"id": "eeee5555", "schedule": "40 10 23 11 *", "recurring": false, "prompt": DEFAULT_DYNAMIC},
            {"id": "ffff6666", "schedule": "*/15 * * * *", "recurring": true, "prompt": DEFAULT_PROMPT},
        ]});
        loops.hook("Stop", &stop, T0);
        let shown: Vec<_> = loops
            .active(T0)
            .into_iter()
            .map(|l| (l.id, l.kind, l.recurring, l.from_loop, l.prompt))
            .collect();
        assert_eq!(
            shown,
            [
                ("eeee5555".into(), Kind::Wakeup, true, true, String::new()),
                ("ffff6666".into(), Kind::Cron, true, true, String::new()),
            ]
        );
    }
}
