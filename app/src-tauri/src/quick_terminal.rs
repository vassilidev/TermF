use std::path::PathBuf;
use std::sync::Mutex;

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Emitter, Manager, State, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};
use termf_term::{SpawnSpec, shell_command};

use crate::{AppState, CmdResult, err};

/// The drop-down shell: a normal PTY session that never shows up in the grid.
#[derive(Default)]
pub struct QuickTerminal {
    pub session: Mutex<Option<u32>>,
    /// Why the global shortcut could not be registered.
    pub error: Mutex<Option<String>>,
    /// The system-wide shortcut actually registered; empty: none.
    pub shortcut: Mutex<String>,
}

pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(on_shortcut)
        .build()
}

/// Registers the system-wide shortcut set in `termf.toml`, if any. A bad or taken shortcut is reported
/// in the UI rather than stopping the app.
pub fn register(app: &AppHandle) {
    let state = app.state::<AppState>();
    let wanted = termf_core::load_settings(&state.config_dir)
        .map(|s| s.global_shortcut())
        .unwrap_or_default();
    follow(app, &wanted);
}

/// Registers `wanted` unless it already is (after a sync pull, say); a refusal is kept for the UI.
pub fn follow(app: &AppHandle, wanted: &str) {
    let quick = &app.state::<AppState>().quick_terminal;
    if *quick.shortcut.lock().unwrap() == wanted {
        return;
    }
    if let Err(e) = replace(app, wanted) {
        *quick.error.lock().unwrap() = Some(format!("{e:#}"));
    }
}

/// Swaps the registered system-wide shortcut for `new` (empty: none) and returns the one it replaced. When the
/// system refuses `new`, the previous one stays registered.
pub fn replace(app: &AppHandle, new: &str) -> anyhow::Result<String> {
    let quick = &app.state::<AppState>().quick_terminal;
    let mut current = quick.shortcut.lock().unwrap();
    let shortcuts = app.global_shortcut();
    if !current.is_empty() {
        let _ = shortcuts.unregister(current.as_str());
    }
    if !new.is_empty()
        && let Err(e) = shortcuts.register(new)
    {
        if !current.is_empty() {
            let _ = shortcuts.register(current.as_str());
        }
        anyhow::bail!("{new}: {e}");
    }
    *quick.error.lock().unwrap() = None;
    Ok(std::mem::replace(&mut current, new.to_owned()))
}

fn on_shortcut(app: &AppHandle, _: &Shortcut, event: ShortcutEvent) {
    if event.state != ShortcutState::Pressed {
        return;
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let was_focused = window.is_focused().unwrap_or(false);
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    // The UI only hides the panel when TermF was already in front; otherwise it shows it.
    let _ = app.emit("quick-terminal", was_focused);
}

/// The running quick terminal, or a new shell in `cwd` (the home folder when `None`).
#[tauri::command]
pub fn quick_terminal(
    state: State<AppState>,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
) -> CmdResult<u32> {
    let mut current = state.quick_terminal.session.lock().unwrap();
    if let Some(id) = *current {
        if state
            .sessions
            .list()
            .iter()
            .any(|s| s.id == id && s.running)
        {
            return Ok(id);
        }
        state.api.forget(id);
        let _ = state.sessions.kill(id);
    }
    let (program, args) = shell_command(None);
    let id = state
        .sessions
        .spawn(SpawnSpec {
            title: "quick terminal".into(),
            program,
            args,
            cwd: cwd.map_or_else(|| termf_core::expand_tilde("~"), PathBuf::from),
            agent: None,
            cols,
            rows,
            env: Vec::new(),
            base_env: None,
        })
        .map_err(err)?;
    *current = Some(id);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    #[test]
    fn parses_shortcut_settings() {
        // The example given in the default termf.toml.
        assert_eq!(
            "Alt+Shift+Space".parse::<Shortcut>().unwrap(),
            Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Space)
        );
        assert_eq!(
            "Ctrl+`".parse::<Shortcut>().unwrap(),
            Shortcut::new(Some(Modifiers::CONTROL), Code::Backquote)
        );
        assert!("Ctrl+".parse::<Shortcut>().is_err());
        assert!("Ctrl+Nope".parse::<Shortcut>().is_err());
    }
}
