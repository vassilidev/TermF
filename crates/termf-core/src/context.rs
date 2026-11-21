//! Context gauge and automatic context reset of Claude Code sessions (ARCHITECTURE §6.5).
//!
//! The gauge comes from the `context_window` Claude Code gives its status line, which TermF sets for the
//! sessions it starts. Past the threshold, at the end of a turn, the Stop hook asks the agent for a
//! handoff, so the turn goes on; its next reply is saved as the handoff, TermF types `/clear`, the new
//! conversation's SessionStart hook hands it the handoff, and TermF types the prompt that resumes the
//! work. `Reset` runs that sequence without any I/O, so it is tested alone.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::ipc::Status;

/// What the UI shows for a session's context.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Gauge {
    /// Share of the window in use, `None` until the conversation's first answer.
    pub percent: Option<u8>,
    pub tokens: Option<u64>,
    pub window: Option<u64>,
    pub phase: Phase,
    /// Why the last reset stopped short, until the next one starts.
    pub failure: Option<Failure>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Idle,
    /// Asked by hand while the agent works: starts at the end of the turn.
    Queued,
    /// The agent writes its handoff.
    Handoff,
    /// The handoff is saved: `/clear` is typed once the agent is back at its prompt.
    Clearing,
    /// The context is cleared: the prompt that resumes the work is typed next.
    Resuming,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// You typed in the session.
    Typed,
    /// The agent asked for an approval or went on with something else.
    Busy,
    /// The turn ended on an API error.
    Interrupted,
    EmptyHandoff,
    SaveFailed,
    /// A step did not happen in time.
    Timeout,
}

/// The settings a turn's end is checked against.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub on: bool,
    pub reset_at: u8,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    Nothing,
    /// Ask the agent for its handoff.
    AskHandoff,
    /// Type `/clear` once the agent is back at its prompt.
    Clear,
}

/// When the automatic reset may start again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Arm {
    #[default]
    Armed,
    /// After a reset: once a turn ends below the threshold, so a context a reset cannot shrink never loops.
    BelowThreshold,
    /// After a failure: that, or your next prompt.
    NextPrompt,
}

#[derive(Debug, Default)]
pub struct Reset {
    pub gauge: Gauge,
    arm: Arm,
    /// You typed since your last prompt: a draft may sit in the prompt, and `/clear` would join it.
    typed: bool,
    handoff: String,
    /// The resume prompt was typed.
    sent: bool,
    /// Bumped by every step, so a timer set for an earlier step does nothing.
    step: u64,
}

impl Reset {
    pub fn step(&self) -> u64 {
        self.step
    }

    /// A sequence is under way.
    pub fn active(&self) -> bool {
        matches!(
            self.gauge.phase,
            Phase::Handoff | Phase::Clearing | Phase::Resuming
        )
    }

    /// Reads the status line's `context_window`; its fields are null until the first answer.
    pub fn measure(&mut self, context_window: &Value) {
        let percent = context_window["used_percentage"].as_f64();
        self.gauge.percent = percent.map(|p| p.round().clamp(0.0, 100.0) as u8);
        self.gauge.tokens = context_window["total_input_tokens"].as_u64();
        self.gauge.window = context_window["context_window_size"].as_u64();
    }

    /// A conversation starts. Returns the handoff to give it when it is the one the reset cleared for.
    pub fn session_start(&mut self, source: &str) -> Option<String> {
        if source != "clear" {
            return None;
        }
        self.gauge = Gauge {
            phase: self.gauge.phase,
            failure: self.gauge.failure,
            ..Gauge::default()
        };
        if self.gauge.phase != Phase::Clearing {
            return None;
        }
        self.go(Phase::Resuming);
        Some(self.handoff.clone())
    }

    /// The turn ended. `busy`: background tasks still run. `reply`: the agent's last message, saved
    /// with `save` when it is the handoff.
    pub fn stop(
        &mut self,
        limits: Limits,
        busy: bool,
        reply: &str,
        save: impl FnOnce(&str) -> Result<()>,
    ) -> Next {
        let over = self.gauge.percent.map(|p| p >= limits.reset_at);
        if over == Some(false) {
            self.arm = Arm::Armed;
        }
        match self.gauge.phase {
            Phase::Handoff => {
                let reply = reply.trim();
                if reply.is_empty() {
                    self.fail(Failure::EmptyHandoff);
                } else if busy {
                    self.fail(Failure::Busy);
                } else if save(reply).is_err() {
                    self.fail(Failure::SaveFailed);
                } else {
                    self.handoff = reply.to_owned();
                    self.go(Phase::Clearing);
                    return Next::Clear;
                }
                Next::Nothing
            }
            Phase::Queued if self.typed => {
                self.fail(Failure::Typed);
                Next::Nothing
            }
            Phase::Queued if !busy => self.start(),
            Phase::Idle
                if limits.on
                    && over == Some(true)
                    && self.arm == Arm::Armed
                    && !self.typed
                    && !busy =>
            {
                self.start()
            }
            _ => Next::Nothing,
        }
    }

    pub fn prompt_submitted(&mut self) {
        self.typed = false;
        match self.gauge.phase {
            Phase::Resuming if self.sent => self.go(Phase::Idle),
            Phase::Clearing | Phase::Resuming => self.fail(Failure::Busy),
            Phase::Idle if self.arm == Arm::NextPrompt => self.arm = Arm::Armed,
            _ => {}
        }
    }

    /// The agent's state changed: a question, or work once the handoff is written, stops the sequence.
    pub fn activity(&mut self, status: Status) {
        let stops = match self.gauge.phase {
            Phase::Handoff => status == Status::Waiting,
            Phase::Clearing | Phase::Resuming => {
                matches!(status, Status::Working | Status::Waiting)
            }
            _ => false,
        };
        if stops {
            self.fail(Failure::Busy);
        }
    }

    /// You typed in the session, other than to answer a question.
    pub fn typed(&mut self) {
        self.typed = true;
        if self.active() {
            self.fail(Failure::Typed);
        }
    }

    /// The turn ended on an API error.
    pub fn interrupted(&mut self) {
        if self.active() {
            self.fail(Failure::Interrupted);
        }
    }

    /// Asked by hand. `idle`: the agent waits at its prompt, so TermF types the request at once (true);
    /// otherwise it waits for the end of the turn.
    pub fn request(&mut self, idle: bool) -> bool {
        if self.active() || self.gauge.phase == Phase::Queued {
            return false;
        }
        if idle {
            self.start();
            return true;
        }
        self.gauge.failure = None;
        self.go(Phase::Queued);
        false
    }

    /// A step's time ran out; true when it was still on.
    pub fn timeout(&mut self, step: u64) -> bool {
        let late = step == self.step && self.active();
        if late {
            self.fail(Failure::Timeout);
        }
        late
    }

    /// Whether `/clear` is still to be typed for the step that scheduled it.
    pub fn clear_due(&self, step: u64) -> bool {
        step == self.step && self.gauge.phase == Phase::Clearing
    }

    /// Whether the resume prompt is still to be typed for the step that scheduled it; it counts as sent.
    pub fn resume_due(&mut self, step: u64) -> bool {
        let due = step == self.step && self.gauge.phase == Phase::Resuming && !self.sent;
        self.sent |= due;
        due
    }

    fn start(&mut self) -> Next {
        self.arm = Arm::BelowThreshold;
        self.gauge.failure = None;
        self.go(Phase::Handoff);
        Next::AskHandoff
    }

    fn go(&mut self, phase: Phase) {
        self.gauge.phase = phase;
        self.sent = false;
        self.step += 1;
    }

    fn fail(&mut self, failure: Failure) {
        self.gauge.failure = Some(failure);
        self.arm = Arm::NextPrompt;
        self.go(Phase::Idle);
    }
}

/// What the agent is asked to write; one line, as it may be typed in the prompt.
pub fn handoff_request(percent: Option<u8>) -> String {
    let used = percent.map(|p| format!(" ({p}% of the model's window used)"));
    format!(
        "TermF, which runs this session, is about to clear its context{} and go on in a fresh \
         conversation. Your next reply is saved as the handoff that conversation starts from, so write \
         it now and do nothing else: the goal, where the work stands, the decisions made and why, what \
         remains to do (next step first) and the files involved, in under 6000 characters. Reply with \
         the handoff only, without calling tools.",
        used.unwrap_or_default()
    )
}

/// Output of the Stop hook that keeps the turn going for the handoff.
pub fn stop_output(request: &str) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "Stop",
            "additionalContext": request,
        }
    })
}

/// Output of the SessionStart hook that gives the cleared conversation its handoff.
pub fn session_start_output(handoff: &str, file: &Path) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": format!(
                "Handoff written by the agent of this session just before TermF cleared its context \
                 (also saved at {}):\n\n{handoff}",
                file.display()
            ),
        }
    })
}

/// Typed in the prompt once the context is cleared; one line.
pub fn resume_prompt(file: &Path) -> String {
    format!(
        "Resume the work from the handoff given at the start of this conversation (also saved at {}).",
        file.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;

    #[test]
    fn reads_the_status_line_context() {
        let mut reset = Reset::default();
        reset.measure(&json!({
            "total_input_tokens": 150_000,
            "context_window_size": 200_000,
            "used_percentage": 74.6,
        }));
        assert_eq!(reset.gauge.percent, Some(75));
        assert_eq!(reset.gauge.tokens, Some(150_000));
        assert_eq!(reset.gauge.window, Some(200_000));

        reset.measure(&json!({"used_percentage": null, "context_window_size": 1_000_000}));
        assert_eq!(reset.gauge.percent, None, "before the first answer");
        reset.measure(&Value::Null);
        assert_eq!(reset.gauge, Gauge::default(), "older Claude Code");

        reset.measure(&json!({"used_percentage": 40}));
        assert_eq!(reset.session_start("compact"), None);
        assert_eq!(reset.gauge.percent, Some(40));
        assert_eq!(reset.session_start("clear"), None, "cleared by hand");
        assert_eq!(reset.gauge.percent, None, "a new conversation");
    }

    const ON: Limits = Limits {
        on: true,
        reset_at: 70,
    };

    fn at(reset: &mut Reset, percent: u64) {
        reset.measure(&json!({ "used_percentage": percent }));
    }

    fn ok(_: &str) -> Result<()> {
        Ok(())
    }

    #[test]
    fn runs_the_whole_sequence_then_waits_below_the_threshold() {
        let mut reset = Reset::default();
        at(&mut reset, 40);
        assert_eq!(reset.stop(ON, false, "done", ok), Next::Nothing);

        at(&mut reset, 72);
        assert_eq!(reset.stop(ON, false, "done", ok), Next::AskHandoff);
        assert_eq!(reset.gauge.phase, Phase::Handoff);
        reset.activity(Status::Working);
        assert_eq!(reset.gauge.phase, Phase::Handoff, "it may read files");

        let mut saved = String::new();
        let next = reset.stop(ON, false, "  Goal: X. Next: Y.  ", |text| {
            saved = text.to_owned();
            Ok(())
        });
        assert_eq!(next, Next::Clear);
        assert_eq!(saved, "Goal: X. Next: Y.");
        assert!(reset.clear_due(reset.step()));
        assert!(!reset.clear_due(reset.step() - 1), "an earlier timer");

        assert_eq!(
            reset.session_start("clear").as_deref(),
            Some("Goal: X. Next: Y.")
        );
        assert_eq!(reset.gauge.phase, Phase::Resuming);
        assert_eq!(reset.gauge.percent, None);
        let step = reset.step();
        assert!(reset.resume_due(step));
        assert!(!reset.resume_due(step), "typed once");
        reset.prompt_submitted();
        reset.activity(Status::Working);
        assert_eq!(reset.gauge.phase, Phase::Idle);
        assert_eq!(reset.gauge.failure, None);
        assert!(!reset.timeout(step), "done before its timer");

        // Still over the threshold after the reset: no second sequence.
        at(&mut reset, 75);
        assert_eq!(reset.stop(ON, false, "done", ok), Next::Nothing);
        at(&mut reset, 10);
        assert_eq!(reset.stop(ON, false, "done", ok), Next::Nothing);
        at(&mut reset, 71);
        assert_eq!(
            reset.stop(ON, false, "done", ok),
            Next::AskHandoff,
            "re-armed"
        );
    }

    #[test]
    fn typing_stops_or_postpones_it() {
        let mut reset = Reset::default();
        at(&mut reset, 80);
        reset.stop(ON, false, "done", ok);
        reset.typed();
        assert_eq!(reset.gauge.phase, Phase::Idle);
        assert_eq!(reset.gauge.failure, Some(Failure::Typed));
        // The end of the handoff turn: nothing saved, nothing started while a draft may sit in the prompt.
        let next = reset.stop(ON, false, "handoff", |_| panic!("not saved"));
        assert_eq!(next, Next::Nothing);
        reset.prompt_submitted();
        assert_eq!(
            reset.stop(ON, false, "done", ok),
            Next::AskHandoff,
            "next turn"
        );

        let mut reset = Reset::default();
        at(&mut reset, 80);
        reset.typed();
        assert_eq!(reset.stop(ON, false, "done", ok), Next::Nothing, "a draft");
        assert_eq!(reset.gauge.failure, None);

        for typed_in in [Phase::Clearing, Phase::Resuming] {
            let mut reset = Reset::default();
            at(&mut reset, 80);
            reset.stop(ON, false, "done", ok);
            reset.stop(ON, false, "handoff", ok);
            if typed_in == Phase::Resuming {
                reset.session_start("clear");
            }
            let step = reset.step();
            reset.typed();
            assert_eq!(reset.gauge.failure, Some(Failure::Typed));
            assert!(!reset.clear_due(step) && !reset.resume_due(step));
        }
    }

    #[test]
    fn a_failed_handoff_never_clears_nor_loops() {
        let mut reset = Reset::default();
        at(&mut reset, 80);
        reset.stop(ON, false, "done", ok);
        let next = reset.stop(ON, false, "handoff", |_| bail!("disk full"));
        assert_eq!(next, Next::Nothing);
        assert_eq!(reset.gauge.failure, Some(Failure::SaveFailed));
        assert!(!reset.clear_due(reset.step()));
        assert_eq!(
            reset.stop(ON, false, "done", ok),
            Next::Nothing,
            "not again on its own"
        );
        reset.prompt_submitted();
        assert_eq!(
            reset.stop(ON, false, "done", ok),
            Next::AskHandoff,
            "after your next prompt"
        );

        let mut reset = Reset::default();
        at(&mut reset, 80);
        reset.stop(ON, false, "done", ok);
        assert_eq!(reset.stop(ON, false, " \n", ok), Next::Nothing);
        assert_eq!(reset.gauge.failure, Some(Failure::EmptyHandoff));
    }

    #[test]
    fn never_clears_while_the_agent_works_or_waits() {
        let mut reset = Reset::default();
        at(&mut reset, 80);
        assert_eq!(
            reset.stop(ON, true, "done", ok),
            Next::Nothing,
            "background tasks"
        );
        assert_eq!(reset.stop(ON, false, "done", ok), Next::AskHandoff);
        reset.activity(Status::Waiting);
        assert_eq!(reset.gauge.failure, Some(Failure::Busy), "an approval");

        let mut reset = Reset::default();
        at(&mut reset, 80);
        reset.stop(ON, false, "done", ok);
        reset.stop(ON, false, "handoff", ok);
        let step = reset.step();
        reset.activity(Status::Working);
        assert_eq!(reset.gauge.failure, Some(Failure::Busy));
        assert!(!reset.clear_due(step));

        let mut reset = Reset::default();
        at(&mut reset, 80);
        reset.stop(ON, false, "done", ok);
        reset.stop(ON, false, "handoff", ok);
        reset.session_start("clear");
        reset.prompt_submitted();
        assert_eq!(
            reset.gauge.failure,
            Some(Failure::Busy),
            "a prompt before ours"
        );

        let mut reset = Reset::default();
        at(&mut reset, 80);
        reset.stop(ON, false, "done", ok);
        reset.interrupted();
        assert_eq!(reset.gauge.failure, Some(Failure::Interrupted));
    }

    #[test]
    fn times_out_only_the_current_step() {
        let mut reset = Reset::default();
        at(&mut reset, 80);
        reset.stop(ON, false, "done", ok);
        let asked = reset.step();
        reset.stop(ON, false, "handoff", ok);
        assert!(!reset.timeout(asked));
        assert_eq!(reset.gauge.phase, Phase::Clearing);
        assert!(reset.timeout(reset.step()));
        assert_eq!(reset.gauge.failure, Some(Failure::Timeout));
        assert_eq!(reset.session_start("clear"), None, "too late");
    }

    #[test]
    fn off_means_only_by_hand() {
        let off = Limits {
            on: false,
            reset_at: 70,
        };
        let mut reset = Reset::default();
        at(&mut reset, 99);
        assert_eq!(reset.stop(off, false, "done", ok), Next::Nothing);
        assert_eq!(reset.gauge.phase, Phase::Idle);

        assert!(reset.request(true), "idle: typed at once");
        assert_eq!(reset.gauge.phase, Phase::Handoff);
        assert!(!reset.request(true), "already running");

        let mut reset = Reset::default();
        assert!(!reset.request(false));
        assert_eq!(reset.gauge.phase, Phase::Queued);
        assert_eq!(
            reset.stop(off, true, "done", ok),
            Next::Nothing,
            "background work"
        );
        assert_eq!(reset.stop(off, false, "done", ok), Next::AskHandoff);

        let mut reset = Reset::default();
        reset.request(false);
        reset.typed();
        assert_eq!(reset.stop(off, false, "done", ok), Next::Nothing);
        assert_eq!(reset.gauge.failure, Some(Failure::Typed));
    }

    #[test]
    fn hook_outputs_are_what_claude_code_reads() {
        let request = handoff_request(Some(72));
        assert!(!request.contains('\n') && request.contains("72%"));
        assert!(!handoff_request(None).contains('\n'));
        assert_eq!(
            stop_output("write it"),
            json!({"hookSpecificOutput": {"hookEventName": "Stop", "additionalContext": "write it"}})
        );
        let file = Path::new("/data/sessions/3/handoff.md");
        let start = session_start_output("Goal: X", file);
        assert_eq!(start["hookSpecificOutput"]["hookEventName"], "SessionStart");
        let context = start["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(
            context.ends_with("\n\nGoal: X") && context.contains("/data/sessions/3/handoff.md")
        );
        let prompt = resume_prompt(file);
        assert!(!prompt.contains('\n') && !prompt.starts_with('/'));
    }
}
