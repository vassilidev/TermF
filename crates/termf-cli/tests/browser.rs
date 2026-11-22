//! The browser of `termf mcp`: what it refuses before any Chrome starts, the reaper that cleans up after it
//! however it ends, and, with the Chrome of the machine (`cargo test -p termf-cli --test browser -- --ignored
//! real_chrome`), its tools on a page of a throwaway dev server.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
#[cfg(unix)]
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Serves the files of its folder on 127.0.0.1, logs each path asked, and `/redirect?to=<url>` redirects.
const SERVE: &str = r#"
import http.server, sys
port, log = int(sys.argv[1]), sys.argv[2]
class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        with open(log, 'a') as f:
            f.write(self.path + '\n')
        if self.path.startswith('/redirect?to='):
            self.send_response(302)
            self.send_header('Location', self.path.split('=', 1)[1])
            self.end_headers()
            return
        super().do_GET()
    def log_message(self, *args):
        pass
http.server.ThreadingHTTPServer(('127.0.0.1', port), Handler).serve_forever()
"#;

/// A project `acme` whose repository `web` serves `index.html` with `dev` on port 5341.
struct Project {
    root: PathBuf,
    repo: PathBuf,
    config: PathBuf,
    data: PathBuf,
}

impl Project {
    fn new(name: &str, dev: bool) -> Project {
        let root =
            std::env::temp_dir().join(format!("termf-browser-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (repo, config, data) = (root.join("web"), root.join("config"), root.join("data"));
        for dir in [&repo, &config.join("projects"), &data, &root.join("home")] {
            fs::create_dir_all(dir).unwrap();
        }
        let repo = repo.canonicalize().unwrap();
        fs::write(root.join("serve.py"), SERVE).unwrap();
        let dev = match dev {
            true => format!(
                "dev = 'python3 {} {{{{ port }}}} {}'\n",
                root.join("serve.py").display(),
                root.join("dev.log").display()
            ),
            false => String::new(),
        };
        fs::write(
            config.join("projects/acme.toml"),
            format!(
                "name = \"Acme\"\n\n[[repos]]\npath = '{}'\nname = \"web\"\n{dev}",
                repo.display()
            ),
        )
        .unwrap();
        // The worktree keeps the port it was given: one of this test's.
        fs::write(
            data.join("ports.json"),
            json!({ repo.to_string_lossy(): { "browser": 5341 } }).to_string(),
        )
        .unwrap();
        Project {
            root,
            repo,
            config,
            data,
        }
    }

    fn server(&self) -> Mcp {
        let mut child = Command::new(env!("CARGO_BIN_EXE_termf"))
            .arg("mcp")
            .current_dir(&self.repo)
            .env("HOME", self.root.join("home"))
            .env("TERMF_CONFIG_DIR", &self.config)
            .env("TERMF_DATA_DIR", &self.data)
            .env(
                "TERMF_MCP_TOOLS",
                "browser_open,browser_snapshot,browser_click,browser_fill,browser_wait,browser_console,browser_network,browser_screenshot",
            )
            .env_remove("TERMF_SOCKET")
            .env_remove("TERMF_MCP_TOKEN")
            .env_remove("TERMF_MCP_SESSION")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Mcp {
            child,
            stdin,
            stdout,
            next: 0,
        }
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: i64,
}

impl Mcp {
    fn ask(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let msg = json!({"jsonrpc": "2.0", "id": self.next, "method": method, "params": params});
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    /// The tool's text, and whether it is an error.
    fn call(&mut self, name: &str, arguments: Value) -> (String, bool) {
        let reply = self.ask("tools/call", json!({"name": name, "arguments": arguments}));
        let result = &reply["result"];
        let text = result["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        (text, result["isError"] == json!(true))
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The number in `[ref=…]` of the first line of `snapshot` that has `has`.
fn reference(snapshot: &str, has: &str) -> String {
    let line = snapshot
        .lines()
        .find(|l| l.contains(has))
        .unwrap_or_else(|| panic!("no {has} in\n{snapshot}"));
    let start = line.find("[ref=").unwrap() + 5;
    line[start..line[start..].find(']').unwrap() + start].to_owned()
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    let out = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let stat = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    !stat.is_empty() && !stat.starts_with('Z')
}

#[cfg(unix)]
fn until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn refuses_other_origins_before_any_chrome_starts() {
    let project = Project::new("refuses", false);
    let mut mcp = project.server();
    let init = mcp.ask("initialize", json!({"protocolVersion": "2025-06-18"}));
    let instructions = init["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("throwaway profile") && instructions.contains("untrusted data"));
    let list = mcp.ask("tools/list", json!({}));
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 8);

    let (text, error) = mcp.call("browser_snapshot", json!({}));
    assert!(error && text.contains("no page yet"), "{text}");
    let (text, error) = mcp.call("browser_open", json!({"url": "https://example.com/login"}));
    assert!(error, "{text}");
    assert!(
        text.contains("https://example.com:443 is not an allowed origin")
            && text.contains("[browser] section of the project"),
        "{text}"
    );
    for url in [
        "file:///etc/passwd",
        "http://localhost@example.com/",
        "chrome://settings",
    ] {
        let (text, error) = mcp.call("browser_open", json!({"url": url}));
        assert!(error, "{url}: {text}");
    }
    let (text, error) = mcp.call("browser_open", json!({"path": "/"}));
    assert!(
        error && text.contains("no dev server known for the repository web"),
        "{text}"
    );
    let other = mcp.ask("tools/call", json!({"name": "team_runs", "arguments": {}}));
    assert_eq!(
        other["error"]["code"], -32602,
        "not a tool of this session: {other}"
    );
}

#[cfg(unix)]
#[test]
fn the_reaper_cleans_up_once_its_server_is_gone() {
    let root = std::env::temp_dir().join(format!("termf-reaper-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("profile")).unwrap();
    fs::write(root.join("profile/Cookies"), "x").unwrap();
    // A stand-in for Chrome: its own process group, and a child of its own.
    let mut browser = Command::new("sh");
    browser.args([
        "-c",
        &format!(
            "sleep 60 & echo $! > {}; wait",
            root.join("child").display()
        ),
    ]);
    std::os::unix::process::CommandExt::process_group(&mut browser, 0);
    let mut browser = browser.spawn().unwrap();
    until("the child started", || root.join("child").exists());
    let child: u32 = fs::read_to_string(root.join("child"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    let mut reaper = Command::new(env!("CARGO_BIN_EXE_termf"))
        .args(["mcp", "--reaper"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = reaper.stdin.take().unwrap();
    writeln!(input, "pid {}", browser.id()).unwrap();
    writeln!(input, "dir {}", root.join("profile").display()).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        alive(browser.id()) && alive(child),
        "nothing happens while its server lives"
    );
    drop(input);
    assert!(reaper.wait().unwrap().success());
    let _ = browser.wait();
    until("Chrome's stand-in and its child are gone", || !alive(child));
    assert!(!root.join("profile").exists(), "the profile is removed");
    let _ = fs::remove_dir_all(&root);
}

/// With the Chrome of the machine: `cargo test -p termf-cli --test browser -- --ignored real_chrome`.
#[cfg(unix)]
#[test]
#[ignore]
fn real_chrome_drives_a_page_and_dies_with_its_server() {
    let project = Project::new("chrome", true);
    let elsewhere = "http://[::ffff:127.0.0.1]:5342";
    // As Chrome writes it.
    let seen = "http://[::ffff:7f00:1]:5342";
    fs::write(
        project.repo.join("index.html"),
        format!(
            "<!doctype html><title>Invoices</title><h1>Invoices</h1>\
             <input id=\"q\" aria-label=\"Search\"><button onclick=\"const v = document.getElementById('q').value; \
             console.log('saved ' + v); document.getElementById('out').textContent = 'Saved ' + v; confirm('Sure?')\">Save</button>\
             <p id=\"out\"></p><a href=\"/redirect?to={elsewhere}/secret\">Away</a><img src=\"{elsewhere}/pixel.png\" alt=\"tracker\">\
             <script>console.error('boom'); fetch('/api/missing');</script>"
        ),
    )
    .unwrap();
    // A server the page could reach if Chrome let it: it must never hear from it.
    let _other = Stopped(
        Command::new("python3")
            .args([
                project.root.join("serve.py").to_str().unwrap(),
                "5342",
                project.root.join("other.log").to_str().unwrap(),
            ])
            .current_dir(&project.root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut mcp = project.server();
    mcp.ask("initialize", json!({"protocolVersion": "2025-06-18"}));

    let (page, error) = mcp.call("browser_open", json!({"path": "/"}));
    assert!(!error, "{page}");
    println!("{page}");
    assert!(
        page.contains("Opened http://localhost:5341/ (desktop, light)"),
        "{page}"
    );
    assert!(
        page.contains("untrusted data") && page.contains("heading \"Invoices\""),
        "{page}"
    );
    assert!(
        page.contains(&format!("Blocked by TermF, origin not allowed")),
        "{page}"
    );
    let field = reference(&page, "textbox \"Search\"");
    let button = reference(&page, "button \"Save\"");

    let (text, error) = mcp.call("browser_fill", json!({"ref": field, "value": "acme"}));
    assert!(!error, "{text}");
    let (text, error) = mcp.call("browser_click", json!({"ref": button}));
    assert!(!error, "{text}");
    assert!(
        text.contains("[dialog] confirm \"Sure?\" accepted by TermF"),
        "{text}"
    );
    let (text, error) = mcp.call("browser_wait", json!({"text": "Saved acme", "timeout": 5}));
    assert!(!error, "{text}");
    let (snapshot, _) = mcp.call("browser_snapshot", json!({}));
    assert!(snapshot.contains("value=\"acme\""), "{snapshot}");

    let (console, _) = mcp.call("browser_console", json!({}));
    assert!(
        console.contains("[error] boom") && console.contains("[log] saved acme"),
        "{console}"
    );
    let (network, _) = mcp.call("browser_network", json!({}));
    assert!(
        network.contains("GET 404 Fetch http://localhost:5341/api/missing"),
        "{network}"
    );
    assert!(
        network.contains(&format!(
            "BLOCKED by TermF (origin not allowed) Image {seen}/pixel.png"
        )),
        "{network}"
    );

    let shot = mcp.ask(
        "tools/call",
        json!({"name": "browser_screenshot", "arguments": {"full_page": true}}),
    );
    let image = &shot["result"]["content"][0];
    assert_eq!(image["mimeType"], "image/png", "{shot}");
    assert!(image["data"].as_str().unwrap().starts_with("iVBOR"));

    let (page, _) = mcp.call("browser_snapshot", json!({}));
    let away = reference(&page, "link \"Away\"");
    let (text, _) = mcp.call("browser_click", json!({"ref": away}));
    assert!(
        text.contains(&format!("{seen}/secret")),
        "a redirect cannot leave either: {text}"
    );
    let (text, error) = mcp.call("browser_open", json!({"url": "https://example.com"}));
    assert!(error && text.contains("not an allowed origin"), "{text}");
    let heard = fs::read_to_string(project.root.join("other.log")).unwrap_or_default();
    assert!(heard.is_empty(), "the other server heard: {heard}");

    // Claude Code kills its MCP servers: Chrome, its profile and the dev server go with it all the same.
    let prefix = format!("termf-run-{}-", mcp.child.id());
    let profiles: Vec<PathBuf> = fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&prefix)
        })
        .collect();
    assert_eq!(
        profiles.len(),
        2,
        "Chrome's profile and the dev server's log: {profiles:?}"
    );
    let chrome = chrome_pids(&profiles);
    assert!(!chrome.is_empty(), "Chrome runs on its throwaway profile");
    mcp.child.kill().unwrap();
    mcp.child.wait().unwrap();
    until("Chrome is gone", || chrome_pids(&profiles).is_empty());
    until("its profile is removed", || {
        profiles.iter().all(|p| !p.exists())
    });
    until("the dev server is stopped", || {
        std::net::TcpStream::connect(("127.0.0.1", 5341)).is_err()
    });
}

/// Stopped even when an assertion fails.
struct Stopped(Child);

impl Drop for Stopped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(unix)]
fn chrome_pids(profiles: &[PathBuf]) -> Vec<u32> {
    let out = Command::new("ps")
        .args(["-axo", "pid=,command="])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| {
            profiles
                .iter()
                .any(|p| l.contains(&format!("--user-data-dir={}", p.display())))
        })
        .filter_map(|l| l.split_whitespace().next()?.parse().ok())
        .collect()
}

/// Claude Code on Haiku, as the team's `qa`, with the Chrome of the machine (a few cents): `cargo test -p termf-cli
/// --test browser -- --ignored real_claude --nocapture`.
#[cfg(unix)]
#[test]
#[ignore]
fn real_claude_checks_a_page_with_its_browser() {
    use termf_agents::{AgentEvent, Agents, SessionSpec, State};

    let project = Project::new("claude", true);
    fs::write(
        project.repo.join("index.html"),
        "<!doctype html><title>Invoices</title><h1>Invoices</h1>\
         <button onclick=\"console.log('exported ' + 3 + ' invoices')\">Export CSV</button>",
    )
    .unwrap();
    fs::write(
        project.config.join("termf.toml"),
        "[agents]\nclaude = \"claude\"\n",
    )
    .unwrap();
    let agents = Agents::build(
        project.config.clone(),
        project.data.clone(),
        Vec::new(),
        |_| {},
    );
    let home = project.root.join("home");
    agents.serve_mcp(
        move |id| {
            Ok(
                json!({"type": "stdio", "command": env!("CARGO_BIN_EXE_termf"), "args": ["mcp"],
                      "env": {"TERMF_MCP_SESSION": id.to_string(), "HOME": home}}),
            )
        },
        |_| {},
    );
    let spec = SessionSpec {
        cwd: project.repo.clone(),
        role: Some("qa".into()),
        model: Some("haiku".into()),
        effort: Some("low".into()),
        budget: Some(0.40),
        ..SessionSpec::default()
    };
    let id = agents.create(spec).unwrap().id;
    agents
        .send(
            id,
            "Use only the browser tools: open the home page of the dev server, click Export CSV, then tell me \
             what the console says.",
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(240);
    let ended = || {
        agents
            .events(id, 0)
            .unwrap()
            .iter()
            .any(|e| matches!(e.event, AgentEvent::TurnEnd { .. }))
    };
    while !ended() || agents.session(id).unwrap().state == State::Working {
        assert!(
            Instant::now() < deadline,
            "{:#?}",
            agents.events(id, 0).unwrap()
        );
        std::thread::sleep(Duration::from_millis(500));
    }
    let events = agents.events(id, 0).unwrap();
    let used: Vec<&str> = events
        .iter()
        .filter_map(|e| match &e.event {
            AgentEvent::ToolUse { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let answer = events
        .iter()
        .rev()
        .find_map(|e| match &e.event {
            AgentEvent::TurnEnd { result, .. } => result.clone(),
            _ => None,
        })
        .unwrap_or_default();
    println!(
        "tools: {used:?}\nanswer: {answer}\ncost: ${:.4}",
        agents.session(id).unwrap().cost
    );
    for tool in ["browser_open", "browser_click", "browser_console"] {
        assert!(
            used.contains(&format!("mcp__termf__{tool}").as_str()),
            "{used:?}"
        );
    }
    assert!(
        !events
            .iter()
            .any(|e| matches!(e.event, AgentEvent::Permission { .. })),
        "the role's browser needs no approval"
    );
    assert!(answer.contains("exported 3 invoices"), "{answer}");
    let _ = agents.stop(id);
}
