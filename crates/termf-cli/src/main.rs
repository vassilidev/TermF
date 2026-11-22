use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use interprocess::local_socket::{GenericFilePath, Stream, prelude::*};
use serde_json::Value;
use termf_core::cli_agents::CODEX_TURN_DONE;
use termf_core::ipc::{self, Call, Reply, Request, SessionSummary};
use termf_core::remote;
use termf_library::archive::{self, Action, Status};
use termf_library::{Item, Kind, Layer, Outcome, Scope};

mod agent;
mod ci;
mod claude;
mod mcp;
mod memory;
mod ops;
mod review;
mod schedule;
mod scout;
mod ssh;
mod sync;
mod team;
mod tickets;
mod vault;
mod worktree;

const USAGE: &str = "\
usage:
  termf ls                              list sessions
  termf open <path> [--agent <name>]    open a session (a shell without --agent)
  termf say <session> <text>...         type text then Enter in a session (id or part of its title)
  termf watch                           print status changes
  termf hook <event>                    forward a Claude Code hook (JSON on stdin)
  termf statusline [<file>]             Claude Code status line of TermF sessions: reports the context,
                                        then runs the status line command saved in <file>
  termf mcp                             MCP server (stdio) of TermF's tools, for the CTO conversation
                                        TermF starts; serves nothing otherwise
  termf library ls [--kind <kind>] [--repo <path>] [--project <id>]
                                        list the library: each item's layer, state and why
  termf library explain <name> [--kind <kind>] [--repo <path>] [--project <id>]
  termf library enable|disable <name> [--kind <kind>] [--project <id>]
                                        switch an item on or off, globally or for a project
  termf library import-claude [--repo <path>] [--project <id>] [--overwrite]
                                        import skills, agents and commands from ~/.claude (or a repository's)
  termf library scout [--item <kind>/<name>]... [--project <id>] [--model <name>] [--budget <usd>]
                                        Claude Code, with web search only, critiques the library (or the
                                        items named) and proposes improvements, reviewed in TermF
  termf config export [-o <file>]       write the config and the library to one archive
  termf config import <file> [--dry-run] [--replace <item>]... [--replace-all]
                                        import an archive: merge, or replace the items named
  termf ssh ls                          list keys, agent keys and config hosts
  termf ssh gen <name> [--type ed25519|ecdsa|rsa] [--comment <text>]
                                        create a key in ~/.ssh (asks for a passphrase)
  termf ssh test <host>                 check that a host accepts your key
  termf ssh agent-add <key>             add a key to the SSH agent
  termf ssh agent-rm <key>              remove a key from the SSH agent
  termf ssh assign <key> [<repo>]       make a repository (default: here) use only this key
  termf ssh assign --none [<repo>]      back to the default keys
  termf ssh trash <key>                 move a key to ~/.ssh/.termf-trash
  termf sync setup --git <url> | --folder <path> [--name <device>] [--scopes config,projects,conventions,vault]
                                        sync this machine's config, encrypted, through a remote
  termf sync setup --s3 <endpoint> --bucket <name> [--region <region>] [--prefix <path>] | --webdav <url>
                                        the same through S3 (curl 8.3 or newer) or WebDAV; asks the credentials
  termf sync status                     remote, last sync, conflicts, missing repositories
  termf sync push | pull                sync now (pull only brings changes in)
  termf sync resolve <path> --local|--remote
                                        settle a conflict
  termf sync devices                    machines syncing, and when they last did
  termf sync change-password            re-encrypt everything; other machines need the new password
  termf sync recover                    set a new password with the recovery key
  termf sync scopes <list>              choose what syncs: config, projects, conventions, vault
                                        and ssh-keys (the keys of ~/.ssh, only when named)
  termf sync forget <device>            remove another machine and change the data key
  termf sync disable                    stop syncing on this machine
  termf run <ticket> [--project <id>] [--repo <name>]
                                        work on a ticket (URL, owner/repo#42, repo#42, #42, ACME-123)
                                        in a new worktree, with a Claude Code session
  termf run resume <run>                open a new session that resumes the run's Claude Code
                                        conversation (after TermF restarted, or once its session ended)
  termf runs [<run>]                    list runs, or show one run and its events
  termf inbox [--project <id> [--refresh]]
                                        open GitHub issues assigned to you, or what the project's
                                        sources list (--refresh asks its MCP sources again)
  termf deliver <run> [--yes]           show what the run would send, then (once you agree, or with
                                        --yes) commit it, push and open the pull request
  termf close <run> [--force]           remove the run's worktree (--force drops work not pushed)
  termf guard                           refuse pushes and tracker writes in a run's session
                                        (Claude Code PreToolUse hook, JSON on stdin)
  termf schedule list                   scheduled tasks, their schedule and next run
  termf schedule add <name> --cron '<min hour day month weekday>' --prompt <text|-> [--dir <path>]
                     [--tile] [--model <m>] [--effort <e>] [--permission-mode <p>] [--budget <usd>]
                     [--no-catchup] [--disable] [--machine <name>|any]
                                        a Claude Code task run on a schedule (local time) in a folder
                                        (default: here), headless or in a tile (--tile), on this machine
                                        only unless --machine says otherwise
  termf schedule edit <id> [--name <n>] [same options] [--headless] [--enable] [--catchup]
  termf schedule rm <id>                delete a task (its history stays)
  termf schedule run <id> [--scheduled] run it now (--scheduled: only if one of its times is due)
  termf schedule history [<id>] [--limit <n>] [--run <run>]
                                        past runs, or one run and its output
  termf schedule install|uninstall <id> also run a headless task while TermF is closed, through the
                                        system's scheduler (launchd, systemd, Task Scheduler)
  termf worktree setup [<path>]         prepare a worktree (default: here) with its project's recipe:
                                        dependencies, local files, its database copy, ports
  termf worktree teardown [<path>]      drop the worktree's database copy and give its ports back
  termf ci runs [--repo <path>] [--branch <name>] [--status <status>] [--pr <n>] [--page <n>]
                                        GitHub Actions runs of a repository (default: here), newest first
  termf ci view <run> [--repo <path>]   a run, its jobs and their steps
  termf ci logs <job> [--repo <path>] [--errors]
                                        a job's log, or only its error lines
  termf ci rerun <run> [--failed] [--repo <path>]
                                        run it again, or only its failed jobs
  termf ci cancel <run> [--repo <path>]
  termf ci dispatch <workflow> [--ref <branch>] [--input key=value]... [--repo <path>]
                                        start a workflow by hand (workflow_dispatch)
  termf agent ls                        headless agent sessions: state, cost, context
  termf agent start [--prompt <text|->] [--dir <path>] [--title <t>] [--project <id>] [--agent <name>]
                    [--role <name>] [--model <m>] [--effort <e>] [--permission-mode <p>] [--budget <usd>]
                                        a headless Claude Code session in a folder (default: here),
                                        driven by TermF; --budget covers the whole session; --role plays
                                        a library role (its prompt, model, effort, tools, and only the
                                        preferences and rules that apply to it)
  termf agent send <id> <text>...       its next message, once its turn is over (resumes a stopped one)
  termf agent events <id> [--after <n>] what it said and did
  termf agent interrupt <id>            stop its turn
  termf agent allow|deny <id> <request> [<message>...]
                                        answer a permission request
  termf agent stop <id>                 end its process
  termf agent takeover <id>             go on with the conversation in a tile, between two turns
  termf agent handback <id>             close that tile and go on headless
  termf team ls                         the agent teams of ticket runs: state, workflow, cost, what waits
  termf team plan <run> [--workflow <name>] [--rule <rule>=<level>]... [--budget <usd>]
                                        start the run's team and stop once its plan is made
  termf team run <run> [--workflow <name>] [--rule <rule>=<level>]... [--budget <usd>]
                                        start the run's team, or go on with it (paused, stopped,
                                        out of budget); levels: off, advise, warn, block
  termf team run --request <text> [--project <id>] [--repo <name>] [--workflow <name>] [--budget <usd>]
                                        start the team on a request written here, in a new run of its
                                        own (the project by default: the one of this folder)
  termf team route <run>                the CTO's routing: workflow, steps, squads, budget, why
  termf team answer <run> <step> <question>=<answer>... [--note <text>]
                                        answer a step's questions at once (a multiple choice:
                                        the question once per option; none: <question>=)
  termf team status <run>               its plan, its steps, what waits for you, what the rules found
  termf team events <run> [--after <n>] what happened, step by step
  termf team approve <run> <step>       let a gate pass, or accept a blocked step as it is
  termf team revise <run> <step> <feedback>...
                                        run a blocked step again, or the step a gate waits on, with
                                        your feedback (your answers to the plan's questions)
  termf team rules <run> [--task <id>] <rule>=<level>...
                                        rule levels for the ticket, or one task of the plan
  termf team stop [<run>]               emergency stop of one team, or of every team at work
  termf team squads [<project>]         the squads and staff of a project (by default every project),
                                        each role with its settings and the layers they come from
  termf review start [--run <run>] [--repo <path> --base <ref> --head <ref>] [--route <path>]...
                                        capture the screens before and after (a run's worktree, or two
                                        commits) with the project's dev server and Chrome, then compare
  termf review show <id> | --run <run>  the screens, what changed, the decisions and the images
  termf review decide <id> <screen>|all approve|reject|changes [--comment <text>] [--pin x,y,<text>]...
                                        decide a screen; once none is pending, the team's visual gate
                                        goes on, stops the run, or sends the changes to the agent
  termf review pr <id> [--yes]          show, then (with --yes) post the images on the run's pull request
  termf ask <question>... [--project <id>] [--budget <usd>]
                                        ask the project's expert (default: the project of this folder);
                                        it searches the project memory and reads the code, read-only, and
                                        answers with its sources (budget: $1 at most)
  termf memory search <query>... [--project <id>] [--source <s>]... [--limit <n>]
                                        search the project memory: conversations, runs, library, CLAUDE.md
                                        and automatic memory, docs, git history, artifacts (masked, local)
  termf memory reindex [--project <id>] bring the memory of a project (default: every project) up to date
  termf project trust <id> [--off]      trust the project's repositories in Claude Code (and so their
                                        worktrees): no more \"Is this a project you trust?\"; --off stops
  termf claude settings get <key> [--scope <s>] [--repo <path>]
                                        a Claude Code setting in each scope's file (masked), and which wins
  termf claude settings set <key> <value> [--scope user|project|local] [--repo <path>]
  termf claude settings unset <key> [--scope user|project|local] [--repo <path>]
                                        <value> is JSON, else text; checked like in the app, then the diff
                                        is shown and the file written (default scope: user; repo: here)
  termf claude mcp add <name> <json> [--scope local|project|user] [--repo <path>]
                                        through claude mcp add-json, the server on stdin
  termf claude mcp remove <name> [--scope <s>] [--repo <path>]
  termf claude plugin install|remove <plugin> [--scope user|project|local] [--accept-command <sha256>]
  termf claude plugin enable|disable <plugin> [--scope <s>]
                                        through claude plugin; a command the marketplace runs to install
                                        is shown, and runs only once you pass its --accept-command
  termf claude sessions [--repo <path>]  Claude Code's saved conversations of the repository and its worktrees
                                        (titles and first messages only); claude --resume <id> takes one up
  termf vault init                     create the vault, protected by a master password
  termf vault unlock | lock | status    TermF's vault: open it with the master password, close it, or see
                                        whether it is open (locks itself after some idle minutes)
                                        with security keys enrolled: a key's PIN, then a touch (Windows
                                        asks for both itself)
  termf vault ls [--scope dev|staging|prod] [--project <id>]
                                        names of the secrets, never their values; a project also sees the
                                        global ones
  termf vault set <name> [--scope <s>] [--project <id>] [--description <text>]
                                        store a value read on stdin (or typed, hidden), never an argument
  termf vault ref <name> <uri> [--scope <s>] [--project <id>] [--description <text>]
                                        a secret kept by a manager and read with its CLI each time it is used:
                                        op://, bw://, doppler://, aws-sm://, vault://
  termf vault reveal <name> [--scope <s>] [--project <id>]
                                        print a value
  termf vault rm <name> [--scope <s>] [--project <id>]
  termf vault items [--project <id>]   the password manager's logins and notes: id, kind, project, title,
                                        user name; never a value
  termf vault item <id> [password|totp|seed|notes]
                                        print a login's password (by default), its TOTP code now, its TOTP
                                        secret or the notes
  termf ops connections [--project <id>]
                                        the project's connections (default: the project of this folder):
                                        kind, environment, what agents may see, where each reaches
  termf ops test <connection> [--project <id>]
                                        what works and what is missing: the client, the vault's secrets,
                                        reaching it, and for a database whether its user can write
  termf ops recipes [--project <id>]    the recipes and their parameters
  termf ops run <recipe> --connection <name> [--<param> <value>]... [--project <id>]
                                        run a recipe without an agent: its exact command, then its output,
                                        masked and audited; one that writes asks you to type the
                                        environment's name (e.g. termf ops run logs-tail --connection
                                        acme-prod --since 1h --grep ERROR)
  termf vault audit [--limit <n>] [--jsonl] | termf vault audit verify
                                        who read or changed what, or check the log's hash chain
  termf vault export -o <file> [--project <id>]
                                        the vault (or a project's part) sealed by an export password
                                        asked on the terminal; --csv writes the logins and notes in clear,
                                        after a warning
  termf vault import <file> [--project <id>] [--restore]
                                        a TermF export (or a config export holding the vault), merged item
                                        by item (--restore: what it holds and was deleted since comes back);
                                        or a Bitwarden, 1Password, Chrome, Edge, Firefox, Safari,
                                        KeePassXC or KeePass export, shown first, duplicates left out
                                        (the scope is dev without --scope; every vault command that reads or
                                        writes a secret asks for the master password)
kinds: skill, command, role, preference, rule, workflow";

fn main() -> ExitCode {
    // ssh-add runs this binary as its askpass helper when TermF adds a key with a passphrase.
    if env::var_os(termf_ssh::ASKPASS_ENV).is_some() {
        return match termf_ssh::askpass() {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        };
    }
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["hook", event] => {
            hook(event);
            return ExitCode::SUCCESS;
        }
        // Codex CLI's `notify`: its JSON last, after the user's own `notify`, run next with it.
        ["hook", CODEX_TURN_DONE, yours @ .., raw] => {
            let payload = serde_json::from_str(raw).unwrap_or(Value::Null);
            let (done, wait) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = forward(CODEX_TURN_DONE.into(), payload);
                let _ = done.send(());
            });
            let _ = wait.recv_timeout(Duration::from_secs(2));
            if let [program, args @ ..] = yours {
                let _ = std::process::Command::new(program)
                    .args(args)
                    .arg(raw)
                    .status();
            }
            return ExitCode::SUCCESS;
        }
        // Gemini CLI's `SessionStart`: the library's instructions become the session's context.
        ["hook", event @ "SessionStart", prompt] => {
            hook(event);
            if let Some(context) = termf_core::cli_agents::gemini_context(Path::new(prompt)) {
                println!("{context}");
            }
            return ExitCode::SUCCESS;
        }
        ["statusline", chained @ ..] if chained.len() < 2 => {
            return statusline(chained.first().copied());
        }
        _ => {}
    }
    if let ["guard"] = args.as_slice() {
        return tickets::guard();
    }
    if let Ok(config_dir) = termf_core::default_config_dir() {
        termf_core::agent_env::configure(config_dir, env::current_exe().ok());
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
        // What TermF runs an agent through, after a login shell's profile: only what it keeps of the environment.
        ["agent-env", command] => termf_core::agent_env::exec(command)?,
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
        ["library", rest @ ..] => library(rest.to_vec())?,
        ["config", rest @ ..] => config(rest.to_vec())?,
        ["ssh", rest @ ..] => ssh::run(rest)?,
        ["sync", rest @ ..] => sync::run(rest)?,
        ["schedule", rest @ ..] => schedule::run(rest.to_vec())?,
        ["worktree", rest @ ..] => worktree::run(rest)?,
        ["ci", rest @ ..] => ci::run(rest.to_vec())?,
        ["agent", rest @ ..] => agent::run(rest.to_vec())?,
        ["team", rest @ ..] => team::run(rest.to_vec())?,
        ["claude", rest @ ..] => claude::run(rest.to_vec())?,
        ["vault", rest @ ..] => vault::run(rest.to_vec())?,
        ["ops", rest @ ..] => ops::run(rest.to_vec())?,
        ["mcp"] => mcp::serve()?,
        ["mcp", "--reaper"] => mcp::reap()?,
        ["review", rest @ ..] => review::run(rest.to_vec())?,
        ["memory", rest @ ..] => memory::run(rest.to_vec())?,
        ["ask", rest @ ..] => memory::ask(rest.to_vec())?,
        ["project", "trust", id, rest @ ..] => {
            let on = match rest {
                [] => true,
                ["--off"] => false,
                _ => bail!(USAGE),
            };
            let config_dir = termf_core::default_config_dir()?;
            for r in termf_claude::trust::set_project(&config_dir, id, on)? {
                let state = if r.trusted { "trusted" } else { "not trusted" };
                println!("{:<12}  {state:<11}  {}", r.name, r.root);
            }
        }
        [
            command @ ("run" | "runs" | "inbox" | "deliver" | "close"),
            rest @ ..,
        ] => tickets::run(command, rest.to_vec())?,
        ["-h" | "--help" | "help"] => println!("{USAGE}"),
        _ => bail!(USAGE),
    }
    Ok(())
}

/// Works on the config files directly: TermF does not need to run.
fn library(mut args: Vec<&str>) -> Result<()> {
    let config_dir = termf_core::default_config_dir()?;
    let kind = match take_opt(&mut args, "--kind")? {
        Some(k) => Some(Kind::parse(&k).with_context(|| format!("unknown kind {k}"))?),
        None => None,
    };
    let repo = take_opt(&mut args, "--repo")?
        .map(std::path::absolute)
        .transpose()?;
    let project = take_opt(&mut args, "--project")?;
    let overwrite = take_flag(&mut args, "--overwrite");
    let scope = || Scope::new(&config_dir, repo.as_deref(), project.clone());
    let named = |name: &str| -> Result<Item> {
        let mut found: Vec<Item> = termf_library::resolve(&config_dir, &scope()?)?
            .into_iter()
            .filter(|i| i.name == name && kind.is_none_or(|k| i.kind == k))
            .collect();
        match found.len() {
            1 => Ok(found.remove(0)),
            0 => bail!("no library item named {name}"),
            _ => bail!("several items are named {name}: add --kind"),
        }
    };
    match args.as_slice() {
        ["ls"] => {
            for item in termf_library::resolve(&config_dir, &scope()?)? {
                if kind.is_none_or(|k| item.kind == k) {
                    let state = if item.active { "on" } else { "off" };
                    let (kind, name, layer) = (item.kind, &item.name, item.layer);
                    println!(
                        "{kind:<10}  {name:<24}  {state:<3}  {layer:<7}  {}",
                        why(&item)
                    );
                }
            }
        }
        ["explain", name] => {
            let item = named(name)?;
            println!(
                "{} {}: {}",
                item.kind,
                item.name,
                if item.active { "on" } else { "off" }
            );
            let place = item.path.as_ref().map(|p| format!(" ({})", p.display()));
            println!("  layer: {}{}", item.layer, place.unwrap_or_default());
            if !item.shadows.is_empty() {
                let hidden: Vec<String> = item.shadows.iter().map(|l| l.to_string()).collect();
                println!("  hides: {}", hidden.join(", "));
            }
            println!("  why: {}", why(&item));
            if let Some(error) = &item.error {
                println!("  error: {error}");
            }
        }
        [action @ ("enable" | "disable"), name] => {
            // Looked up first, so that a typo writes no toggle.
            let kind = named(name)?.kind;
            // With --repo, the toggle goes where `ls --repo` reads it: the project that lists the repository.
            let target = scope()?.project;
            let on = *action == "enable";
            termf_library::set_toggle(&config_dir, target.as_deref(), kind, name, Some(on))?;
            let item = named(name)?;
            let file = target.map_or("termf.toml".into(), |id| format!("projects/{id}.toml"));
            let state = if item.active { "on" } else { "off" };
            println!("{} {}: {state} (written in {file})", item.kind, item.name);
        }
        ["scout", rest @ ..] => {
            if kind.is_some() || repo.is_some() || overwrite {
                bail!("termf library scout takes --item, --project, --model and --budget");
            }
            scout::run(&config_dir, project.as_deref(), rest.to_vec())?
        }
        ["import-claude"] => {
            let from = match &repo {
                Some(repo) => repo.join(".claude"),
                None => termf_core::expand_tilde("~/.claude"),
            };
            let library = termf_library::library_dir(&config_dir, project.as_deref())?;
            for i in termf_library::import_claude(&from, &library, overwrite)? {
                let outcome = match i.outcome {
                    Outcome::Imported => "imported",
                    Outcome::Replaced => "replaced",
                    Outcome::Skipped => "skipped (already in the library; --overwrite replaces it)",
                    Outcome::Unsupported => "skipped (subfolders are not supported)",
                };
                println!("{:<10}  {:<24}  {outcome}", i.kind, i.name);
            }
        }
        _ => bail!(USAGE),
    }
    Ok(())
}

fn why(item: &Item) -> String {
    match item.toggles.iter().next_back() {
        Some((layer, on)) => {
            let place = match layer {
                Layer::Global => "termf.toml",
                Layer::Project => "the project's file",
                _ => "this launch",
            };
            format!("{}{} in {place}", if *on { '+' } else { '-' }, item.name)
        }
        None if !item.activated_by.is_empty() => {
            format!("switched on by the stack: {}", item.activated_by.join(", "))
        }
        None if item.layer == Layer::Package => format!(
            "shipped by the package {}, on where Claude Code trusts the repository",
            item.package.as_ref().map_or("", |p| p.name.as_str())
        ),
        None if item.layer == Layer::Shipped => "shipped with TermF, off until enabled".into(),
        None if item.layer == Layer::Repo => "from the repository, off until you enable it".into(),
        None => format!("in the {} library", item.layer),
    }
}

fn config(mut args: Vec<&str>) -> Result<()> {
    let config_dir = termf_core::default_config_dir()?;
    let out = take_opt(&mut args, "-o")?;
    let dry_run = take_flag(&mut args, "--dry-run");
    let replace_all = take_flag(&mut args, "--replace-all");
    let mut replace = Vec::new();
    while let Some(item) = take_opt(&mut args, "--replace")? {
        replace.push(item);
    }
    match args.as_slice() {
        ["export"] => {
            let out = out.unwrap_or_else(|| "termf-config.tar.gz".into());
            let exported = archive::export(&config_dir, Path::new(&out))?;
            for link in &exported.skipped_links {
                println!("skipped {link}: links are not exported");
            }
            println!("{} files written to {out}", exported.files);
        }
        ["import", file] => {
            let file = Path::new(file);
            if replace_all {
                let preview = archive::preview(file, &config_dir)?;
                replace = preview.changes.into_iter().map(|c| c.item).collect();
            }
            let preview = if dry_run {
                archive::preview(file, &config_dir)?
            } else {
                archive::apply(file, &config_dir, &replace)?
            };
            for change in &preview.changes {
                let status = match change.status {
                    Status::Added => "added",
                    Status::Unchanged => "unchanged",
                    Status::Changed => "changed",
                    Status::Conflict => "conflict",
                };
                let done = match (dry_run, change.action(&replace)) {
                    (true, _) | (_, Action::Write) => "",
                    (false, Action::Replace) => ", replaced",
                    (false, Action::Merge) => ", merged",
                    (false, Action::Keep) if change.status == Status::Unchanged => "",
                    (false, Action::Keep) => ", kept yours",
                };
                let mut keys = Vec::new();
                if change.linked {
                    keys.push("a link, never written through".into());
                }
                if !change.added_keys.is_empty() {
                    keys.push(format!("new: {}", change.added_keys.join(", ")));
                }
                if !change.conflicts.is_empty() {
                    keys.push(format!("differ: {}", change.conflicts.join(", ")));
                }
                let keys = if keys.is_empty() {
                    String::new()
                } else {
                    format!("  ({})", keys.join("; "))
                };
                println!("{:<24}  {}{keys}", format!("{status}{done}"), change.item);
            }
            for repo in &preview.missing_repos {
                let clone = repo
                    .remote
                    .as_deref()
                    .map(|r| format!(": git clone {r} {}", repo.path));
                println!(
                    "missing repository {} in {}{}",
                    repo.path,
                    repo.project,
                    clone.unwrap_or_default()
                );
            }
        }
        _ => bail!(USAGE),
    }
    Ok(())
}

fn take_opt(args: &mut Vec<&str>, name: &str) -> Result<Option<String>> {
    match args.iter().position(|a| *a == name) {
        Some(i) if i + 1 < args.len() => {
            let value = args.remove(i + 1).to_owned();
            args.remove(i);
            Ok(Some(value))
        }
        Some(_) => bail!("{name} needs a value"),
        None => Ok(None),
    }
}

fn take_flag(args: &mut Vec<&str>, name: &str) -> bool {
    let found = args.iter().position(|a| *a == name);
    found.map(|i| args.remove(i)).is_some()
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

/// Prints TermF's answer, if any, for Claude Code to read (e.g. the handoff it asks for).
fn forward_hook(event: String) -> Result<()> {
    let mut payload = String::new();
    std::io::stdin().read_to_string(&mut payload)?;
    let reply = forward(event, serde_json::from_str(&payload).unwrap_or(Value::Null))?;
    if !reply.is_null() {
        println!("{reply}");
    }
    Ok(())
}

fn forward(event: String, mut payload: Value) -> Result<Value> {
    // Not started by TermF: nothing to report to.
    let session = env::var(ipc::ENV_SESSION)?.parse()?;
    if let (Some(map), Ok(bridge)) = (payload.as_object_mut(), env::var(remote::ENV_BRIDGE)) {
        map.insert(remote::PAYLOAD_BRIDGE.into(), bridge.into());
    }
    // Waiting for the reply keeps consecutive hooks in order.
    Client::connect(Some(env::var(ipc::ENV_TOKEN)?))?.call(Call::Hook {
        session,
        event,
        payload,
    })
}

/// The status line of the Claude Code sessions TermF starts: reports the context to TermF, then shows
/// the status line you set, saved in `chained`, with the same input.
fn statusline(chained: Option<&str>) -> ExitCode {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let payload = serde_json::from_str(&input).unwrap_or(Value::Null);
    let (done, wait) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = forward("StatusLine".into(), payload);
        let _ = done.send(());
    });
    let code = match chained.and_then(|file| std::fs::read_to_string(file).ok()) {
        Some(command) => run_chained(&command, &input),
        None => ExitCode::SUCCESS,
    };
    let _ = wait.recv_timeout(Duration::from_secs(1));
    code
}

fn run_chained(command: &str, input: &str) -> ExitCode {
    let Some(shell) = shell() else {
        return ExitCode::FAILURE;
    };
    let child = std::process::Command::new(shell)
        .args(["-c", command])
        .stdin(std::process::Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return ExitCode::FAILURE;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    match child.wait() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

/// The shell Claude Code runs status lines with: Git Bash on Windows, where a bare `bash` can be WSL's.
// shortcut: without Git Bash, Claude Code uses PowerShell and your status line is not shown; upgrade if
// TermF ever runs on such machines.
fn shell() -> Option<std::path::PathBuf> {
    if !cfg!(windows) {
        return Some("sh".into());
    }
    env::var_os("CLAUDE_CODE_GIT_BASH_PATH")
        .map(Into::into)
        .or_else(|| env::var_os("ProgramFiles").map(|p| Path::new(&p).join(r"Git\bin\bash.exe")))
        .filter(|bash: &std::path::PathBuf| bash.is_file())
}

struct Client {
    conn: BufReader<Stream>,
    token: String,
}

impl Client {
    /// `token: None` uses the user's token. Sessions started by TermF get the socket in their environment.
    fn connect(token: Option<String>) -> Result<Self> {
        let data_dir = termf_core::default_data_dir()?;
        let socket = env::var(ipc::ENV_SOCKET).unwrap_or_else(|_| ipc::socket_name(&data_dir));
        let token = match token {
            Some(token) => token,
            None => ipc::read_user_token()?,
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
