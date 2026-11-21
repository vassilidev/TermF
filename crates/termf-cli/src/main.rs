use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use interprocess::local_socket::{GenericFilePath, Stream, prelude::*};
use serde_json::Value;
use termf_core::ipc::{self, Call, Reply, Request, SessionSummary};

const USAGE: &str = "\
usage:
  termf ls                              list sessions
  termf open <path> [--agent <name>]    open a session (a shell without --agent)
  termf say <session> <text>...         type text then Enter in a session (id or part of its title)
  termf watch                           print status changes
  termf hook <event>                    forward a Claude Code hook (JSON on stdin)";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    if let ["hook", event] = args.as_slice() {
        hook(event);
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("termf: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[&str]) -> Result<()> {
    match args {
        ["ls"] => {
            let sessions: Vec<SessionSummary> =
                serde_json::from_value(Client::connect(None)?.call(Call::List)?)?;
            for s in sessions {
                let state = match s.status {
                    Some(status) => status.as_str(),
                    None if s.running => "running",
                    None => "exited",
                };
                let remote = if s.remote_control {
                    "  [remote control]"
                } else {
                    ""
                };
                println!("{:>4}  {state:<8}  {}{remote}", s.id, s.title);
            }
        }
        ["open", path, rest @ ..] => {
            let agent = match rest {
                [] => None,
                ["--agent", name] => Some(name.to_string()),
                _ => bail!(USAGE),
            };
            let path = std::path::absolute(path)?.to_string_lossy().into_owned();
            let s: SessionSummary =
                serde_json::from_value(Client::connect(None)?.call(Call::Open { path, agent })?)?;
            println!("{:>4}  {}", s.id, s.title);
        }
        ["say", session, text @ ..] if !text.is_empty() => {
            Client::connect(None)?.call(Call::Say {
                session: session.to_string(),
                text: text.join(" "),
            })?;
        }
        ["watch"] => {
            let mut client = Client::connect(None)?;
            client.call(Call::Watch)?;
            loop {
                match client.read()? {
                    Reply::Event(e) => {
                        println!("{:>4}  {:<8}  {}", e.id, e.status.as_str(), e.title)
                    }
                    Reply::Error(e) => bail!(e),
                    Reply::Ok(_) => {}
                }
            }
        }
        ["-h" | "--help" | "help"] => println!("{USAGE}"),
        _ => bail!(USAGE),
    }
    Ok(())
}

/// Never fails and returns within two seconds: a hook must not get in the agent's way, even when TermF is gone.
fn hook(event: &str) {
    let event = event.to_owned();
    let (done, wait) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = forward_hook(event);
        let _ = done.send(());
    });
    let _ = wait.recv_timeout(Duration::from_secs(2));
}

fn forward_hook(event: String) -> Result<()> {
    // Not started by TermF: nothing to report to.
    let session = env::var(ipc::ENV_SESSION)?.parse()?;
    let mut payload = String::new();
    std::io::stdin().read_to_string(&mut payload)?;
    let payload = serde_json::from_str(&payload).unwrap_or(Value::Null);
    // Waiting for the reply keeps consecutive hooks in order.
    Client::connect(Some(env::var(ipc::ENV_TOKEN)?))?.call(Call::Hook {
        session,
        event,
        payload,
    })?;
    Ok(())
}

struct Client {
    conn: BufReader<Stream>,
    token: String,
}

impl Client {
    /// `token: None` uses the user's token from the data dir. Sessions started by TermF get the socket in their environment.
    fn connect(token: Option<String>) -> Result<Self> {
        let data_dir = termf_core::default_data_dir()?;
        let socket = env::var(ipc::ENV_SOCKET).unwrap_or_else(|_| ipc::socket_name(&data_dir));
        let token = match token {
            Some(token) => token,
            None => ipc::read_token(&data_dir)?,
        };
        let conn = Stream::connect(socket.as_str().to_fs_name::<GenericFilePath>()?)
            .with_context(|| format!("cannot reach TermF at {socket} (is it running?)"))?;
        Ok(Self {
            conn: BufReader::new(conn),
            token,
        })
    }

    fn call(&mut self, call: Call) -> Result<Value> {
        let request = ipc::encode(&Request {
            token: self.token.clone(),
            call,
        });
        self.conn.get_mut().write_all(request.as_bytes())?;
        match self.read()? {
            Reply::Ok(value) => Ok(value),
            Reply::Error(e) => bail!(e),
            Reply::Event(_) => bail!("unexpected event"),
        }
    }

    fn read(&mut self) -> Result<Reply> {
        let mut line = String::new();
        if self.conn.read_line(&mut line)? == 0 {
            bail!("TermF closed the connection");
        }
        Ok(serde_json::from_str(&line)?)
    }
}
