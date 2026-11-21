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
    let accelerator = termf_core::load_settings(&state.config_dir)
        .map(|s| s.quick_terminal_shortcut)
        .unwrap_or_default();
    let accelerator = accelerator.trim();
    if accelerator.is_empty() {
        return;
    }
    if let Err(e) = app.global_shortcut().register(accelerator) {
        *state.quick_terminal.error.lock().unwrap() = Some(format!("{accelerator}: {e}"));
    }
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
