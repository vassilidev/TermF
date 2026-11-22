//! Headless Chrome (or Chromium, Edge) driven over the DevTools protocol: a WebSocket client on the standard
//! library, since a capture only needs to send a command and read its answer.

use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use base64ct::Encoding;
use serde_json::{Value, json};

use crate::config::{Login, field_value};

const MAX_HEIGHT: u32 = 8000;
const LOAD_TIMEOUT: Duration = Duration::from_secs(45);
/// After the fonts, for the last layout and images.
const SETTLE: Duration = Duration::from_millis(400);

/// So that two captures of an unchanged page are the same: no animation, transition or caret, and the zones
/// that change on their own hidden.
const FREEZE: &str = r#"((masks) => {
  const style = document.createElement('style');
  style.textContent = '*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important;scroll-behavior:auto!important}'
    + masks.map((m) => m + '{visibility:hidden!important}').join('');
  (document.head || document.documentElement).appendChild(style);
  return document.fonts.ready.then(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(true)))));
})"#;

const TEXTS: &str = r#"(() => {
  const shown = (el) => el.checkVisibility ? el.checkVisibility({ visibilityProperty: true, opacityProperty: true }) : el.getClientRects().length > 0;
  const clean = (t) => t.replace(/\s+/g, ' ').trim();
  const out = [];
  if (!document.body) return out;
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const p = n.parentElement;
    const t = clean(n.textContent || '');
    if (t && p && !['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEMPLATE'].includes(p.tagName) && shown(p)) out.push(t);
  }
  for (const el of document.querySelectorAll('[alt],[title],[placeholder],[aria-label]')) {
    if (!shown(el)) continue;
    for (const a of ['alt', 'title', 'placeholder', 'aria-label']) {
      const v = clean(el.getAttribute(a) || '');
      if (v) out.push(v);
    }
  }
  return out;
})()"#;

const SIZE: &str = "[document.documentElement.scrollWidth, Math.max(document.documentElement.scrollHeight, document.body ? document.body.scrollHeight : 0)]";

/// On macOS and Windows, browsers are seldom on the `PATH`: where their installers put them comes next.
pub fn find() -> Result<PathBuf> {
    const NAMES: [&str; 7] = [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "chrome",
        "msedge",
        "microsoft-edge",
    ];
    let path = termf_core::shell::login_path()
        .map(OsString::from)
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let exts: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };
    for dir in std::env::split_paths(&path) {
        for name in NAMES {
            for ext in exts {
                let file = dir.join(format!("{name}{ext}"));
                if file.is_file() {
                    return Ok(file);
                }
            }
        }
    }
    let mut known: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        let apps = [
            "Google Chrome.app/Contents/MacOS/Google Chrome",
            "Chromium.app/Contents/MacOS/Chromium",
            "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ];
        let home = termf_core::expand_tilde("~/Applications");
        for app in apps {
            known.push(Path::new("/Applications").join(app));
            known.push(home.join(app));
        }
    } else if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
            if let Some(dir) = std::env::var_os(var) {
                let dir = PathBuf::from(dir);
                known.push(dir.join(r"Google\Chrome\Application\chrome.exe"));
                known.push(dir.join(r"Microsoft\Edge\Application\msedge.exe"));
            }
        }
    }
    known.into_iter().find(|p| p.is_file()).context(
        "no Chrome, Chromium or Edge found: install Google Chrome (or Chromium), or put it on your PATH",
    )
}

#[derive(Clone, Debug)]
pub struct Target {
    pub side: &'static str,
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub mobile: bool,
    pub theme: String,
    pub mask: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Shot {
    pub png: Vec<u8>,
    pub texts: Vec<String>,
}

pub struct Browser {
    child: Child,
    profile: PathBuf,
    ws: Ws,
    next: u64,
    session: Option<String>,
}

impl Browser {
    pub fn launch(program: &Path) -> Result<Browser> {
        let profile = termf_core::process::scratch_dir()?;
        let mut cmd = Command::new(program);
        cmd.args([
            "--headless=new",
            "--remote-debugging-port=0",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-gpu",
            "--disable-extensions",
            "--disable-sync",
            "--disable-background-networking",
            "--disable-dev-shm-usage",
            "--hide-scrollbars",
            "--mute-audio",
            "--force-color-profile=srgb",
            "--force-device-scale-factor=1",
        ])
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = cmd
            .spawn()
            .with_context(|| format!("cannot start {}", program.display()))?;
        // Chrome writes its port and its browser endpoint there once it listens.
        let file = profile.join("DevToolsActivePort");
        let deadline = Instant::now() + Duration::from_secs(30);
        let (port, path) = loop {
            if let Some(found) = fs::read_to_string(&file).ok().and_then(|t| {
                let mut lines = t.lines();
                let port: u16 = lines.next()?.trim().parse().ok()?;
                Some((port, lines.next()?.trim().to_owned()))
            }) {
                break found;
            }
            if child.try_wait()?.is_some() || Instant::now() > deadline {
                stop(&mut child);
                let _ = fs::remove_dir_all(&profile);
                bail!("{} did not start headless", program.display());
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let ws = match Ws::connect(port, &path) {
            Ok(ws) => ws,
            Err(e) => {
                stop(&mut child);
                let _ = fs::remove_dir_all(&profile);
                return Err(e);
            }
        };
        let mut browser = Browser {
            child,
            profile,
            ws,
            next: 0,
            session: None,
        };
        let target = browser.call("Target.createTarget", json!({"url": "about:blank"}))?;
        let target = target["targetId"].as_str().context("no page")?.to_owned();
        let attached = browser.call(
            "Target.attachToTarget",
            json!({"targetId": target, "flatten": true}),
        )?;
        browser.session = Some(
            attached["sessionId"]
                .as_str()
                .context("no page session")?
                .to_owned(),
        );
        browser.call("Page.enable", json!({}))?;
        Ok(browser)
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next += 1;
        let id = self.next;
        let mut message = json!({"id": id, "method": method, "params": params});
        if let Some(session) = &self.session {
            message["sessionId"] = session.clone().into();
        }
        self.ws.send(&message.to_string())?;
        loop {
            let reply: Value = serde_json::from_str(&self.ws.recv()?)?;
            // Events of the page come in between.
            if reply["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(error) = reply.get("error") {
                bail!(
                    "{method}: {}",
                    error["message"].as_str().unwrap_or("failed")
                );
            }
            return Ok(reply["result"].clone());
        }
    }

    fn eval(&mut self, script: &str) -> Result<Value> {
        let reply = self.call(
            "Runtime.evaluate",
            json!({"expression": script, "returnByValue": true, "awaitPromise": true}),
        )?;
        if let Some(e) = reply.get("exceptionDetails") {
            let text = e["exception"]["description"]
                .as_str()
                .or(e["text"].as_str())
                .unwrap_or("script failed");
            bail!("{text}");
        }
        Ok(reply["result"]["value"].clone())
    }

    fn open(&mut self, url: &str) -> Result<()> {
        let nav = self.call("Page.navigate", json!({"url": url}))?;
        if let Some(error) = nav["errorText"].as_str().filter(|e| !e.is_empty()) {
            bail!("{url}: {error}");
        }
        let deadline = Instant::now() + LOAD_TIMEOUT;
        while self.eval("document.readyState")?.as_str() != Some("complete") {
            ensure!(Instant::now() < deadline, "{url} did not finish loading");
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    pub fn login(&mut self, base: &str, login: &Login) -> Result<()> {
        self.open(&format!("{base}{}", login.url))?;
        let mut last = String::new();
        for (selector, value) in &login.fields {
            let value = field_value(value)?;
            // The value setter of the element's prototype, so frameworks that track the value see it.
            let script = format!(
                "((sel, value) => {{ const el = document.querySelector(sel); if (!el) return false; \
                 const proto = Object.getPrototypeOf(el); \
                 Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value); \
                 el.dispatchEvent(new Event('input', {{ bubbles: true }})); \
                 el.dispatchEvent(new Event('change', {{ bubbles: true }})); return true; }})({}, {})",
                json!(selector),
                json!(value)
            );
            ensure!(
                self.eval(&script)?.as_bool() == Some(true),
                "the login page has no {selector}"
            );
            last = selector.clone();
        }
        let submit = match &login.submit {
            Some(button) => format!(
                "(() => {{ const b = document.querySelector({}); if (!b) return false; b.click(); return true; }})()",
                json!(button)
            ),
            None => format!(
                "(() => {{ const f = document.querySelector({})?.form; if (!f) return false; f.requestSubmit(); return true; }})()",
                json!(last)
            ),
        };
        ensure!(
            self.eval(&submit)?.as_bool() == Some(true),
            "the login page has nothing to submit"
        );
        let check = match login.wait.as_deref() {
            Some(path) if path.starts_with('/') => format!("location.pathname === {}", json!(path)),
            Some(selector) => format!("!!document.querySelector({})", json!(selector)),
            None => "document.readyState === 'complete'".into(),
        };
        let deadline = Instant::now() + LOAD_TIMEOUT;
        std::thread::sleep(Duration::from_millis(500));
        // A navigation in progress destroys the context the check runs in: that is not an answer yet.
        while self.eval(&check).ok().and_then(|v| v.as_bool()) != Some(true) {
            ensure!(Instant::now() < deadline, "the login did not succeed");
            std::thread::sleep(Duration::from_millis(200));
        }
        Ok(())
    }

    /// The page as a PDF, its own `@page` size and its backgrounds kept. Nothing on the page runs and nothing
    /// loads from the network: it is a document of a repository, not a program.
    pub fn print(&mut self, url: &str) -> Result<Vec<u8>> {
        self.call(
            "Emulation.setScriptExecutionDisabled",
            json!({"value": true}),
        )?;
        self.call("Network.enable", json!({}))?;
        self.call(
            "Network.setBlockedURLs",
            json!({"urls": ["http://*", "https://*", "ws://*", "wss://*", "ftp://*"]}),
        )?;
        self.open(url)?;
        self.eval("document.fonts.ready.then(() => true)")?;
        let pdf = self.call(
            "Page.printToPDF",
            json!({"printBackground": true, "preferCSSPageSize": true}),
        )?;
        let data = pdf["data"].as_str().context("no PDF")?;
        base64ct::Base64::decode_vec(data).context("unreadable PDF")
    }

    pub fn capture(&mut self, target: &Target) -> Result<Shot> {
        self.call(
            "Emulation.setDeviceMetricsOverride",
            json!({"width": target.width, "height": target.height, "deviceScaleFactor": 1, "mobile": target.mobile}),
        )?;
        self.call(
            "Emulation.setEmulatedMedia",
            json!({"media": "", "features": [
                {"name": "prefers-color-scheme", "value": target.theme},
                {"name": "prefers-reduced-motion", "value": "reduce"},
            ]}),
        )?;
        self.open(&target.url)?;
        self.eval(&format!("{FREEZE}({})", json!(target.mask)))?;
        std::thread::sleep(SETTLE);
        let size = self.eval(SIZE)?;
        let height = size[1]
            .as_u64()
            .unwrap_or(target.height as u64)
            .clamp(1, MAX_HEIGHT as u64);
        let shot = self.call(
            "Page.captureScreenshot",
            json!({"format": "png", "captureBeyondViewport": true,
                   "clip": {"x": 0, "y": 0, "width": target.width, "height": height, "scale": 1}}),
        )?;
        let data = shot["data"].as_str().context("no screenshot")?;
        let png = base64ct::Base64::decode_vec(data).context("unreadable screenshot")?;
        let texts = serde_json::from_value(self.eval(TEXTS)?).unwrap_or_default();
        Ok(Shot { png, texts })
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.session = None;
        let _ = self
            .ws
            .send(&json!({"id": 0, "method": "Browser.close"}).to_string());
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        stop(&mut self.child);
        let _ = fs::remove_dir_all(&self.profile);
    }
}

pub(crate) fn stop(child: &mut Child) {
    #[cfg(unix)]
    termf_core::process::kill_group(child.id());
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

struct Ws {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl Ws {
    fn connect(port: u16, path: &str) -> Result<Ws> {
        let mut writer = TcpStream::connect(("127.0.0.1", port))
            .with_context(|| format!("cannot reach Chrome on port {port}"))?;
        writer.set_read_timeout(Some(Duration::from_secs(120)))?;
        let key = base64ct::Base64::encode_string(&mask().repeat(4));
        write!(
            writer,
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )?;
        let mut reader = BufReader::new(writer.try_clone()?);
        let mut status = String::new();
        reader.read_line(&mut status)?;
        ensure!(
            status.split_whitespace().nth(1) == Some("101"),
            "Chrome refused the DevTools connection: {}",
            status.trim()
        );
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            if line.trim().is_empty() {
                break;
            }
        }
        Ok(Ws { reader, writer })
    }

    fn send(&mut self, text: &str) -> Result<()> {
        self.frame(0x1, text.as_bytes())
    }

    fn frame(&mut self, opcode: u8, payload: &[u8]) -> Result<()> {
        let mut frame = vec![0x80 | opcode];
        match payload.len() {
            n if n < 126 => frame.push(0x80 | n as u8),
            n if n <= 0xffff => {
                frame.push(0x80 | 126);
                frame.extend((n as u16).to_be_bytes());
            }
            n => {
                frame.push(0x80 | 127);
                frame.extend((n as u64).to_be_bytes());
            }
        }
        let mask = mask();
        frame.extend(mask);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.writer.write_all(&frame)?;
        Ok(())
    }

    fn recv(&mut self) -> Result<String> {
        let mut message = Vec::new();
        loop {
            let mut head = [0u8; 2];
            self.reader.read_exact(&mut head)?;
            let len = match head[1] & 0x7f {
                126 => {
                    let mut n = [0u8; 2];
                    self.reader.read_exact(&mut n)?;
                    u16::from_be_bytes(n) as u64
                }
                127 => {
                    let mut n = [0u8; 8];
                    self.reader.read_exact(&mut n)?;
                    u64::from_be_bytes(n)
                }
                n => n as u64,
            };
            let mut mask = [0u8; 4];
            if head[1] & 0x80 != 0 {
                self.reader.read_exact(&mut mask)?;
            }
            let mut payload = vec![0u8; len as usize];
            self.reader.read_exact(&mut payload)?;
            if head[1] & 0x80 != 0 {
                for (i, b) in payload.iter_mut().enumerate() {
                    *b ^= mask[i % 4];
                }
            }
            match head[0] & 0x0f {
                0x8 => bail!("Chrome closed the DevTools connection"),
                0x9 => self.frame(0xA, &payload)?,
                0xA => {}
                _ => {
                    message.extend(payload);
                    if head[0] & 0x80 != 0 {
                        return Ok(String::from_utf8(message)?);
                    }
                }
            }
        }
    }
}

/// A frame's mask: it guards proxies against forged requests, so any changing value does on loopback.
fn mask() -> [u8; 4] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    (nanos ^ 0x9e37_79b9).to_le_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A server end that reads the client's handshake and masked frame, and answers with frames of its own.
    #[test]
    fn speaks_websocket_frames() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            assert!(request.starts_with("GET /devtools/browser/x HTTP/1.1"));
            let mut out = stream;
            out.write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\r\n")
                .unwrap();
            let mut head = [0u8; 2];
            reader.read_exact(&mut head).unwrap();
            assert_eq!(head[0], 0x81);
            assert_eq!(head[1] & 0x80, 0x80, "client frames are masked");
            let mut len = [0u8; 2];
            reader.read_exact(&mut len).unwrap();
            let len = u16::from_be_bytes(len) as usize;
            let mut mask = [0u8; 4];
            reader.read_exact(&mut mask).unwrap();
            let mut body = vec![0u8; len];
            reader.read_exact(&mut body).unwrap();
            let body: Vec<u8> = body
                .iter()
                .enumerate()
                .map(|(i, b)| b ^ mask[i % 4])
                .collect();
            // A ping, then the answer split in two frames, the second one long.
            out.write_all(&[0x89, 0x00]).unwrap();
            out.write_all(&[0x01, 0x02, b'o', b'k']).unwrap();
            let mut tail = vec![0x80, 126];
            tail.extend((body.len() as u16).to_be_bytes());
            tail.extend(&body);
            out.write_all(&tail).unwrap();
            let mut pong = [0u8; 2];
            reader.read_exact(&mut pong).unwrap();
            assert_eq!(pong[0], 0x8A);
        });
        let mut ws = Ws::connect(port, "/devtools/browser/x").unwrap();
        let text = "x".repeat(300);
        ws.send(&text).unwrap();
        assert_eq!(ws.recv().unwrap(), format!("ok{text}"));
        server.join().unwrap();
    }
}
