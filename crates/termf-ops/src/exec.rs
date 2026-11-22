//! Running the system's clients: what each kind of connection adds to a recipe's arguments, the SSH tunnel of
//! a database behind a server, and a run bounded in time and size (the client and what it started killed past
//! either). Secrets go through the client's environment or its standard input, never its arguments.

use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::Serialize;
use termf_vault::{Secret, Zeroizing};

use crate::recipe::quote;
use crate::{Connection, Kind, Mode, install_hint};

/// Kept past a recipe's byte limit, so that a value cut by the stop is dropped with its line, never half shown.
const MARGIN: usize = 4096;
const TUNNEL_WAIT: Duration = Duration::from_secs(15);
const TEST_TIMEOUT: u64 = 20;
/// A client is stopped once it printed this many times its byte limit (at least `MIN_READ`), whatever `keep`
/// kept of it.
const READ_FACTOR: u64 = 16;
const MIN_READ: u64 = 4 << 20;
/// Where a client reaches a database through its tunnel, once the tunnel has its port.
const PORT: &str = "<local port>";
/// A followed line longer than this arrives in pieces.
const FOLLOW_LINE: u64 = 16 << 10;

/// TermF's part of a command: the client, its connection options, and where its secrets go.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Base {
    pub program: String,
    pub args: Vec<String>,
    /// Variable → the vault's secret it holds.
    pub env: Vec<(String, String)>,
    pub vars: Vec<(String, String)>,
    /// curl's config on stdin: header → the vault's secret it carries.
    pub stdin: Vec<(String, String)>,
    pub tunnel: Option<Tunnel>,
}

/// The SSH tunnel TermF opens to a database's host and port, through a server.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tunnel {
    ssh: Vec<String>,
    host: String,
    port: u16,
}

impl Tunnel {
    fn args(&self, local: &str) -> Vec<String> {
        let forward = format!("127.0.0.1:{local}:{}:{}", self.host, self.port);
        let mut args: Vec<String> = ["-N", "-o", "ExitOnForwardFailure=yes", "-L", &forward]
            .map(String::from)
            .to_vec();
        args.extend(self.ssh.iter().cloned());
        args
    }

    pub fn shown(&self) -> String {
        let args: Vec<String> = self.args(PORT).iter().map(|a| quote(a)).collect();
        format!("ssh {}", args.join(" "))
    }
}

/// `ssh`'s options for a connection, then `--` and its destination.
fn ssh(conn: &Connection, timeout: u64) -> Vec<String> {
    let mut args: Vec<String> = [
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        format!("ConnectTimeout={}", timeout.clamp(5, 30)),
    ]
    .to_vec();
    if let Some(port) = conn.port {
        args.extend(["-p".into(), port.to_string()]);
    }
    if let Some(user) = &conn.user {
        args.extend(["-l".into(), user.clone()]);
    }
    if let Some(jump) = &conn.jump {
        args.extend(["-J".into(), jump.clone()]);
    }
    args.extend(["--".into(), conn.host.clone().unwrap_or_default()]);
    args
}

/// What TermF puts before a recipe's arguments on `conn`.
pub(crate) fn base(
    conn: &Connection,
    all: &[Connection],
    mode: Mode,
    timeout: u64,
) -> Result<Base> {
    let opt = |args: &mut Vec<String>, flag: &str, value: &Option<String>| {
        if let Some(v) = value {
            args.extend([flag.to_owned(), v.clone()]);
        }
    };
    let secret = |role: &str| conn.secrets.get(role).cloned();
    let mut base = Base {
        program: conn.kind.program().into(),
        ..Base::default()
    };
    match conn.kind {
        Kind::Ssh => base.args = ssh(conn, timeout),
        Kind::Postgres | Kind::Mysql => {
            let (host, port) = match &conn.via {
                Some(via) => {
                    let server = all
                        .iter()
                        .find(|c| &c.name == via && c.kind == Kind::Ssh)
                        .with_context(|| format!("{} goes through {via}, which is not an SSH connection of the project", conn.name))?;
                    let default = if conn.kind == Kind::Postgres {
                        5432
                    } else {
                        3306
                    };
                    base.tunnel = Some(Tunnel {
                        ssh: ssh(server, timeout),
                        host: conn.host.clone().unwrap_or_else(|| "127.0.0.1".into()),
                        port: conn.port.unwrap_or(default),
                    });
                    ("127.0.0.1".to_owned(), PORT.to_owned())
                }
                None => (
                    conn.host.clone().unwrap_or_default(),
                    conn.port.map(|p| p.to_string()).unwrap_or_default(),
                ),
            };
            let (user, db) = (
                conn.user.clone().unwrap_or_default(),
                conn.database.clone().unwrap_or_default(),
            );
            if conn.kind == Kind::Postgres {
                base.args = ["-X", "-q", "-w", "-v", "ON_ERROR_STOP=1", "-h", &host]
                    .map(String::from)
                    .to_vec();
                if !port.is_empty() {
                    base.args.extend(["-p".into(), port]);
                }
                base.args.extend(["-U".into(), user, "-d".into(), db]);
                base.vars.push(("PGCONNECT_TIMEOUT".into(), "10".into()));
                if mode == Mode::Read {
                    // The session refuses to write, whatever the user may: defense in depth (§12.3).
                    let options = format!(
                        "-c default_transaction_read_only=on -c statement_timeout={}",
                        timeout * 1000
                    );
                    base.vars.push(("PGOPTIONS".into(), options));
                }
                if let Some(s) = secret("password") {
                    base.env.push(("PGPASSWORD".into(), s));
                }
            } else {
                base.args = [
                    "--batch",
                    "--protocol=TCP",
                    "--connect-timeout=10",
                    "-h",
                    &host,
                ]
                .map(String::from)
                .to_vec();
                if !port.is_empty() {
                    base.args.extend(["-P".into(), port]);
                }
                base.args.extend(["-u".into(), user, "-D".into(), db]);
                if mode == Mode::Read {
                    base.args
                        .push("--init-command=SET SESSION TRANSACTION READ ONLY".into());
                }
                if let Some(s) = secret("password") {
                    base.env.push(("MYSQL_PWD".into(), s));
                }
            }
        }
        Kind::Kubernetes => {
            opt(&mut base.args, "--context", &conn.context);
            opt(&mut base.args, "--namespace", &conn.namespace);
            base.args.push(format!("--request-timeout={timeout}s"));
        }
        Kind::Cloudwatch => {
            opt(&mut base.args, "--profile", &conn.profile);
            opt(&mut base.args, "--region", &conn.region);
            base.args.push("--no-cli-pager".into());
            base.vars.push(("AWS_PAGER".into(), String::new()));
        }
        Kind::Gcp => {
            opt(&mut base.args, "--project", &conn.gcp_project);
            base.args.push("--quiet".into());
        }
        Kind::Loki => {
            base.vars
                .push(("LOKI_ADDR".into(), conn.url.clone().unwrap_or_default()));
            if let Some(s) = secret("token") {
                base.env.push(("LOKI_BEARER_TOKEN".into(), s));
            }
        }
        Kind::Datadog => {
            base.args = ["-sS", "--max-time", &timeout.to_string(), "-K", "-"]
                .map(String::from)
                .to_vec();
            for (header, role) in [("DD-API-KEY", "api_key"), ("DD-APPLICATION-KEY", "app_key")] {
                if let Some(s) = secret(role) {
                    base.stdin.push((header.into(), s));
                }
            }
        }
    }
    Ok(base)
}

/// What a client printed, before masking.
#[derive(Debug, Default)]
pub struct Raw {
    /// Whole lines, up to a little past the byte limit.
    pub stdout: String,
    pub stderr: String,
    pub exit: Option<i32>,
    /// Why it was stopped before it ended: too slow, or too much output.
    pub stopped: Option<String>,
    /// Bytes it printed on its output, as far as TermF read.
    pub bytes: u64,
    pub millis: u64,
    /// The secrets it was given: masked in its output too, even a reference's value the vault's masker lacks.
    pub used: Vec<Secret>,
}

/// One thing a connection's test looked at.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    /// `tool`, `secret`, `tunnel`, `connect`, `readonly` or `journal`.
    pub code: &'static str,
    pub ok: bool,
    /// It works, with something to know: a database user that can write, no journalctl.
    pub warn: bool,
    /// The program, the secret or the connection it is about.
    pub subject: String,
    /// What the client said (masked), or what to install.
    pub detail: String,
}

impl Check {
    fn new(code: &'static str, ok: bool, subject: &str, detail: impl Into<String>) -> Self {
        Self {
            code,
            ok,
            warn: false,
            subject: subject.into(),
            detail: detail.into(),
        }
    }
}

/// Resolves a secret of the vault by its name, for a connection's environment.
pub type Secrets<'a> = &'a dyn Fn(&str) -> Result<Secret>;

/// Runs the clients, with `path` as their `PATH` (`None`: the login shell's).
#[derive(Clone, Debug, Default)]
pub struct Runner {
    pub path: Option<OsString>,
}

struct Open {
    child: Child,
    port: u16,
    log: PathBuf,
}

impl Drop for Open {
    fn drop(&mut self) {
        #[cfg(unix)]
        termf_core::process::kill_group(self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(dir) = self.log.parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

impl Runner {
    pub fn new(path: Option<OsString>) -> Self {
        Self { path }
    }

    fn path(&self) -> Option<OsString> {
        self.path
            .clone()
            .or_else(|| termf_core::shell::login_path().map(OsString::from))
            .or_else(|| std::env::var_os("PATH"))
    }

    /// The client `PATH` resolves, or `None`.
    pub fn find(&self, program: &str) -> Option<PathBuf> {
        let exts: &[&str] = if cfg!(windows) {
            &[".exe", ".cmd", ".bat", ""]
        } else {
            &[""]
        };
        std::env::split_paths(&self.path()?).find_map(|dir| {
            exts.iter()
                .map(|ext| dir.join(format!("{program}{ext}")))
                .find(|p| p.is_file())
        })
    }

    fn program(&self, program: &str) -> Result<Command> {
        let found = self.find(program).ok_or_else(|| {
            anyhow!(
                "{program} is not installed or not on the PATH: install {}",
                install_hint(program)
            )
        })?;
        let mut cmd = Command::new(found);
        if let Some(path) = self.path() {
            cmd.env("PATH", path);
        }
        // Launched from the Finder or a desktop menu, TermF may lack the agent the user's keys are in.
        if std::env::var_os("SSH_AUTH_SOCK").is_none() {
            cmd.envs(termf_core::shell::login_vars(&["SSH_AUTH_SOCK"]));
        }
        Ok(cmd)
    }

    /// Opens the tunnel, and waits until its local port answers.
    fn tunnel(&self, tunnel: &Tunnel) -> Result<Open> {
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?
            .local_addr()?
            .port();
        let dir = termf_core::process::scratch_dir()?;
        let log = dir.join("ssh.err");
        let mut cmd = self.program("ssh")?;
        cmd.args(tunnel.args(&port.to_string()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log)?);
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut open = Open {
            child: cmd.spawn().context("cannot start ssh for the tunnel")?,
            port,
            log,
        };
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let deadline = Instant::now() + TUNNEL_WAIT;
        loop {
            if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
                return Ok(open);
            }
            if let Some(status) = open.child.try_wait()? {
                let said = fs::read_to_string(&open.log).unwrap_or_default();
                bail!(
                    "the SSH tunnel closed ({status}): {}",
                    termf_core::process::tail(&said)
                );
            }
            if Instant::now() > deadline {
                bail!(
                    "the SSH tunnel did not open within {} s",
                    TUNNEL_WAIT.as_secs()
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Runs `base` and `args` within `timeout`, keeping the lines that hold `keep`, up to `bytes` and a margin.
    fn launch(
        &self,
        base: &Base,
        args: &[String],
        keep: Option<&str>,
        limits: &crate::Limits,
        secrets: Secrets,
        opened: Option<Open>,
    ) -> Result<Raw> {
        let (bytes, timeout) = (limits.bytes, limits.timeout_s);
        let started = Instant::now();
        let mut used = Vec::new();
        let mut secret = |name: &str| -> Result<String> {
            let value = secrets(name)?;
            let text = value.expose().to_owned();
            used.push(value);
            Ok(text)
        };
        let mut cmd = self.program(&base.program)?;
        let mut input = Zeroizing::new(String::new());
        for (header, name) in &base.stdin {
            let value = secret(name)?;
            // A line break would end curl's quoted value and start an option of the secret's choosing.
            ensure!(
                !value.chars().any(char::is_control),
                "the secret {name} holds a line break or another control character: it cannot go in a header"
            );
            let line = format!("{header}: {value}")
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            input.push_str(&format!("header = \"{line}\"\n"));
        }
        for (var, name) in &base.env {
            cmd.env(var, secret(name)?);
        }
        cmd.envs(base.vars.iter().map(|(k, v)| (k, v)));
        let tunnel = match opened {
            Some(open) => Some(open),
            None => base.tunnel.as_ref().map(|t| self.tunnel(t)).transpose()?,
        };
        let port = tunnel
            .as_ref()
            .map(|t| t.port.to_string())
            .unwrap_or_default();
        cmd.args(
            base.args
                .iter()
                .chain(args)
                .map(|a| if a == PORT { port.as_str() } else { a }),
        );
        let cap = bytes + MARGIN;
        // What a client may print in all, the lines `keep` drops included.
        let read_cap = (cap as u64).saturating_mul(READ_FACTOR).max(MIN_READ);
        let cancel = AtomicBool::new(false);
        let (mut out, mut seen) = (String::new(), 0u64);
        let kept =
            |line: &str| keep.is_none_or(|k| line.to_lowercase().contains(&k.to_lowercase()));
        let mut on_line = |line: &str| {
            seen += line.len() as u64 + 1;
            if out.len() <= cap && kept(line) {
                out.push_str(line);
                out.push('\n');
            }
            if out.len() > cap || seen > read_cap {
                cancel.store(true, Ordering::SeqCst);
            }
        };
        let done = termf_core::process::stream_until(
            cmd,
            input.as_bytes(),
            Duration::from_secs(timeout),
            &mut on_line,
            &cancel,
            cap,
        );
        drop(tunnel);
        let mut raw = Raw {
            millis: started.elapsed().as_millis() as u64,
            used,
            ..Raw::default()
        };
        match done {
            Ok((status, stdout, stderr)) => {
                raw.exit = status.code();
                raw.bytes = stdout.len() as u64;
                raw.stderr = String::from_utf8_lossy(&stderr).into_owned();
                let text = String::from_utf8_lossy(&stdout);
                let mut kept_lines = String::new();
                for line in text.lines().filter(|l| kept(l)) {
                    if kept_lines.len() + line.len() + 1 > cap {
                        raw.stopped = Some(format!("stopped past {bytes} bytes"));
                        break;
                    }
                    kept_lines.push_str(line);
                    kept_lines.push('\n');
                }
                raw.stdout = kept_lines;
            }
            Err(e) => {
                raw.bytes = seen;
                raw.stdout = out;
                raw.stopped = Some(match cancel.load(Ordering::SeqCst) {
                    true if seen > read_cap => format!("stopped after reading {read_cap} bytes"),
                    true => format!("stopped past {bytes} bytes"),
                    false => format!("{e:#}"),
                });
            }
        }
        Ok(raw)
    }

    /// Starts a recipe's plan that runs until stopped: `lines` gets its output a few lines at a time (those
    /// `keep` keeps), `end` its exit and the end of its error output once it stops on its own.
    pub fn follow(
        &self,
        plan: &crate::Plan,
        secrets: Secrets,
        mut lines: impl FnMut(Vec<String>) + Send + 'static,
        end: impl FnOnce(Option<i32>, String) + Send + 'static,
    ) -> Result<Following> {
        // shortcut: no tunnel nor secret on stdin, which no log client needs; add them for a followed query.
        ensure!(
            plan.base.tunnel.is_none() && plan.base.stdin.is_empty(),
            "{} cannot be followed: it needs a tunnel or its input",
            plan.recipe
        );
        let mut cmd = self.program(&plan.base.program)?;
        for (var, name) in &plan.base.env {
            cmd.env(var, secrets(name)?.expose());
        }
        cmd.envs(plan.base.vars.iter().map(|(k, v)| (k, v)))
            .args(plan.base.args.iter().chain(&plan.args))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = cmd
            .spawn()
            .with_context(|| format!("cannot start {}", plan.base.program))?;
        let (stdout, mut stderr) = (child.stdout.take(), child.stderr.take());
        let child = Arc::new(Mutex::new(child));
        let errors = std::thread::spawn(move || {
            // Read to its end, keeping only the last of it: a full pipe would stop the client.
            let (mut said, mut buf) = (Vec::new(), [0u8; 8192]);
            if let Some(err) = stderr.as_mut() {
                while let Ok(n) = err.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    said.extend_from_slice(&buf[..n]);
                    if said.len() > 64 << 10 {
                        said.drain(..said.len() - (16 << 10));
                    }
                }
            }
            let said = String::from_utf8_lossy(&said).into_owned();
            termf_core::process::tail(&said)
        });
        let (tx, rx) = mpsc::channel::<String>();
        let keep = plan.keep.as_ref().map(|k| k.to_lowercase());
        std::thread::spawn(move || {
            let Some(out) = stdout else { return };
            let mut reader = BufReader::new(out);
            let mut line = Vec::new();
            loop {
                line.clear();
                // A line without end arrives in pieces, never held whole.
                match (&mut reader).take(FOLLOW_LINE).read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let text = String::from_utf8_lossy(&line).trim_end().to_owned();
                if keep
                    .as_ref()
                    .is_none_or(|k| text.to_lowercase().contains(k))
                    && tx.send(text).is_err()
                {
                    break;
                }
            }
        });
        let waited = Arc::clone(&child);
        std::thread::spawn(move || {
            let mut batch = Vec::new();
            loop {
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(line) => {
                        batch.push(line);
                        if batch.len() < 500 {
                            continue;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                if !batch.is_empty() {
                    lines(std::mem::take(&mut batch));
                }
            }
            if !batch.is_empty() {
                lines(batch);
            }
            let exit = loop {
                match waited.lock().unwrap().try_wait() {
                    Ok(Some(status)) => break status.code(),
                    Ok(None) => {}
                    Err(_) => break None,
                }
                std::thread::sleep(Duration::from_millis(50));
            };
            end(exit, errors.join().unwrap_or_default());
        });
        Ok(Following { child })
    }

    /// Runs a recipe's plan; its output is still to mask ([`crate::redact::mask`]).
    pub fn run(&self, plan: &crate::Plan, secrets: Secrets) -> Result<Raw> {
        self.launch(
            &plan.base,
            &plan.args,
            plan.keep.as_deref(),
            &plan.limits,
            secrets,
            None,
        )
    }

    /// What works and what is missing: the client, the secrets, the tunnel, reaching the target, and for a
    /// database whether its user can write. Errors only when the vault must be opened (or prod confirmed) first.
    pub fn test(
        &self,
        conn: &Connection,
        all: &[Connection],
        secrets: Secrets,
    ) -> Result<Vec<Check>> {
        conn.check()?;
        let mut checks = Vec::new();
        let program = conn.kind.program();
        let mut tools = vec![program];
        if conn.via.is_some() {
            tools.insert(0, "ssh");
        }
        for tool in tools {
            match self.find(tool) {
                Some(path) => checks.push(Check::new("tool", true, tool, path.to_string_lossy())),
                None => {
                    checks.push(Check::new("tool", false, tool, install_hint(tool)));
                    return Ok(checks);
                }
            }
        }
        let mut values = Vec::new();
        for name in conn.secrets.values() {
            match secrets(name) {
                Ok(v) => {
                    values.push(v);
                    checks.push(Check::new("secret", true, name, ""));
                }
                Err(e) if vault_first(&e) => return Err(e),
                Err(e) => {
                    checks.push(Check::new("secret", false, name, format!("{e:#}")));
                    return Ok(checks);
                }
            }
        }
        let known = |name: &str| -> Result<Secret> {
            let at = conn
                .secrets
                .values()
                .position(|n| n == name)
                .context("unknown secret")?;
            Ok(Secret::new(values[at].expose().to_owned()))
        };
        let base = base(conn, all, Mode::Read, TEST_TIMEOUT)?;
        let probe: Vec<String> = match conn.kind {
            Kind::Ssh => vec!["echo termf-ok; command -v journalctl || true".into()],
            Kind::Postgres => vec![
                "-At".into(),
                "-F".into(),
                "|".into(),
                "-c".into(),
                "SELECT 'termf-ok', (r.rolsuper OR r.rolcreaterole OR r.rolcreatedb \
                 OR has_database_privilege(current_database(), 'CREATE') \
                 OR EXISTS (SELECT 1 FROM information_schema.role_table_grants g \
                 WHERE g.grantee = current_user AND g.privilege_type IN ('INSERT','UPDATE','DELETE','TRUNCATE'))) \
                 FROM pg_roles r WHERE r.rolname = current_user"
                    .into(),
            ],
            Kind::Mysql => vec!["-e".into(), "SELECT 'termf-ok'; SHOW GRANTS FOR CURRENT_USER()".into()],
            Kind::Kubernetes => ["auth", "can-i", "list", "pods"].map(String::from).to_vec(),
            Kind::Cloudwatch => vec![
                "logs".into(),
                "describe-log-groups".into(),
                "--log-group-name-prefix".into(),
                conn.log_group.clone().unwrap_or_default(),
                "--max-items".into(),
                "1".into(),
                "--output".into(),
                "text".into(),
            ],
            Kind::Gcp => ["logging", "read", "--limit", "1", "--freshness", "1h", "--format", "value(timestamp)"]
                .map(String::from)
                .to_vec(),
            Kind::Loki => ["labels", "--quiet"].map(String::from).to_vec(),
            Kind::Datadog => vec![format!(
                "https://api.{}/api/v1/validate",
                conn.site.as_deref().unwrap_or("datadoghq.com")
            )],
        };
        let target = conn.target(all);
        // The tunnel the check opened is the one the probe goes through.
        let mut opened = None;
        if let Some(t) = &base.tunnel {
            match self.tunnel(t) {
                Ok(open) => {
                    opened = Some(open);
                    checks.push(Check::new(
                        "tunnel",
                        true,
                        conn.via.as_deref().unwrap_or_default(),
                        "",
                    ));
                }
                Err(e) => {
                    checks.push(Check::new(
                        "tunnel",
                        false,
                        conn.via.as_deref().unwrap_or_default(),
                        format!("{e:#}"),
                    ));
                    return Ok(checks);
                }
            }
        }
        let limits = crate::Limits {
            bytes: 64 << 10,
            timeout_s: TEST_TIMEOUT,
            ..crate::Limits::default()
        };
        let raw = self.launch(&base, &probe, None, &limits, &known, opened)?;
        let masked = |text: &str| crate::redact::mask_with(text, &raw.used);
        let reached = raw.exit == Some(0)
            && match conn.kind {
                Kind::Ssh | Kind::Postgres | Kind::Mysql => raw.stdout.contains("termf-ok"),
                Kind::Kubernetes => raw.stdout.trim() == "yes",
                Kind::Datadog => raw.stdout.contains("\"valid\":true"),
                _ => true,
            };
        let said = [
            raw.stderr.trim(),
            raw.stdout.trim(),
            raw.stopped.as_deref().unwrap_or_default(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
        checks.push(Check::new(
            "connect",
            reached,
            &target,
            if reached {
                String::new()
            } else {
                masked(&termf_core::process::tail(&said))
            },
        ));
        if !reached {
            return Ok(checks);
        }
        match conn.kind {
            Kind::Ssh => {
                let journal = raw
                    .stdout
                    .lines()
                    .any(|l| l.trim_end().ends_with("journalctl"));
                checks.push(Check {
                    warn: !journal,
                    ..Check::new("journal", journal, &target, "")
                });
            }
            Kind::Postgres | Kind::Mysql => {
                let writes = if conn.kind == Kind::Postgres {
                    raw.stdout.lines().any(|l| l.trim() == "termf-ok|t")
                } else {
                    mysql_writes(&raw.stdout)
                };
                checks.push(Check {
                    warn: writes,
                    ..Check::new(
                        "readonly",
                        !writes,
                        conn.user.as_deref().unwrap_or_default(),
                        "",
                    )
                });
            }
            _ => {}
        }
        Ok(checks)
    }
}

/// Whether `SHOW GRANTS` gives a privilege that writes: only the list between `GRANT` and ` ON ` is read, never
/// the names of databases, tables or columns after it.
fn mysql_writes(grants: &str) -> bool {
    const WRITES: &[&str] = &[
        "ALL", "INSERT", "UPDATE", "DELETE", "DROP", "ALTER", "CREATE", "SUPER", "INDEX",
        "TRIGGER", "EVENT",
    ];
    grants.lines().any(|line| {
        let upper = line.trim().to_uppercase();
        let Some(rest) = upper.strip_prefix("GRANT ") else {
            return false;
        };
        let Some((privileges, _)) = rest.split_once(" ON ") else {
            return false;
        };
        privileges
            .split(',')
            .filter_map(|p| {
                p.split(|c: char| c.is_whitespace() || c == '(')
                    .find(|w| !w.is_empty())
            })
            .any(|p| WRITES.contains(&p))
    })
}

/// A recipe followed until stopped: dropping it kills its client and what that started.
pub struct Following {
    child: Arc<Mutex<Child>>,
}

impl Following {
    /// Kills the client and its process group, unless it already ended: once waited for, its number (and its
    /// group's) may be another program's. True when it signalled.
    pub fn stop(&self) -> bool {
        let mut child = self.child.lock().unwrap();
        // Under the lock the follower cannot wait for it in between: alive here means not reaped.
        if !matches!(child.try_wait(), Ok(None)) {
            return false;
        }
        #[cfg(unix)]
        termf_core::process::kill_group(child.id());
        let _ = child.kill();
        true
    }
}

impl Drop for Following {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The vault must be opened, or prod confirmed, before anything else: the app asks for it.
pub fn vault_first(e: &anyhow::Error) -> bool {
    e.downcast_ref::<termf_vault::Locked>().is_some()
        || e.downcast_ref::<termf_vault::NeedsPassword>().is_some()
}

#[cfg(test)]
mod tests {
    use super::mysql_writes;

    #[test]
    fn reads_only_the_privileges_of_mysql_grants() {
        for read in [
            "Grants for ro@%",
            "GRANT USAGE ON *.* TO `ro`@`%`",
            "GRANT SELECT ON `updates_db`.* TO `ro`@`%`",
            "GRANT SELECT (id, created_at) ON `app`.`insert_log` TO `ro`@`%`",
            "GRANT `reader`@`%` TO `ro`@`%`",
        ] {
            assert!(!mysql_writes(read), "{read}");
        }
        for write in [
            "GRANT SELECT, INSERT ON `app`.* TO `app`@`%`",
            "GRANT ALL PRIVILEGES ON *.* TO `root`@`localhost` WITH GRANT OPTION",
            "GRANT UPDATE(status) ON `app`.`orders` TO `app`@`%`",
            "grant create view on app.* to app",
        ] {
            assert!(mysql_writes(write), "{write}");
        }
    }
}
