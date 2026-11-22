//! Running the system's clients: what each kind of connection adds to a recipe's arguments, the SSH tunnel of
//! a database behind a server, and a run bounded in time and size (the client and what it started killed past
//! either). Secrets go through the client's environment or its standard input, never its arguments.

use std::ffi::OsString;
use std::fs;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use termf_vault::{Secret, Zeroizing};

use crate::recipe::quote;
use crate::{Connection, Kind, Mode, install_hint};

/// Kept past a recipe's byte limit, so that a value cut by the stop is dropped with its line, never half shown.
const MARGIN: usize = 4096;
const TUNNEL_WAIT: Duration = Duration::from_secs(15);
const TEST_TIMEOUT: u64 = 20;
/// Where a client reaches a database through its tunnel, once the tunnel has its port.
const PORT: &str = "<local port>";

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
        bytes: usize,
        timeout: u64,
        secrets: Secrets,
    ) -> Result<Raw> {
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
            let line = format!("{header}: {value}")
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            input.push_str(&format!("header = \"{line}\"\n"));
        }
        for (var, name) in &base.env {
            cmd.env(var, secret(name)?);
        }
        cmd.envs(base.vars.iter().map(|(k, v)| (k, v)));
        let tunnel = base.tunnel.as_ref().map(|t| self.tunnel(t)).transpose()?;
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
            if out.len() > cap {
                cancel.store(true, Ordering::SeqCst);
            }
        };
        let done = termf_core::process::stream_until(
            cmd,
            input.as_bytes(),
            Duration::from_secs(timeout),
            &mut on_line,
            &cancel,
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
                    true => format!("stopped past {bytes} bytes"),
                    false => format!("{e:#}"),
                });
            }
        }
        Ok(raw)
    }

    /// Runs a recipe's plan; its output is still to mask ([`crate::redact::mask`]).
    pub fn run(&self, plan: &crate::Plan, secrets: Secrets) -> Result<Raw> {
        self.launch(
            &plan.base,
            &plan.args,
            plan.keep.as_deref(),
            plan.limits.bytes,
            plan.limits.timeout_s,
            secrets,
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
        if let Some(t) = &base.tunnel {
            match self.tunnel(t) {
                Ok(_) => checks.push(Check::new(
                    "tunnel",
                    true,
                    conn.via.as_deref().unwrap_or_default(),
                    "",
                )),
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
        let raw = self.launch(&base, &probe, None, 64 << 10, TEST_TIMEOUT, &known)?;
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
                    raw.stdout.lines().filter(|l| l.contains("GRANT")).any(|l| {
                        let grant = l.to_uppercase();
                        [
                            "ALL PRIVILEGES",
                            "INSERT",
                            "UPDATE",
                            "DELETE",
                            "DROP",
                            "ALTER",
                            "CREATE",
                            "SUPER",
                        ]
                        .iter()
                        .any(|w| grant.contains(w))
                    })
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

/// The vault must be opened, or prod confirmed, before anything else: the app asks for it.
pub fn vault_first(e: &anyhow::Error) -> bool {
    e.downcast_ref::<termf_vault::Locked>().is_some()
        || e.downcast_ref::<termf_vault::NeedsPassword>().is_some()
}
