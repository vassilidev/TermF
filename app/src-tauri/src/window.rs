//! The main window and the macOS menu bar: the window's colour before the page paints, its place from one launch to
//! the next, its close button on macOS (it hides the window, the agents keep working), and the native menu in the
//! app's language.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::window::Color;
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, Runtime, Theme, WebviewWindow, Window,
    WindowEvent,
};

include!(concat!(env!("OUT_DIR"), "/brand.rs"));

// macOS places windows in points across screens of any scale, Windows and X11 in pixels: a position kept in the
// other unit reopens the window elsewhere when the screens' scales differ.
const POINTS: bool = cfg!(target_os = "macos");

/// Moves and resizes come in bursts (a drag, the maximize animation): the place is read once they stop.
const SETTLE: Duration = Duration::from_millis(500);

/// The position in the system's own unit (see `POINTS`), the size in logical pixels (the one before maximizing).
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
    moved: Sender<()>,
}

fn native(position: PhysicalPosition<i32>, scale: f64, points: bool) -> (f64, f64) {
    if points {
        let p = position.to_logical::<f64>(scale);
        (p.x, p.y)
    } else {
        (position.x.into(), position.y.into())
    }
}

/// A window last seen on a screen since unplugged opens centred instead.
fn on_screen(place: &Place, screens: &[(f64, f64, f64, f64)]) -> bool {
    let (x, y) = (place.x + place.width / 2.0, place.y + 16.0);
    place.width > 0.0
        && place.height > 0.0
        && screens
            .iter()
            .any(|&(sx, sy, w, h)| x >= sx && x < sx + w && y >= sy && y < sy + h)
}

/// What a settled window leaves of its place: nothing while minimized or full screen, and the size from before when
/// maximized (on Windows, a maximized window's position is off the screen's edge).
fn settle(before: Option<Place>, now: Place, minimized: bool, fullscreen: bool) -> Option<Place> {
    match before {
        _ if minimized || fullscreen => before,
        Some(before) if now.maximized => Some(Place {
            maximized: true,
            ..before
        }),
        _ => Some(now),
    }
}

pub fn open<R: Runtime>(app: &AppHandle<R>, data_dir: &Path) {
    let file = data_dir.join("window.json");
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let dark = window.theme().is_ok_and(|t| t == Theme::Dark);
    let _ = window.set_background_color(Some(BACKGROUNDS[usize::from(dark)]));
    let screens: Vec<_> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let scale = m.scale_factor();
            let (x, y) = native(*m.position(), scale, POINTS);
            let (w, h) = if POINTS {
                let s = m.size().to_logical::<f64>(scale);
                (s.width, s.height)
            } else {
                (m.size().width.into(), m.size().height.into())
            };
            (x, y, w, h)
        })
        .collect();
    let place = fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice::<Place>(&b).ok())
        .filter(|p| on_screen(p, &screens));
    if let Some(place) = place {
        if POINTS {
            let _ = window.set_position(tauri::LogicalPosition::new(place.x, place.y));
        } else {
            let _ = window.set_position(PhysicalPosition::new(place.x as i32, place.y as i32));
        }
        let _ = window.set_size(LogicalSize::new(place.width, place.height));
        if place.maximized {
            let _ = window.maximize();
        }
    }
    let (moved, rx) = mpsc::channel();
    app.manage(Placement {
        file,
        place: Mutex::new(place),
        moved,
    });
    let watched = window.clone();
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            while rx.recv_timeout(SETTLE).is_ok() {}
            keep(&watched);
        }
    });
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn on_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            if let Some(state) = window.try_state::<Placement>() {
                let _ = state.moved.send(());
            }
        }
        #[cfg(target_os = "macos")]
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = window.hide();
        }
        _ => {}
    }
}

/// Written once the window settles, so that a crash or a forced quit keeps it.
fn keep<R: Runtime>(window: &WebviewWindow<R>) {
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
    let (x, y) = native(position, scale, POINTS);
    let size = size.to_logical::<f64>(scale);
    let now = Place {
        x,
        y,
        width: size.width,
        height: size.height,
        maximized: window.is_maximized().unwrap_or(false),
    };
    let mut place = state.place.lock().unwrap();
    let next = settle(
        *place,
        now,
        window.is_minimized().unwrap_or(true),
        window.is_fullscreen().unwrap_or(true),
    );
    if next != *place {
        *place = next;
        write(&state.file, next);
    }
}

fn write(file: &Path, place: Option<Place>) {
    if let Some(place) = place
        && let Ok(json) = serde_json::to_vec(&place)
    {
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(file, json);
    }
}

/// At exit: a move less than `SETTLE` before quitting.
pub fn save<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        keep(&window);
    }
}

#[cfg(target_os = "macos")]
pub fn reopen<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

const SETTINGS: &str = "settings";
const WEBSITE: &str = "website";

/// The macOS menu bar, without Close Window, whose ⌘W would never reach the webview: there it closes a split. In the
/// system's language, as the UI (`lang` of i18n.ts); its words are the `packagingMenu*` keys of i18n.ts (build.rs).
#[cfg(target_os = "macos")]
pub fn menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{
        AboutMetadata, HELP_SUBMENU_ID, Menu, MenuItem, PredefinedMenuItem as Item, Submenu,
        WINDOW_SUBMENU_ID,
    };
    let french = objc2_foundation::NSLocale::preferredLanguages()
        .firstObject()
        .is_some_and(|l| l.to_string().to_lowercase().starts_with("fr"));
    let words = MENU[if french { 0 } else { 1 }];
    let w = |key: &'static str| -> &'static str {
        words
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(key, |(_, text)| *text)
    };
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
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                &info.name,
                true,
                &[
                    &Item::about(app, Some(w("About")), Some(about))?,
                    &separator()?,
                    &MenuItem::with_id(app, SETTINGS, w("Settings"), true, Some("CmdOrCtrl+,"))?,
                    &separator()?,
                    &Item::services(app, Some(w("Services")))?,
                    &separator()?,
                    &Item::hide(app, Some(w("Hide")))?,
                    &Item::hide_others(app, Some(w("HideOthers")))?,
                    &Item::show_all(app, Some(w("ShowAll")))?,
                    &separator()?,
                    &Item::quit(app, Some(w("Quit")))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                w("Edit"),
                true,
                &[
                    &Item::undo(app, Some(w("Undo")))?,
                    &Item::redo(app, Some(w("Redo")))?,
                    &separator()?,
                    &Item::cut(app, Some(w("Cut")))?,
                    &Item::copy(app, Some(w("Copy")))?,
                    &Item::paste(app, Some(w("Paste")))?,
                    &Item::select_all(app, Some(w("SelectAll")))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                w("View"),
                true,
                &[&Item::fullscreen(app, Some(w("Fullscreen")))?],
            )?,
            &Submenu::with_id_and_items(
                app,
                WINDOW_SUBMENU_ID,
                w("Window"),
                true,
                &[
                    &Item::minimize(app, Some(w("Minimize")))?,
                    &Item::maximize(app, Some(w("Zoom")))?,
                    &separator()?,
                    &Item::bring_all_to_front(app, Some(w("Front")))?,
                ],
            )?,
            &Submenu::with_id_and_items(
                app,
                HELP_SUBMENU_ID,
                w("Help"),
                true,
                &[&MenuItem::with_id(
                    app,
                    WEBSITE,
                    w("Website"),
                    true,
                    None::<&str>,
                )?],
            )?,
        ],
    )
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
            if let Some(site) = app.config().bundle.homepage.clone() {
                let _ = crate::browser::open_url(site);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{BACKGROUNDS, MENU, Place, native, on_screen, settle};
    use tauri::PhysicalPosition;
    use tauri::window::Color;

    fn at(x: f64, y: f64) -> Place {
        Place {
            x,
            y,
            width: 1400.0,
            height: 900.0,
            maximized: false,
        }
    }

    #[test]
    fn opens_where_it_was_only_on_a_screen_still_there() {
        let screens = [(0.0, 0.0, 1512.0, 982.0), (1512.0, -200.0, 2560.0, 1440.0)];
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

    #[test]
    fn keeps_pixels_on_windows_with_screens_of_different_scales() {
        // A 4K screen at 200 %, then a 1080p one at 100 % on its right, as Windows lays them out in pixels.
        let screens = [(0.0, 0.0, 3840.0, 2160.0), (3840.0, 0.0, 1920.0, 1080.0)];
        let window = PhysicalPosition::new(4000, 100);
        let (x, y) = native(window, 1.0, false);
        assert_eq!((x, y), (4000.0, 100.0));
        let place = Place {
            x,
            y,
            width: 1000.0,
            height: 700.0,
            maximized: false,
        };
        assert!(on_screen(&place, &screens));
        // In points, the same window would be taken back to the first screen at its scale.
        assert_eq!(native(window, 2.0, true), (2000.0, 50.0));
    }

    #[test]
    fn keeps_only_settled_normal_places() {
        let before = Some(at(100.0, 80.0));
        let maximized = Place {
            x: -8.0,
            y: -8.0,
            width: 2560.0,
            height: 1400.0,
            maximized: true,
        };
        assert_eq!(
            settle(before, maximized, false, false),
            Some(Place {
                maximized: true,
                ..at(100.0, 80.0)
            }),
            "maximized keeps the size from before"
        );
        assert_eq!(
            settle(before, at(0.0, 0.0), false, true),
            before,
            "full screen"
        );
        assert_eq!(
            settle(before, at(-32000.0, -32000.0), true, false),
            before,
            "minimized"
        );
        assert_eq!(
            settle(before, at(300.0, 200.0), false, false),
            Some(at(300.0, 200.0))
        );
        assert_eq!(settle(None, maximized, false, false), Some(maximized));
    }

    #[test]
    fn reads_the_brand_from_the_ui_sources() {
        assert_eq!(BACKGROUNDS[0], Color(0xee, 0xf0, 0xf3, 255));
        assert_eq!(BACKGROUNDS[1], Color(0x0c, 0x0d, 0x10, 255));
        assert!(MENU[0].contains(&("Settings", "Réglages…")));
        assert!(MENU[1].contains(&("Settings", "Settings…")));
        assert_eq!(MENU[0].len(), MENU[1].len());
    }
}
