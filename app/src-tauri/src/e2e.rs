//! The end-to-end bridge, only in the `e2e` build (`pnpm e2e:build`): where no WebDriver drives the webview (macOS,
//! and Windows' WebView2 that never opened its debugging port to tauri-driver), the tests send JavaScript to this
//! server, the real window runs it and hands the result back through `e2e_result`. Off unless `TERMF_E2E_PORT` and
//! `TERMF_E2E_TOKEN` are set; listens on 127.0.0.1 only and refuses any request without the token.
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, State, Wry};

// A release package never carries the bridge, whatever its features.
#[cfg(not(debug_assertions))]
compile_error!("the e2e bridge is for debug builds only");

/// Its own identifier: its own single-instance socket, webview data and app folders, never the user's TermF.
pub const IDENTIFIER: &str = "dev.termf.app.e2e";

#[derive(Default)]
pub struct Pending {
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, Sender<String>>>,
}

pub fn plugin() -> Option<TauriPlugin<Wry>> {
    let port: u16 = std::env::var("TERMF_E2E_PORT").ok()?.parse().ok()?;
    let token = std::env::var("TERMF_E2E_TOKEN")
        .ok()
        .filter(|t| t.len() >= 32)?;
    // The tests' own setup (error collection), run before the page's scripts.
    let init = std::env::var("TERMF_E2E_INIT").unwrap_or_default();
    let plugin = tauri::plugin::Builder::new("e2e")
        .js_init_script(init)
        .setup(move |app, _| {
            if app.config().identifier != IDENTIFIER {
                eprintln!("termf: e2e bridge off, the app is not built as {IDENTIFIER}");
                return Ok(());
            }
            app.manage(Pending::default());
            let listener = TcpListener::bind(("127.0.0.1", port))?;
            let app = app.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let (app, token) = (app.clone(), token.clone());
                    std::thread::spawn(move || {
                        let _ = serve(&app, stream, &token);
                    });
                }
            });
            Ok(())
        })
        .build();
    Some(plugin)
}

#[tauri::command]
pub fn e2e_result(pending: State<Pending>, id: u64, result: String) {
    if let Some(tx) = pending.waiting.lock().unwrap().remove(&id) {
        let _ = tx.send(result);
    }
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |d, (x, y)| d | (x ^ y)) == 0
}

fn serve(app: &AppHandle, stream: TcpStream, token: &str) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let (mut length, mut authorized) = (0usize, false);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            match name.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = value.trim().parse().unwrap_or(0),
                "x-termf-e2e-token" => authorized = same(value.trim(), token),
                _ => {}
            }
        }
    }
    let mut body = vec![0; length.min(16 << 20)];
    reader.read_exact(&mut body)?;
    let route = request.split(' ').take(2).collect::<Vec<_>>().join(" ");
    let (status, reply) = match (authorized, route.as_str()) {
        (false, _) => (401, "\"a token is required\"".to_string()),
        (true, "GET /status") => (200, window_number(app).to_string()),
        (true, "POST /eval") => eval(app, &String::from_utf8_lossy(&body)),
        (true, "POST /quit") => {
            app.exit(0);
            (200, "null".into())
        }
        _ => (404, "\"no such route\"".into()),
    };
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 {status} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
        if status == 200 { "OK" } else { "Error" },
        reply.len()
    )
}

/// `expr` is a JavaScript expression, awaited in the window; the reply is `{"ok", "value"}` as JSON.
fn eval(app: &AppHandle, expr: &str) -> (u16, String) {
    let Some(window) = app.get_webview_window("main") else {
        return (503, "\"no window yet\"".into());
    };
    let pending = app.state::<Pending>();
    let id = pending.next.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = channel();
    pending.waiting.lock().unwrap().insert(id, tx);
    let script = format!(
        "(async () => {{ let r; try {{ r = JSON.stringify({{ ok: true, value: await ({expr}) }}); }} \
         catch (e) {{ r = JSON.stringify({{ ok: false, value: String((e && e.stack) || e) }}); }} \
         window.__TAURI_INTERNALS__.invoke('e2e_result', {{ id: {id}, result: r }}); }})()"
    );
    let reply = window
        .eval(script)
        .map_err(|e| e.to_string())
        .and_then(|()| {
            rx.recv_timeout(Duration::from_secs(20))
                .map_err(|_| "no answer from the page".into())
        });
    pending.waiting.lock().unwrap().remove(&id);
    match reply {
        Ok(json) => (200, json),
        Err(e) => (504, format!("{e:?}")),
    }
}

/// The window's number for `screencapture -l`, on macOS; `null` elsewhere or before the window shows.
fn window_number(app: &AppHandle) -> String {
    let Some(window) = app.get_webview_window("main") else {
        return "null".into();
    };
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = channel();
        let w = window.clone();
        let _ = window.run_on_main_thread(move || {
            let n = w.ns_window().ok().map(|ptr| {
                let ns: &objc2::runtime::AnyObject = unsafe { &*(ptr as *const _) };
                let n: isize = unsafe { objc2::msg_send![ns, windowNumber] };
                n
            });
            let _ = tx.send(n);
        });
        if let Ok(Some(n)) = rx.recv_timeout(Duration::from_secs(5)) {
            return n.to_string();
        }
    }
    let _ = window;
    "null".into()
}
