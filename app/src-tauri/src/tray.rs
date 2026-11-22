//! The icon in the menu bar (macOS) or the notification area (Windows, Linux with libappindicator), and the count on
//! the dock or taskbar icon: what waits for the user (ARCHITECTURE §13.3), the sessions open to Remote Control, and a
//! few shortcuts. The UI says what to show, in its own words.

use std::sync::Mutex;

use serde::Deserialize;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, Wry};
use termf_core::{KeepAwake, TraySettings};

use crate::{AppState, CmdResult, err};

const ID: &str = "termf";
/// The menu lists this many waits at most; the window has them all.
const MOST: usize = 10;

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Entry {
    key: String,
    label: String,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Shown {
    /// What waits for the user: the badge and the count beside the icon.
    count: u32,
    tooltip: String,
    waits_title: String,
    waits: Vec<Entry>,
    nothing: String,
    remote_title: String,
    /// The tiles open to Remote Control, by session id.
    remote: Vec<Entry>,
    cut_all: String,
    quick: String,
    open: String,
    awake: String,
    quit: String,
}

pub struct Tray {
    shown: Mutex<Option<Shown>>,
    settings: Mutex<TraySettings>,
}

impl Tray {
    pub fn new(settings: TraySettings) -> Self {
        Self {
            shown: Mutex::default(),
            settings: Mutex::new(settings),
        }
    }
}

/// Built at the UI's first word, so that its menu is never shown without the words.
#[tauri::command]
pub fn tray_update(app: AppHandle, tray: State<Tray>, shown: Shown) -> CmdResult<()> {
    if tray.shown.lock().unwrap().as_ref() == Some(&shown) {
        return Ok(());
    }
    *tray.shown.lock().unwrap() = Some(shown);
    apply(&app, &tray).map_err(err)
}

#[tauri::command]
pub fn tray_settings(tray: State<Tray>) -> TraySettings {
    *tray.settings.lock().unwrap()
}

#[tauri::command]
pub fn tray_settings_set(
    app: AppHandle,
    state: State<AppState>,
    tray: State<Tray>,
    settings: TraySettings,
) -> CmdResult<TraySettings> {
    termf_core::save_tray(&state.config_dir, settings).map_err(err)?;
    *tray.settings.lock().unwrap() = settings;
    apply(&app, &tray).map_err(err)?;
    Ok(settings)
}

fn apply(app: &AppHandle, tray: &Tray) -> anyhow::Result<()> {
    let settings = *tray.settings.lock().unwrap();
    let Some(shown) = tray.shown.lock().unwrap().clone() else {
        return Ok(());
    };
    badge(app, if settings.badge { shown.count } else { 0 });
    if !settings.icon {
        app.remove_tray_by_id(ID);
        return Ok(());
    }
    let menu = menu(app, &shown)?;
    let icon = match app.tray_by_id(ID) {
        Some(icon) => {
            icon.set_menu(Some(menu))?;
            icon
        }
        None => build(app, &menu)?,
    };
    icon.set_tooltip(Some(&shown.tooltip))?;
    // Beside the icon in the macOS menu bar and some Linux panels; Windows has the tooltip.
    icon.set_title((shown.count > 0).then(|| shown.count.to_string()))?;
    Ok(())
}

fn build(app: &AppHandle, menu: &Menu<Wry>) -> tauri::Result<tauri::tray::TrayIcon> {
    let builder = TrayIconBuilder::with_id(ID)
        .menu(menu)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu);
    // A template image takes the menu bar's own color, light or dark.
    #[cfg(target_os = "macos")]
    let builder = builder
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true);
    #[cfg(not(target_os = "macos"))]
    let builder = match app.default_window_icon() {
        Some(icon) => builder.icon(icon.clone()),
        None => builder,
    };
    builder.build(app)
}

fn menu(app: &AppHandle, shown: &Shown) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    let label = |text: &str| MenuItem::new(app, text, false, None::<&str>);
    let item = |id: String, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    let separator = || PredefinedMenuItem::separator(app);
    menu.append(&label(&shown.waits_title)?)?;
    for wait in shown.waits.iter().take(MOST) {
        menu.append(&item(format!("wait:{}", wait.key), &wait.label)?)?;
    }
    if shown.waits.is_empty() {
        menu.append(&label(&shown.nothing)?)?;
    }
    if !shown.remote.is_empty() {
        menu.append(&separator()?)?;
        menu.append(&label(&shown.remote_title)?)?;
        for tile in &shown.remote {
            menu.append(&item(format!("tile:{}", tile.key), &tile.label)?)?;
        }
        menu.append(&item("cut-all".into(), &shown.cut_all)?)?;
    }
    let always = app.state::<AppState>().keep_awake.state().mode == KeepAwake::Always;
    menu.append(&separator()?)?;
    menu.append(&item("quick".into(), &shown.quick)?)?;
    menu.append(&item("open".into(), &shown.open)?)?;
    menu.append(&CheckMenuItem::with_id(
        app,
        "awake",
        &shown.awake,
        true,
        always,
        None::<&str>,
    )?)?;
    menu.append(&separator()?)?;
    menu.append(&item("quit".into(), &shown.quit)?)?;
    Ok(menu)
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "quit" => app.exit(0),
        "open" => show(app),
        "awake" => toggle_awake(app),
        "cut-all" => {
            let app = app.clone();
            std::thread::spawn(move || {
                app.state::<AppState>().api.remote_cut_all(&app);
            });
        }
        "quick" => {
            show(app);
            // As the system-wide shortcut does, with TermF not in front: the panel opens.
            let _ = app.emit("quick-terminal", false);
        }
        // `wait:<key>` or `tile:<id>`: the window goes there.
        other => {
            show(app);
            let _ = app.emit("tray-open", other);
        }
    }
}

fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// "Stay awake": always, or back to while agents work.
fn toggle_awake(app: &AppHandle) {
    let state = app.state::<AppState>();
    let now = state.keep_awake.state();
    let mode = match now.mode {
        KeepAwake::Always => KeepAwake::Auto,
        _ => KeepAwake::Always,
    };
    if let Err(e) = termf_core::save_keep_awake(&state.config_dir, mode, now.display_on) {
        eprintln!("termf: keep awake not saved: {e:#}");
        return;
    }
    state.keep_awake.configure(app, mode, now.display_on);
    let _ = app.emit("keep-awake", state.keep_awake.state());
}

/// The dock's badge on macOS, the launcher's count on Linux desktops that show one, a dot over the taskbar icon on
/// Windows, which has no count; none at zero.
fn badge(app: &AppHandle, count: u32) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    #[cfg(not(windows))]
    let _ = window.set_badge_count((count > 0).then_some(i64::from(count)));
    #[cfg(windows)]
    let _ = window.set_overlay_icon((count > 0).then(dot));
}

/// The amber of "waiting", 16 px.
#[cfg(windows)]
fn dot() -> tauri::image::Image<'static> {
    let size = 16u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            let alpha = (7.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
            rgba.extend([0xf2, 0xb0, 0x3d, (alpha * 255.0) as u8]);
        }
    }
    tauri::image::Image::new_owned(rgba, size, size)
}
