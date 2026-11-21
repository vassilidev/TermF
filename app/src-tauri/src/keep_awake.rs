//! Keeps the computer from sleeping while agents work: sleep suspends them, a locked screen does not.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use termf_core::KeepAwake as Mode;
use termf_core::ipc;

use crate::AppState;

/// In auto mode, how long the computer stays awake after the last agent stopped working.
const GRACE: Duration = Duration::from_secs(120);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KeepAwakeState {
    mode: Mode,
    display_on: bool,
    /// Sleep is blocked right now.
    active: bool,
    /// Why the system refused to stay awake.
    error: Option<String>,
}

#[derive(Default)]
struct Inner {
    mode: Mode,
    display_on: bool,
    guard: Option<keepawake::KeepAwake>,
    idle_since: Option<Instant>,
    error: Option<String>,
}

#[derive(Default)]
pub struct KeepAwake {
    inner: Mutex<Inner>,
}

impl KeepAwake {
    pub fn state(&self) -> KeepAwakeState {
        let inner = self.inner.lock().unwrap();
        KeepAwakeState {
            mode: inner.mode,
            display_on: inner.display_on,
            active: inner.guard.is_some(),
            error: inner.error.clone(),
        }
    }

    pub fn configure(&self, app: &AppHandle, mode: Mode, display_on: bool) {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.display_on != display_on {
                // The guard is built with the display option, so it is rebuilt with the new one.
                inner.guard = None;
            }
            inner.mode = mode;
            inner.display_on = display_on;
            inner.idle_since = None;
        }
        self.update(app);
    }

    /// Re-evaluates after any change in what sessions are doing.
    pub fn update(&self, app: &AppHandle) {
        let state = app.state::<AppState>();
        let busy = state.api.list(&state.sessions).iter().any(ipc::keeps_awake);

        let mut inner = self.inner.lock().unwrap();
        let was_active = inner.guard.is_some();
        let hold = match inner.mode {
            Mode::Always => true,
            Mode::Off => false,
            Mode::Auto if busy => {
                inner.idle_since = None;
                true
            }
            Mode::Auto => match inner.idle_since {
                _ if !was_active => false,
                Some(since) => since.elapsed() < GRACE,
                None => {
                    inner.idle_since = Some(Instant::now());
                    let app = app.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(GRACE);
                        app.state::<AppState>().keep_awake.update(&app);
                    });
                    true
                }
            },
        };

        if hold && inner.guard.is_none() {
            match keepawake::Builder::default()
                .idle(true)
                .display(inner.display_on)
                .reason("AI agents are working")
                .app_name("TermF")
                .app_reverse_domain("dev.termf.app")
                .create()
            {
                Ok(guard) => {
                    inner.guard = Some(guard);
                    inner.error = None;
                }
                Err(e) => inner.error = Some(e.to_string()),
            }
        } else if !hold {
            inner.guard = None;
            inner.idle_since = None;
        }

        let changed = was_active != inner.guard.is_some();
        drop(inner);
        if changed {
            let _ = app.emit("keep-awake", self.state());
        }
    }
}

#[tauri::command]
pub fn keep_awake_state(state: tauri::State<AppState>) -> KeepAwakeState {
    state.keep_awake.state()
}

#[tauri::command]
pub fn keep_awake_set(
    app: AppHandle,
    state: tauri::State<AppState>,
    mode: Mode,
    display_on: bool,
) -> Result<KeepAwakeState, String> {
    termf_core::save_keep_awake(&state.config_dir, mode, display_on)
        .map_err(|e| format!("{e:#}"))?;
    state.keep_awake.configure(&app, mode, display_on);
    Ok(state.keep_awake.state())
}
