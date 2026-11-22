//! The main window and the macOS menu bar: the window's colour before the page paints, its place from one launch to
//! the next, its close button on macOS (it hides the window, the agents keep working), and the native menu in the
//! app's language.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::window::Color;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Runtime, Theme, Window, WindowEvent,
};

/// `--bg` of app.css, light and dark: no white flash before the page paints in a dark system.
const LIGHT: Color = Color(0xee, 0xf0, 0xf3, 0xff);
const DARK: Color = Color(0x0c, 0x0d, 0x10, 0xff);

/// Where the window was, in logical pixels: its size before it was maximized, and whether it was.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub maximized: bool,
}

pub struct Placement {
    file: PathBuf,
    place: Mutex<Option<Place>>,
}

/// Whether the middle of the title bar is on one of the screens (x, y, width, height): a window last seen on a
/// screen since unplugged opens centred instead.
fn on_screen(place: &Place, screens: &[(f64, f64, f64, f64)]) -> bool {
    let (x, y) = (place.x + place.width / 2.0, place.y + 16.0);
    place.width > 0.0
        && place.height > 0.0
        && screens
            .iter()
            .any(|&(sx, sy, w, h)| x >= sx && x < sx + w && y >= sy && y < sy + h)
}

/// Shows the main window, created hidden, in the theme's colour and where it was.
pub fn open<R: Runtime>(app: &AppHandle<R>, data_dir: &Path) {
    let file = data_dir.join("window.json");
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let dark = window.theme().is_ok_and(|t| t == Theme::Dark);
    let _ = window.set_background_color(Some(if dark { DARK } else { LIGHT }));
    let screens: Vec<_> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let (p, s) = (m.position(), m.size());
            let scale = m.scale_factor();
            let (p, s) = (p.to_logical::<f64>(scale), s.to_logical::<f64>(scale));
            (p.x, p.y, s.width, s.height)
        })
        .collect();
    let place = fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice::<Place>(&b).ok())
        .filter(|p| on_screen(p, &screens));
    app.manage(Placement {
        file,
        place: Mutex::new(place),
    });
    if let Some(place) = place {
        let _ = window.set_size(LogicalSize::new(place.width, place.height));
        let _ = window.set_position(LogicalPosition::new(place.x, place.y));
        if place.maximized {
            let _ = window.maximize();
        }
    }
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn on_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => keep(window),
        // macOS keeps the app running without a window: the Dock icon or the quick terminal bring it back.
        #[cfg(target_os = "macos")]
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = window.hide();
        }
        _ => {}
    }
}

fn keep<R: Runtime>(window: &Window<R>) {
    let Some(state) = window.try_state::<Placement>() else {
        return;
    };
    let (Ok(scale), Ok(position), Ok(size)) = (
        window.scale_factor(),
        window.outer_position(),
        window.inner_size(),
    ) else {
        return;
    };
    if window.is_minimized().unwrap_or(true) || window.is_fullscreen().unwrap_or(true) {
        return;
    }
    let (position, size) = (
        position.to_logical::<f64>(scale),
        size.to_logical::<f64>(scale),
    );
    let now = Place {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized: window.is_maximized().unwrap_or(false),
    };
    let mut place = state.place.lock().unwrap();
    *place = match *place {
        Some(before) if now.maximized => Some(Place {
            maximized: true,
            ..before
        }),
        _ => Some(now),
    };
}

/// Called once the app exits.
pub fn save<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<Placement>() else {
        return;
    };
    if let Some(place) = *state.place.lock().unwrap()
        && let Ok(json) = serde_json::to_vec(&place)
    {
        if let Some(dir) = state.file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&state.file, json);
    }
}

/// A click on the Dock icon while the window is hidden.
#[cfg(target_os = "macos")]
pub fn reopen<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The menu's words, from i18n.ts (`packagingMenu*`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct Words {
    about: String,
    settings: String,
    services: String,
    hide: String,
    hide_others: String,
    show_all: String,
    quit: String,
    edit: String,
    undo: String,
    redo: String,
    cut: String,
    copy: String,
    paste: String,
    select_all: String,
    view: String,
    fullscreen: String,
    window: String,
    minimize: String,
    zoom: String,
    front: String,
    help: String,
    website: String,
}

const SETTINGS: &str = "settings";
const WEBSITE: &str = "website";
const SITE: &str = "https://vassilidev.github.io/TermF/";

/// The macOS menu bar, without Close Window, whose ⌘W would never reach the webview: there it closes a split. Until the
/// UI sends its words, Tauri's English defaults, without Settings… nor Help.
#[cfg(target_os = "macos")]
pub fn menu<R: Runtime>(
    app: &AppHandle<R>,
    words: Option<&Words>,
) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{
        AboutMetadata, HELP_SUBMENU_ID, Menu, MenuItem, PredefinedMenuItem as Item, Submenu,
        WINDOW_SUBMENU_ID,
    };
    let none = Words::default();
    let w = words.unwrap_or(&none);
    fn text(s: &str) -> Option<&str> {
        (!s.is_empty()).then_some(s)
    }
    let title = |s: &str, english: &'static str| text(s).unwrap_or(english).to_owned();
    let info = app.package_info();
    let config = app.config();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };
    let separator = || Item::separator(app);
    let app_menu = Submenu::with_items(
        app,
        &info.name,
        true,
        &[
            &Item::about(app, text(&w.about), Some(about))?,
            &separator()?,
        ],
    )?;
    if words.is_some() {
        app_menu.append(&MenuItem::with_id(
            app,
            SETTINGS,
            &w.settings,
            true,
            Some("CmdOrCtrl+,"),
        )?)?;
        app_menu.append(&separator()?)?;
    }
    app_menu.append_items(&[
        &Item::services(app, text(&w.services))?,
        &separator()?,
        &Item::hide(app, text(&w.hide))?,
        &Item::hide_others(app, text(&w.hide_others))?,
        &Item::show_all(app, text(&w.show_all))?,
        &separator()?,
        &Item::quit(app, text(&w.quit))?,
    ])?;
    let menu = Menu::with_items(
        app,
        &[
            &app_menu,
            &Submenu::with_items(
                app,
                title(&w.edit, "Edit"),
                true,
                &[
                    &Item::undo(app, text(&w.undo))?,
                    &Item::redo(app, text(&w.redo))?,
                    &separator()?,
                    &Item::cut(app, text(&w.cut))?,
                    &Item::copy(app, text(&w.copy))?,
                    &Item::paste(app, text(&w.paste))?,
                    &Item::select_all(app, text(&w.select_all))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                title(&w.view, "View"),
                true,
                &[&Item::fullscreen(app, text(&w.fullscreen))?],
            )?,
            &Submenu::with_id_and_items(
                app,
                WINDOW_SUBMENU_ID,
                title(&w.window, "Window"),
                true,
                &[
                    &Item::minimize(app, text(&w.minimize))?,
                    &Item::maximize(app, text(&w.zoom))?,
                    &separator()?,
                    &Item::bring_all_to_front(app, text(&w.front))?,
                ],
            )?,
        ],
    )?;
    if words.is_some() {
        menu.append(&Submenu::with_id_and_items(
            app,
            HELP_SUBMENU_ID,
            &w.help,
            true,
            &[&MenuItem::with_id(
                app,
                WEBSITE,
                &w.website,
                true,
                None::<&str>,
            )?],
        )?)?;
    }
    Ok(menu)
}

pub fn on_menu<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    use tauri::Emitter;
    match event.id().as_ref() {
        SETTINGS => {
            #[cfg(target_os = "macos")]
            reopen(app);
            let _ = app.emit("menu-settings", ());
        }
        WEBSITE => {
            let _ = crate::browser::open_url(SITE.into());
        }
        _ => {}
    }
}

/// The menu bar in the UI's language (macOS; elsewhere TermF has no menu bar).
#[tauri::command]
pub fn app_menu(app: AppHandle, words: Words) -> crate::CmdResult<()> {
    #[cfg(target_os = "macos")]
    menu(&app, Some(&words))
        .and_then(|menu| app.set_menu(menu))
        .map_err(|e| e.to_string())?;
    #[cfg(not(target_os = "macos"))]
    let _ = (app, words);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Place, on_screen};

    #[test]
    fn opens_where_it_was_only_on_a_screen_still_there() {
        let screens = [(0.0, 0.0, 1512.0, 982.0), (1512.0, -200.0, 2560.0, 1440.0)];
        let at = |x, y| Place {
            x,
            y,
            width: 1400.0,
            height: 900.0,
            maximized: false,
        };
        assert!(on_screen(&at(40.0, 30.0), &screens));
        assert!(on_screen(&at(2000.0, -150.0), &screens), "second screen");
        assert!(
            on_screen(&at(-600.0, 30.0), &screens),
            "half off the left edge"
        );
        assert!(!on_screen(&at(5000.0, 30.0), &screens), "unplugged screen");
        assert!(
            !on_screen(&at(40.0, 1200.0), &screens),
            "below every screen"
        );
        assert!(!on_screen(&Place::default(), &screens), "no size");
    }
}
