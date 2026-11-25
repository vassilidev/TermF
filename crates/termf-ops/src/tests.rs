// The fake clients are shell scripts: most tests, and what only they use, are Unix-only.
#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use termf_sync::KeyStore;
use termf_vault::{Action, Actor, Secret, SecretId, SystemClock, Vault};

use super::*;

const PASSWORD: &str = "correct horse battery";
const DB_PASSWORD: &str = "Pg-4f8a9c2e7b1d";

#[derive(Default, Clone)]
struct MemoryStore(Arc<Mutex<HashMap<String, Vec<u8>>>>);

impl KeyStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().get(account).cloned())
    }
    fn set(&self, account: &str, secret: &[u8]) -> Result<()> {
        self.0.lock().unwrap().insert(account.into(), secret.into());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<()> {
        self.0.lock().unwrap().remove(account);
        Ok(())
    }
}

/// A throwaway config with the project `acme`, data folder and fake clients: never the user's.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("termf-ops-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("config/projects")).unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(
            root.join("config/projects/acme.toml"),
            "# Acme's project\nname = \"Acme\"\n\n[[repos]]\npath = \"/tmp/acme/api\"\n",
        )
        .unwrap();
        Self { root }
    }

    fn config(&self) -> PathBuf {
        self.root.join("config")
    }

    fn data(&self) -> PathBuf {
        self.root.join("data")
    }

    fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    /// The fakes first, then the system's own tools (sh, grep, tail).
    fn runner(&self) -> Runner {
        let mut paths = vec![self.bin()];
        paths.extend(std::env::split_paths(&OsString::from("/usr/bin:/bin")));
        Runner::new(Some(std::env::join_paths(paths).unwrap()))
    }

    #[cfg(unix)]
    fn fake(&self, name: &str, script: &str) {
        use std::os::unix::fs::PermissionsExt;
        let file = self.bin().join(name);
        let script = script.replace("@DIR@", &self.root.to_string_lossy());
        fs::write(&file, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn vault(&self) -> Vault {
        Vault::new(
            &self.data(),
            Box::new(MemoryStore::default()),
            Box::new(SystemClock::default()),
        )
    }

    fn connection(&self, toml: &str) -> Connection {
        let c: Connection = toml::from_str(toml).unwrap();
        save_connection(&self.config(), "acme", &c, None).unwrap();
        c
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// An SSH that runs the remote command here, in `sh`, as the server's shell would read it; a tunnel listens
/// on its local port.
#[cfg(unix)]
const SSH: &str = r#"
for a in "$@"; do printf '%s\n' "$a" >> "@DIR@/ssh.args"; done
printf -- '----\n' >> "@DIR@/ssh.args"
tunnel=""
while [ $# -gt 0 ]; do
  case "$1" in
    --) shift; break;;
    -N) tunnel=1; shift;;
    -L) fwd="$2"; shift 2;;
    -o|-p|-l|-J) shift 2;;
    *) shift;;
  esac
done
if [ -n "$tunnel" ]; then
  port=$(echo "$fwd" | cut -d: -f2)
  exec python3 -c "import socket,time
s=socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); s.bind(('127.0.0.1', $port)); s.listen(5); time.sleep(60)"
fi
exec /bin/sh -c "$2"
"#;

#[cfg(unix)]
const JOURNALCTL: &str = r#"
for a in "$@"; do printf 'ARG:%s\n' "$a"; done
echo "10:00:01 api: login ok for alice@example.com, phone +33 6 12 34 56 78"
echo "10:00:02 api: refund to FR76 3000 6000 0112 3456 7890 189"
echo "10:00:03 api: db pass Pg-4f8a9c2e7b1d rejected"
echo "10:00:04 api: session eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhbGljZSJ9.c2lnbmF0dXJlLXZhbHVl"
echo "10:00:05 api: key Zq8LmT3vXp1RkW9sYb2N stays out"
echo "10:00:06 api: deployed commit 3f2a9c1e77b04d1e9a5c0e8b done"
"#;

fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn recipe(name: &str) -> Recipe {
    let (recipes, errors) = recipes(Path::new("/nonexistent"), None);
    assert!(errors.is_empty(), "{errors:?}");
    recipes.into_iter().find(|r| r.name == name).unwrap()
}

const PROD_SSH: &str = r#"
name = "acme-prod"
kind = "ssh"
env = "prod"
ai_access = "redacted"
host = "acme-prod"
user = "deploy"
jump = "bastion.acme.test"
"#;

#[test]
fn every_shipped_recipe_reads() {
    let (recipes, errors) = recipes(Path::new("/nonexistent"), None);
    assert!(errors.is_empty(), "{errors:?}");
    let names: Vec<&str> = recipes.iter().map(|r| r.name.as_str()).collect();
    for name in [
        "logs-tail",
        "logs-cloud",
        "k8s-pods",
        "k8s-logs",
        "ssh-run-readonly",
        "service-restart",
    ] {
        assert!(names.contains(&name), "{name} missing from {names:?}");
    }
    assert_eq!(recipe("service-restart").mode, Mode::Write);
    let tail = recipe("logs-tail");
    let order: Vec<&str> = tail.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        order,
        ["unit", "file", "since", "grep", "lines"],
        "the file's order is the form's"
    );
}

#[test]
fn connections_are_written_in_the_project_file_and_its_comments_stay() {
    let env = Fixture::new("connections");
    let db: Connection = toml::from_str(
        r#"
name = "acme-db"
kind = "postgres"
env = "prod"
database = "acme"
user = "readonly"
via = "acme-prod"
secrets = { password = "ACME_DB_PASSWORD" }
"#,
    )
    .unwrap();
    let refused = save_connection(&env.config(), "acme", &db, None).unwrap_err();
    assert!(refused.to_string().contains("via names"), "{refused}");
    env.connection(PROD_SSH);
    let all = save_connection(&env.config(), "acme", &db, None).unwrap();
    assert_eq!(all.len(), 2);
    let text = fs::read_to_string(env.config().join("projects/acme.toml")).unwrap();
    assert!(text.starts_with("# Acme's project"), "{text}");
    assert!(
        text.contains("secrets = { password = \"ACME_DB_PASSWORD\" }"),
        "{text}"
    );
    assert!(!text.contains(DB_PASSWORD));
    assert_eq!(
        termf_core::load_projects(&env.config()).unwrap()[0].name,
        "Acme"
    );
    let deleting = delete_connection(&env.config(), "acme", "acme-prod").unwrap_err();
    assert!(deleting.to_string().contains("tunnel"), "{deleting}");
    // Renaming the server renames it where the database goes through; it cannot stop being SSH meanwhile.
    let mut server = connections(&env.config(), "acme").unwrap()[0].clone();
    server.name = "acme-api".into();
    let all = save_connection(&env.config(), "acme", &server, Some("acme-prod")).unwrap();
    assert_eq!(all[1].via.as_deref(), Some("acme-api"));
    let k8s = Connection {
        kind: Kind::Kubernetes,
        context: Some("prod".into()),
        ..server.clone()
    };
    let refused = save_connection(&env.config(), "acme", &k8s, Some("acme-api")).unwrap_err();
    assert!(
        refused.to_string().contains("stays an SSH connection"),
        "{refused}"
    );
    server.name = "acme-prod".into();
    save_connection(&env.config(), "acme", &server, Some("acme-api")).unwrap();
    let renamed = Connection {
        name: "acme-db-ro".into(),
        ..db.clone()
    };
    let all = save_connection(&env.config(), "acme", &renamed, Some("acme-db")).unwrap();
    assert_eq!(
        all.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["acme-prod", "acme-db-ro"]
    );
    let all = delete_connection(&env.config(), "acme", "acme-db-ro").unwrap();
    assert_eq!(all.len(), 1);
    let mut bad = all[0].clone();
    for host in ["-oProxyCommand=touch /tmp/x", "a host", "host\nx"] {
        bad.host = Some(host.into());
        assert!(
            save_connection(&env.config(), "acme", &bad, Some("acme-prod")).is_err(),
            "{host}"
        );
    }
    bad.host = Some("acme-prod".into());
    bad.secrets.insert("password".into(), "x".into());
    assert!(
        bad.check()
            .unwrap_err()
            .to_string()
            .contains("no password secret")
    );
}

#[test]
fn parameters_never_become_options_nor_shell() {
    let tail = recipe("logs-tail");
    let p = |n: &str| tail.param(n).unwrap().clone();
    for refused in [
        "-f/etc/shadow",
        "--output=/tmp/x",
        "a\nb",
        "a\tb",
        "\u{1b}[31m",
    ] {
        assert!(check_param(&p("grep"), refused).is_err(), "{refused:?}");
    }
    assert_eq!(
        check_param(&p("grep"), "$(rm -rf /)").unwrap(),
        "$(rm -rf /)"
    );
    for refused in [
        "../etc/passwd",
        "/var/log/../../etc/shadow",
        "relative.log",
        "/var/log/a b.log",
        "/x;y",
    ] {
        assert!(check_param(&p("file"), refused).is_err(), "{refused:?}");
    }
    assert!(check_param(&p("unit"), "nginx.service; reboot").is_err());
    assert_eq!(check_param(&p("since"), "2d").unwrap(), "48h");
    assert_eq!(check_param(&p("since"), "15m").unwrap(), "15m");
    assert!(check_param(&p("since"), "91d").is_err());
    assert!(check_param(&p("since"), "1w").is_err());
    assert!(check_param(&p("lines"), "0").is_err() && check_param(&p("lines"), "2001").is_err());
    let readonly = recipe("ssh-run-readonly");
    let command = readonly.param("command").unwrap();
    assert!(check_param(command, "rm -rf /").is_err());
    assert!(check_param(command, "df -h").is_ok());
    for (arg, quoted) in [
        ("plain-word_1.2", "plain-word_1.2"),
        ("1h ago", "'1h ago'"),
        ("it's", r"'it'\''s'"),
        ("$(id)", "'$(id)'"),
        ("", "''"),
    ] {
        assert_eq!(quote(arg), quoted);
    }
}

#[cfg(unix)]
#[test]
fn quoting_gives_the_remote_shell_each_argument_as_it_is() {
    for arg in [
        "$(touch /tmp/termf-pwned)",
        "`id`",
        "a'b\"c",
        "; rm -rf ~",
        "x || y && z",
        "*",
        "~",
        "a\\b",
        "é ü",
    ] {
        let out = std::process::Command::new("/bin/sh")
            .args(["-c", &format!("printf '%s' {}", quote(arg))])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), arg);
    }
}

#[cfg(unix)]
#[test]
fn logs_tail_runs_through_ssh_masked_and_the_injection_stays_text() {
    let env = Fixture::new("tail");
    env.fake("ssh", SSH);
    env.fake("journalctl", JOURNALCTL);
    let conn = env.connection(PROD_SSH);
    let pwned = env.root.join("pwned");
    let grep = format!("$(touch {}); echo", pwned.display());
    let plan = crate::plan(
        &recipe("logs-tail"),
        &conn,
        std::slice::from_ref(&conn),
        &params(&[("grep", &grep), ("unit", "api.service")]),
    )
    .unwrap();
    assert_eq!(plan.env, Env::Prod);
    assert!(
        plan.command.starts_with("ssh -o BatchMode=yes"),
        "{}",
        plan.command
    );
    assert!(
        plan.command.contains("-J bastion.acme.test -- acme-prod"),
        "{}",
        plan.command
    );
    let remote = plan.remote.clone().unwrap();
    assert!(remote.starts_with("journalctl --no-pager"), "{remote}");
    assert!(
        remote.contains("--since '1h ago'") && remote.contains("-u api.service"),
        "{remote}"
    );
    let raw = env.runner().run(&plan, &|_| unreachable!()).unwrap();
    assert!(!pwned.exists(), "the parameter ran as a command");
    assert!(
        raw.stdout.contains(&format!("ARG:{grep}")),
        "{}",
        raw.stdout
    );
    let masker = termf_vault::Masker::new([&Secret::new(DB_PASSWORD.into())]);
    let all = Redact::PERSONAL;
    let out = redact::mask(raw, Some(&masker), &all, &plan.limits);
    assert!(!out.text.contains(DB_PASSWORD), "{}", out.text);
    assert!(
        !out.text.contains("alice@example.com") && out.text.contains("[email]"),
        "{}",
        out.text
    );
    assert!(
        out.text.contains("[phone]") && out.text.contains("[iban]"),
        "{}",
        out.text
    );
    assert!(
        !out.text.contains("eyJhbGciOiJIUzI1NiJ9"),
        "a JWT: {}",
        out.text
    );
    assert!(
        !out.text.contains("Zq8LmT3vXp1RkW9sYb2N"),
        "a random key: {}",
        out.text
    );
    assert!(
        out.text.contains("3f2a9c1e77b04d1e9a5c0e8b"),
        "a hash stays: {}",
        out.text
    );
    assert!(out.masks >= 6, "{}", out.masks);
    assert_eq!(out.hidden.len(), 1);
    assert!(
        out.parts
            .iter()
            .any(|p| matches!(p, Part::Hidden { hidden: 0 }))
    );
    let args = fs::read_to_string(env.root.join("ssh.args")).unwrap();
    assert!(
        args.contains("BatchMode=yes\n") && args.contains("-J\nbastion.acme.test\n--\nacme-prod\n"),
        "{args}"
    );
}

#[cfg(unix)]
#[test]
fn personal_data_stays_unless_the_policy_masks_it() {
    let raw = Raw {
        stdout: "mail bob@acme.test, call 06 12 34 56 78, at 2026-11-22 10:00:01, ip 10.0.0.12\n"
            .into(),
        ..Raw::default()
    };
    let out = redact::mask(raw, None, &[], &Limits::default());
    assert!(
        out.text.contains("bob@acme.test") && out.text.contains("06 12 34 56 78"),
        "{}",
        out.text
    );
    let raw = Raw {
        stdout: "mail bob@acme.test, call 06 12 34 56 78, at 2026-11-22 10:00:01, ip 10.0.0.12\n"
            .into(),
        ..Raw::default()
    };
    let out = redact::mask(raw, None, &Redact::PERSONAL, &Limits::default());
    assert_eq!(
        out.text,
        "mail [email], call [phone], at 2026-11-22 10:00:01, ip 10.0.0.12\n"
    );
}

#[cfg(unix)]
#[test]
fn a_run_stops_at_its_deadline_and_past_its_size() {
    let env = Fixture::new("limits");
    env.fake("ssh", SSH);
    let conn = env.connection(PROD_SSH);
    let slow = Recipe::parse(
        r#"
name = "slow"
[limits]
timeout_s = 1
[[run]]
kind = "ssh"
args = ["sleep", "30"]
"#,
        "project",
    )
    .unwrap();
    let plan = crate::plan(&slow, &conn, std::slice::from_ref(&conn), &BTreeMap::new()).unwrap();
    let started = std::time::Instant::now();
    let raw = env.runner().run(&plan, &|_| unreachable!()).unwrap();
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(
        raw.stopped
            .as_deref()
            .unwrap_or_default()
            .contains("timed out"),
        "{raw:?}"
    );
    let big = Recipe::parse(
        r#"
name = "big"
[limits]
bytes = 1000
lines = 10
[[run]]
kind = "ssh"
args = ["yes", "a line of output"]
"#,
        "project",
    )
    .unwrap();
    let plan = crate::plan(&big, &conn, std::slice::from_ref(&conn), &BTreeMap::new()).unwrap();
    let raw = env.runner().run(&plan, &|_| unreachable!()).unwrap();
    assert!(
        raw.stopped
            .as_deref()
            .unwrap_or_default()
            .contains("stopped past 1000 bytes"),
        "{:?}",
        raw.stopped
    );
    let out = redact::mask(raw, None, &[], &plan.limits);
    assert!(out.truncated);
    assert_eq!(out.text.lines().count(), 10, "{}", out.text);

    // Nothing kept, a flood all the same: stopped by what it printed in all, long before its deadline.
    let filtered = Recipe::parse(
        r#"
name = "filtered"
[limits]
bytes = 1000
timeout_s = 60
[[run]]
kind = "ssh"
args = ["yes", "nothing to see"]
keep = "ERROR"
"#,
        "project",
    )
    .unwrap();
    let plan = crate::plan(
        &filtered,
        &conn,
        std::slice::from_ref(&conn),
        &BTreeMap::new(),
    )
    .unwrap();
    let started = std::time::Instant::now();
    let raw = env.runner().run(&plan, &|_| unreachable!()).unwrap();
    assert!(started.elapsed() < Duration::from_secs(30));
    assert!(
        raw.stopped
            .as_deref()
            .unwrap_or_default()
            .contains("stopped after reading"),
        "{:?}",
        raw.stopped
    );
    assert!(raw.stdout.is_empty());

    // One line that never ends: stopped past its limit too, and never shown cut.
    let endless = Recipe::parse(
        r#"
name = "endless"
[limits]
bytes = 1000
timeout_s = 60
[[run]]
kind = "ssh"
args = ["sh", "-c", "yes | tr -d '\\n'"]
"#,
        "project",
    )
    .unwrap();
    let plan = crate::plan(
        &endless,
        &conn,
        std::slice::from_ref(&conn),
        &BTreeMap::new(),
    )
    .unwrap();
    let started = std::time::Instant::now();
    let raw = env.runner().run(&plan, &|_| unreachable!()).unwrap();
    assert!(started.elapsed() < Duration::from_secs(30));
    assert!(
        raw.stopped
            .as_deref()
            .unwrap_or_default()
            .contains("stopped past 1000 bytes"),
        "{:?}",
        raw.stopped
    );
}

#[cfg(unix)]
#[test]
fn secrets_go_through_the_environment_or_stdin_never_the_arguments() {
    let env = Fixture::new("secrets");
    env.fake("psql", r#"printf 'ARGS:%s\n' "$*"; echo "options $PGOPTIONS"; echo "password $PGPASSWORD"; echo "termf-ok|t""#);
    env.fake("curl", r#"printf 'ARGS:%s\n' "$*"; cat"#);
    let db = env.connection(
        r#"
name = "acme-db"
kind = "postgres"
env = "staging"
host = "db.acme.test"
database = "acme"
user = "app"
secrets = { password = "ACME_DB_PASSWORD" }
"#,
    );
    let dd = env.connection(
        r#"
name = "acme-dd"
kind = "datadog"
env = "prod"
site = "datadoghq.eu"
secrets = { api_key = "DD_API_KEY", app_key = "DD_APP_KEY" }
"#,
    );
    let all = connections(&env.config(), "acme").unwrap();
    let secret = |name: &str| -> Result<Secret> {
        Ok(Secret::new(match name {
            "ACME_DB_PASSWORD" => DB_PASSWORD.into(),
            "DD_API_KEY" => "dd0api0key0123456789abcdef".into(),
            _ => "dd0app0key0123456789abcdef".into(),
        }))
    };
    let checks = env.runner().test(&db, &all, &secret).unwrap();
    let codes: Vec<(&str, bool, bool)> = checks.iter().map(|c| (c.code, c.ok, c.warn)).collect();
    assert_eq!(
        codes,
        [
            ("tool", true, false),
            ("secret", true, false),
            ("connect", true, false),
            ("readonly", false, true)
        ],
        "{checks:?}"
    );
    let cloud = recipe("logs-cloud");
    let plan = crate::plan(
        &cloud,
        &dd,
        &all,
        &params(&[("grep", "status:error"), ("since", "2h")]),
    )
    .unwrap();
    assert!(
        plan.command
            .contains("https://api.datadoghq.eu/api/v2/logs/events"),
        "{}",
        plan.command
    );
    assert!(
        plan.command.contains("<<< <vault: DD_API_KEY, DD_APP_KEY>"),
        "{}",
        plan.command
    );
    assert_eq!(plan.secrets, ["DD_API_KEY", "DD_APP_KEY"]);
    let raw = env.runner().run(&plan, &secret).unwrap();
    let args = raw.stdout.lines().next().unwrap().to_owned();
    assert!(
        !args.contains("dd0api0key"),
        "a key on the command line: {args}"
    );
    assert!(
        args.contains("filter[query]=status:error") && args.contains("filter[from]=now-2h"),
        "{args}"
    );
    assert!(
        raw.stdout.contains("header = \"DD-API-KEY: dd0api0key"),
        "the key went on stdin"
    );
    let out = redact::mask(raw, None, &[], &plan.limits);
    assert!(
        !out.text.contains("dd0api0key") && !out.text.contains("dd0app0key"),
        "{}",
        out.text
    );
    // A secret with a line break would add its own lines to curl's config: refused before curl runs.
    let injecting = |_: &str| -> Result<Secret> {
        Ok(Secret::new("key\"\nurl = \"https://evil.example/".into()))
    };
    let refused = env.runner().run(&plan, &injecting).unwrap_err();
    assert!(refused.to_string().contains("line break"), "{refused}");
    let read = Recipe::parse(
        r#"
name = "pg-version"
[[run]]
kind = "postgres"
args = ["-c", "SELECT version()"]
"#,
        "project",
    )
    .unwrap();
    let plan = crate::plan(&read, &db, &all, &BTreeMap::new()).unwrap();
    assert!(
        plan.command
            .starts_with("PGPASSWORD=<vault: ACME_DB_PASSWORD> psql"),
        "{}",
        plan.command
    );
    let raw = env.runner().run(&plan, &secret).unwrap();
    assert!(!raw.stdout.lines().next().unwrap().contains(DB_PASSWORD));
    assert!(
        raw.stdout.contains("default_transaction_read_only=on"),
        "{}",
        raw.stdout
    );
    let out = redact::mask(raw, None, &[], &plan.limits);
    assert!(out.text.contains("password ••••••"), "{}", out.text);
}

#[cfg(unix)]
#[test]
fn a_test_says_what_is_missing_and_how_to_get_it() {
    let env = Fixture::new("test");
    env.fake("ssh", SSH);
    let loki = env.connection(
        r#"
name = "acme-loki"
kind = "loki"
env = "prod"
url = "https://loki.acme.test"
"#,
    );
    let checks = env
        .runner()
        .test(&loki, std::slice::from_ref(&loki), &|_| unreachable!())
        .unwrap();
    assert_eq!(checks.len(), 1);
    assert!(
        !checks[0].ok && checks[0].detail.contains("logcli"),
        "{checks:?}"
    );
    let ssh = env.connection(PROD_SSH);
    // Only the fakes: the machine running the tests may have a journalctl of its own (Linux).
    let only_fakes = Runner::new(Some(env.bin().into()));
    let checks = only_fakes
        .test(&ssh, std::slice::from_ref(&ssh), &|_| unreachable!())
        .unwrap();
    let journal = checks.iter().find(|c| c.code == "journal").unwrap();
    assert!(
        !journal.ok && journal.warn,
        "no journalctl here: {checks:?}"
    );
    env.fake("journalctl", "true");
    let checks = env
        .runner()
        .test(&ssh, std::slice::from_ref(&ssh), &|_| unreachable!())
        .unwrap();
    assert!(checks.iter().all(|c| c.ok), "{checks:?}");
    env.fake(
        "ssh",
        "echo 'deploy@acme-prod: Permission denied (publickey).' >&2; exit 255",
    );
    let checks = env
        .runner()
        .test(&ssh, std::slice::from_ref(&ssh), &|_| unreachable!())
        .unwrap();
    let connect = checks.iter().find(|c| c.code == "connect").unwrap();
    assert!(
        !connect.ok && connect.detail.contains("Permission denied"),
        "{checks:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_database_behind_a_server_gets_a_tunnel_closed_after_use() {
    if std::process::Command::new("python3")
        .arg("-V")
        .output()
        .is_err()
    {
        return;
    }
    let env = Fixture::new("tunnel");
    env.fake("ssh", SSH);
    env.fake("psql", r#"printf 'ARGS:%s\n' "$*"; echo "termf-ok|f""#);
    env.connection(PROD_SSH);
    let db = env.connection(
        r#"
name = "acme-db"
kind = "postgres"
env = "prod"
database = "acme"
user = "readonly"
via = "acme-prod"
"#,
    );
    let all = connections(&env.config(), "acme").unwrap();
    let checks = env.runner().test(&db, &all, &|_| unreachable!()).unwrap();
    let codes: Vec<(&str, bool)> = checks.iter().map(|c| (c.code, c.ok)).collect();
    assert_eq!(
        codes,
        [
            ("tool", true),
            ("tool", true),
            ("tunnel", true),
            ("connect", true),
            ("readonly", true)
        ],
        "{checks:?}"
    );
    let args = fs::read_to_string(env.root.join("ssh.args")).unwrap();
    assert!(
        args.contains("-N\n-o\nExitOnForwardFailure=yes\n-L\n127.0.0.1:"),
        "{args}"
    );
    assert!(args.contains(":127.0.0.1:5432\n"), "{args}");
    assert_eq!(
        args.matches("-N\n").count(),
        1,
        "one tunnel for the whole test: {args}"
    );
}

fn agent() -> Agent {
    Agent {
        session: 7,
        claude: "0f6c1d2e-aaaa-bbbb-cccc-1234567890ab".into(),
        run: Some(42),
        title: "CTO · Acme".into(),
        role: Some("cto".into()),
    }
}

/// Answers the first request that shows up, from another thread, as the user in the app would.
fn answer_soon(approvals: Arc<Approvals>, allow: bool) -> std::thread::JoinHandle<Option<Request>> {
    std::thread::spawn(move || {
        for _ in 0..200 {
            if let Some(r) = approvals.list().into_iter().next() {
                approvals.answer(r.id, allow);
                return Some(r);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    })
}

#[cfg(unix)]
#[test]
fn an_agent_gets_its_run_once_approved_masked_audited_and_its_session_marked() {
    let env = Fixture::new("agent");
    env.fake("ssh", SSH);
    env.fake("journalctl", JOURNALCTL);
    env.connection(PROD_SSH);
    env.connection(
        r#"
name = "acme-hidden"
kind = "ssh"
env = "staging"
host = "acme-staging"
"#,
    );
    let vault = env.vault();
    let human = Actor::human();
    vault.init(&human, PASSWORD, false).unwrap();
    vault.reauth(&human, PASSWORD).unwrap();
    let id = SecretId::new("ACME_DB_PASSWORD", Env::Prod, Some("acme")).unwrap();
    vault
        .set(&human, &id, Secret::new(DB_PASSWORD.into()), None)
        .unwrap();
    let runner = env.runner();
    let ops = Ops {
        config_dir: &env.config(),
        data_dir: &env.data(),
        vault: &vault,
        runner: &runner,
    };
    let seen = ops.seen("acme").unwrap();
    assert_eq!(
        seen.connections
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["acme-prod"]
    );
    assert!(
        seen.recipes
            .iter()
            .any(|r| r.name == "logs-tail" && r.connections == ["acme-prod"])
    );
    assert!(
        seen.recipes
            .iter()
            .any(|r| r.name == "service-restart" && r.proposal_only)
    );
    assert!(
        !seen.recipes.iter().any(|r| r.name.starts_with("k8s")),
        "no Kubernetes connection"
    );
    let approvals = Arc::new(Approvals::default());
    let shown = Mutex::new(Vec::new());
    let show = |event: &str, r: &Request| shown.lock().unwrap().push((event.to_owned(), r.id));
    let hidden = ops
        .agent_run(
            "acme",
            &agent(),
            "logs-tail",
            Some("acme-hidden"),
            &BTreeMap::new(),
            &approvals,
            APPROVAL_WAIT,
            &show,
            &|| false,
        )
        .unwrap_err();
    assert!(hidden.to_string().contains("visible to agents"), "{hidden}");
    assert!(
        shown.lock().unwrap().is_empty(),
        "nothing asked for a connection agents cannot see"
    );

    let user = answer_soon(approvals.clone(), true);
    let answer = ops
        .agent_run(
            "acme",
            &agent(),
            "logs-tail",
            None,
            &params(&[("grep", "api")]),
            &approvals,
            APPROVAL_WAIT,
            &show,
            &|| false,
        )
        .unwrap();
    let request = user.join().unwrap().unwrap();
    assert_eq!(request.kind, "approve");
    assert_eq!(request.plan.env, Env::Prod);
    assert!(
        request
            .plan
            .remote
            .as_deref()
            .unwrap()
            .contains("--grep api"),
        "{:?}",
        request.plan
    );
    let output = answer["output"].as_str().unwrap();
    assert!(
        !output.contains(DB_PASSWORD) && !output.contains("alice@example.com"),
        "{output}"
    );
    assert!(
        output.contains("[email]"),
        "redacted access masks personal data: {output}"
    );
    assert_eq!(
        shown
            .lock()
            .unwrap()
            .iter()
            .map(|(e, _)| e.as_str())
            .collect::<Vec<_>>(),
        ["ops-approval", "ops-approval-done"]
    );
    assert!(termf_core::taint::session(&env.data(), &agent().claude));
    assert!(termf_core::taint::run(&env.data(), 42));

    let user = answer_soon(approvals.clone(), false);
    let refused = ops
        .agent_run(
            "acme",
            &agent(),
            "logs-tail",
            None,
            &BTreeMap::new(),
            &approvals,
            APPROVAL_WAIT,
            &show,
            &|| false,
        )
        .unwrap_err();
    user.join().unwrap();
    assert!(refused.to_string().contains("refused"), "{refused}");
    let expired = ops
        .agent_run(
            "acme",
            &agent(),
            "logs-tail",
            None,
            &BTreeMap::new(),
            &approvals,
            Duration::from_millis(300),
            &show,
            &|| false,
        )
        .unwrap_err();
    assert!(expired.to_string().contains("Nobody approved"), "{expired}");
    let late = shown.lock().unwrap().last().unwrap().1;
    assert!(
        approvals.answer(late, true).is_none(),
        "an expired card takes no answer"
    );
    assert!(approvals.get(late).is_none());
    assert!(approvals.list().is_empty());

    let proposed = ops
        .agent_run(
            "acme",
            &agent(),
            "service-restart",
            None,
            &params(&[("unit", "api.service")]),
            &approvals,
            APPROVAL_WAIT,
            &show,
            &|| false,
        )
        .unwrap();
    assert_eq!(proposed["proposed"], true);
    let waiting = approvals.list();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].kind, "propose");
    assert!(
        !fs::read_to_string(env.root.join("ssh.args"))
            .unwrap()
            .contains("restart"),
        "a write ran for an agent"
    );

    let history = ops.history("acme", 10).unwrap();
    let approvals_seen: Vec<String> = history
        .iter()
        .map(|e| serde_json::from_str::<serde_json::Value>(e.detail.as_deref().unwrap()).unwrap()["approval"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        approvals_seen,
        ["proposed", "expired", "refused", "approved"]
    );
    let approved = history.last().unwrap();
    assert_eq!(approved.action, Action::Ops);
    assert_eq!(approved.target.as_deref(), Some("acme-prod"));
    assert_eq!(approved.actor.session.as_deref(), Some("7"));
    assert!(approved.masks.unwrap() >= 1 && approved.size.unwrap() > 0);
    assert!(vault.verify().unwrap().ok);
}

#[cfg(unix)]
#[test]
fn the_user_types_the_environment_to_run_a_recipe_that_writes() {
    let env = Fixture::new("write");
    env.fake("ssh", SSH);
    env.fake("sudo", r#"echo "sudo $*""#);
    env.connection(PROD_SSH);
    let vault = env.vault();
    let human = Actor::human();
    vault.init(&human, PASSWORD, false).unwrap();
    vault.lock(&human).unwrap();
    vault.unlock(&human, Some(PASSWORD), None, None).unwrap();
    let runner = env.runner();
    let ops = Ops {
        config_dir: &env.config(),
        data_dir: &env.data(),
        vault: &vault,
        runner: &runner,
    };
    let unit = params(&[("unit", "api.service")]);
    let typed = ops
        .run("acme", &human, "service-restart", "acme-prod", &unit, None)
        .unwrap_err();
    assert!(
        typed
            .to_string()
            .contains("type the environment's name (prod)"),
        "{typed}"
    );
    let wrong = ops
        .run(
            "acme",
            &human,
            "service-restart",
            "acme-prod",
            &unit,
            Some("staging"),
        )
        .unwrap_err();
    assert!(wrong.to_string().contains("(prod)"), "{wrong}");
    if vault.status().unwrap().prod_until.is_none() {
        let reauth = ops
            .run(
                "acme",
                &human,
                "service-restart",
                "acme-prod",
                &unit,
                Some("prod"),
            )
            .unwrap_err();
        assert!(vault_first(&reauth), "{reauth}");
        vault.reauth(&human, PASSWORD).unwrap();
    }
    let (plan, out) = ops
        .run(
            "acme",
            &human,
            "service-restart",
            "acme-prod",
            &unit,
            Some("prod"),
        )
        .unwrap();
    assert_eq!(plan.mode, Mode::Write);
    assert_eq!(out.text.trim(), "sudo -n systemctl restart api.service");
    let agent_tried = ops.run(
        "acme",
        &Actor::agent(None, None),
        "logs-tail",
        "acme-prod",
        &BTreeMap::new(),
        None,
    );
    assert!(agent_tried.is_err());
}

#[cfg(unix)]
#[test]
fn any_argument_reaches_the_shell_as_it_is() {
    use termf_core::rng::{AWKWARD, Rng};
    let mut rng = Rng::new(0x9007);
    let args: Vec<String> = (0..300)
        .map(|_| rng.text(AWKWARD, 16).replace('\0', ""))
        .collect();
    let script = args
        .iter()
        .map(|a| format!("printf '%s\\0' {}", quote(a)))
        .collect::<Vec<_>>()
        .join("; ");
    let out = std::process::Command::new("/bin/sh")
        .args(["-c", &script])
        .output()
        .unwrap();
    let got: Vec<&str> = std::str::from_utf8(&out.stdout)
        .unwrap()
        .split_terminator('\0')
        .collect();
    assert_eq!(
        got.len(),
        args.len(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for (got, arg) in got.iter().zip(&args) {
        assert_eq!(got, arg);
    }
}

#[test]
fn no_value_of_the_vault_survives_the_masking_nor_the_cut() {
    use termf_core::rng::{AWKWARD, Rng};
    let mask = termf_claude::mask::MASK;
    let mut rng = Rng::new(0x3a5c);
    for case in 0..2000 {
        let values: Vec<String> = (0..1 + rng.below(3))
            .map(|_| {
                loop {
                    let v = rng.text(AWKWARD, 20);
                    if v.len() >= 4 && !v.contains('•') {
                        break v;
                    }
                }
            })
            .collect();
        let mut stdout = String::new();
        for _ in 0..rng.below(8) {
            stdout.push_str(&rng.text(AWKWARD, 20));
            if rng.chance(2) {
                stdout.push_str(rng.pick(&values));
            }
        }
        let limits = Limits {
            lines: 1 + rng.below(6),
            bytes: rng.below(200),
            ..Limits::default()
        };
        let raw = Raw {
            stdout: stdout.clone(),
            stderr: stdout,
            used: values.iter().map(|v| Secret::new(v.clone())).collect(),
            ..Raw::default()
        };
        let personal: &[Redact] = if rng.chance(2) {
            &Redact::PERSONAL
        } else {
            &[]
        };
        let out = redact::mask(raw, None, personal, &limits);
        for v in &values {
            assert!(
                !out.text.contains(v.as_str()) && !out.errors.contains(v.as_str()),
                "case {case}: {v:?} in {:?}",
                out.text
            );
        }
        assert!(
            out.text.len() <= limits.bytes.max(mask.len()),
            "case {case}: {} bytes",
            out.text.len()
        );
        assert!(
            out.text.matches('\n').count() <= limits.lines,
            "case {case}"
        );
    }
}

/// A psql that refuses to answer unless TermF opened a read-only transaction with the timeout before the query.
#[cfg(unix)]
const PSQL: &str = r#"
ro=""; to=""; scs=""; q=""; prev=""
for a in "$@"; do
  if [ "$prev" = "-c" ]; then
    case "$a" in
      "START TRANSACTION READ ONLY") ro=1;;
      "SET LOCAL statement_timeout = 15000") [ -n "$ro" ] && to=1;;
      "SET LOCAL standard_conforming_strings = on") scs=1;;
      *) [ -n "$to" ] && [ -n "$scs" ] || { echo "the query came before the read-only transaction" >&2; exit 3; }; q="$a";;
    esac
  fi
  prev="$a"
done
case "$PGOPTIONS" in *default_transaction_read_only=on*statement_timeout=15000*) ;; *) echo "no read-only session: $PGOPTIONS" >&2; exit 3;; esac
[ -n "$q" ] || { echo "no query" >&2; exit 3; }
printf '%s' "$q" > "@DIR@/psql.query"
echo 'id,email,plan'
echo '1,alice@example.com,pro'
echo '2,bob@example.com,"team, yearly"'
"#;

/// The same for mysql: the session and the transaction read only, the timeout, then the query.
#[cfg(unix)]
const MYSQL: &str = r#"
init=""; script=""; prev=""
for a in "$@"; do
  case "$a" in --init-command=*) init="${a#--init-command=}";; esac
  [ "$prev" = "-e" ] && script="$a"
  prev="$a"
done
[ "$init" = "SET SESSION TRANSACTION READ ONLY" ] || { echo "no read-only session" >&2; exit 3; }
case "$script" in
  "SET SESSION max_execution_time = 15000;
START TRANSACTION READ ONLY;
SELECT"*) ;;
  *) echo "no timeout or read-only transaction before the query: $script" >&2; exit 3;;
esac
printf '%s' "$script" > "@DIR@/mysql.script"
printf 'id\temail\n1\talice@example.com\n'
"#;

#[cfg(unix)]
#[test]
fn the_sql_console_runs_one_read_in_a_read_only_transaction_and_refuses_the_rest() {
    let env = Fixture::new("sql");
    env.fake("psql", PSQL);
    env.fake("mysql", MYSQL);
    env.connection(
        r#"
name = "acme-db"
kind = "postgres"
env = "prod"
ai_access = "redacted"
host = "db.acme.test"
database = "acme"
user = "readonly"
"#,
    );
    env.connection(
        r#"
name = "acme-legacy"
kind = "mysql"
env = "staging"
host = "legacy.acme.test"
database = "acme"
user = "readonly"
"#,
    );
    let vault = env.vault();
    let runner = env.runner();
    let (config, data) = (env.config(), env.data());
    let ops = Ops {
        config_dir: &config,
        data_dir: &data,
        vault: &vault,
        runner: &runner,
    };
    let human = Actor::human();
    let query = params(&[("query", "select id, email, plan from users")]);
    let (plan, out) = ops
        .run("acme", &human, "sql-readonly", "acme-db", &query, None)
        .unwrap();
    assert_eq!(out.exit, Some(0), "{}", out.errors);
    assert_eq!(
        plan.params["query"],
        "select id, email, plan from users LIMIT 500"
    );
    assert_eq!(
        fs::read_to_string(env.root.join("psql.query")).unwrap(),
        plan.params["query"],
        "the client runs the query checked"
    );
    assert!(
        plan.command.contains("-c 'START TRANSACTION READ ONLY'"),
        "{}",
        plan.command
    );
    let table = out.table.clone().unwrap();
    assert_eq!(table.columns, ["id", "email", "plan"]);
    assert_eq!(
        table.rows,
        [
            vec!["1", "[email]", "pro"],
            vec!["2", "[email]", "team, yearly"]
        ]
    );
    assert!(!out.text.contains("alice@example.com"), "{}", out.text);

    let (_, out) = ops
        .run(
            "acme",
            &human,
            "sql-readonly",
            "acme-legacy",
            &params(&[("query", "SELECT id, email FROM users")]),
            None,
        )
        .unwrap();
    assert_eq!(out.exit, Some(0), "{}", out.errors);
    assert_eq!(out.table.unwrap().rows, [vec!["1", "[email]"]]);
    assert!(
        fs::read_to_string(env.root.join("mysql.script"))
            .unwrap()
            .ends_with("SELECT id, email FROM users LIMIT 500")
    );

    let runs = ops.history("acme", 50).unwrap().len();
    for (query, why) in [
        ("DELETE FROM users", "DELETE is not a read"),
        (
            "WITH gone AS (DELETE FROM users RETURNING *) SELECT * FROM gone",
            "DELETE is not a read",
        ),
        ("SELECT 1; DROP TABLE users", "one statement at a time"),
        ("EXPLAIN ANALYZE DELETE FROM users", "EXPLAIN ANALYZE runs"),
        ("SELECT pg_terminate_backend(42)", "pg_terminate_backend()"),
    ] {
        let e = ops
            .run(
                "acme",
                &human,
                "sql-readonly",
                "acme-db",
                &params(&[("query", query)]),
                None,
            )
            .unwrap_err();
        assert!(e.to_string().contains(why), "{query}: {e}");
    }
    assert_eq!(
        ops.history("acme", 50).unwrap().len(),
        runs,
        "nothing ran for a refused query"
    );

    let approvals = Arc::new(Approvals::default());
    let user = answer_soon(approvals.clone(), true);
    let answer = ops
        .agent_run(
            "acme",
            &agent(),
            "sql-readonly",
            None,
            &query,
            &approvals,
            APPROVAL_WAIT,
            &|_, _| {},
            &|| false,
        )
        .unwrap();
    let request = user.join().unwrap().unwrap();
    assert!(
        request.plan.command.contains("LIMIT 500"),
        "the card shows the query that runs"
    );
    let output = answer["output"].as_str().unwrap();
    assert!(
        output.contains("[email]") && !output.contains("alice@example.com"),
        "{output}"
    );
    let refused = ops
        .agent_run(
            "acme",
            &agent(),
            "sql-readonly",
            None,
            &params(&[("query", "UPDATE users SET plan = 'free'")]),
            &approvals,
            APPROVAL_WAIT,
            &|_, _| {},
            &|| false,
        )
        .unwrap_err();
    assert!(refused.to_string().contains("UPDATE is not a read"));
    assert!(approvals.list().is_empty(), "no card for a refused query");
}

#[cfg(unix)]
const DOTENV: &str = "# Acme prod\nDATABASE_URL=postgres://app:Pg-4f8a9c2e7b1d@db/acme\nSTRIPE_KEY=sk_live_51HabcdefGHIJ\nAPP_ENV=production\n";

#[cfg(unix)]
#[test]
fn a_production_env_is_masked_revealed_one_by_one_and_an_agent_gets_only_its_names() {
    let env = Fixture::new("dotenv");
    env.fake("ssh", SSH);
    fs::write(env.root.join("prod.env"), DOTENV).unwrap();
    env.connection(PROD_SSH);
    let vault = env.vault();
    let runner = env.runner();
    let (config, data) = (env.config(), env.data());
    let ops = Ops {
        config_dir: &config,
        data_dir: &data,
        vault: &vault,
        runner: &runner,
    };
    let path = env.root.join("prod.env").to_string_lossy().into_owned();
    let given = params(&[("path", &path)]);
    let (plan, out) = ops
        .run(
            "acme",
            &Actor::human(),
            "env-fetch",
            "acme-prod",
            &given,
            None,
        )
        .unwrap();
    assert_eq!(out.keys, ["DATABASE_URL", "STRIPE_KEY", "APP_ENV"]);
    assert_eq!(
        out.hidden.iter().map(Secret::expose).collect::<Vec<_>>(),
        [
            "postgres://app:Pg-4f8a9c2e7b1d@db/acme",
            "sk_live_51HabcdefGHIJ",
            "production"
        ]
    );
    let shown = serde_json::to_string(&out).unwrap();
    for value in ["Pg-4f8a9c2e7b1d", "sk_live_51HabcdefGHIJ", "production"] {
        assert!(!shown.contains(value), "{value} in {shown}");
    }
    assert!(
        out.text.starts_with("DATABASE_URL=••••••\n"),
        "{}",
        out.text
    );

    ops.record_use("acme", &plan, Use::Reveal, "STRIPE_KEY", 21, None)
        .unwrap();
    ops.record_use("acme", &plan, Use::Write(42), "STRIPE_KEY", 21, None)
        .unwrap();
    let details: Vec<String> = ops
        .history("acme", 10)
        .unwrap()
        .iter()
        .map(|e| e.detail.clone().unwrap_or_default())
        .collect();
    assert!(
        details[0].contains("\"event\":\"write\"") && details[0].contains("\"run\":42"),
        "{details:?}"
    );
    assert!(details[1].contains("\"event\":\"reveal\"") && details[1].contains("STRIPE_KEY"));
    assert!(!details.concat().contains("sk_live"), "{details:?}");

    let local =
        env::parse("DATABASE_URL=postgres://dev@localhost/acme\nAPP_ENV=production\nDEBUG=1\n");
    let prod: Vec<(String, Secret)> = out.keys.iter().cloned().zip(out.hidden).collect();
    let states: Vec<(String, env::State)> = env::compare(&prod, &local)
        .into_iter()
        .map(|d| (d.key, d.state))
        .collect();
    assert_eq!(
        states,
        [
            ("DATABASE_URL".into(), env::State::Different),
            ("STRIPE_KEY".into(), env::State::Prod),
            ("APP_ENV".into(), env::State::Same),
            ("DEBUG".into(), env::State::Local),
        ]
    );

    let approvals = Arc::new(Approvals::default());
    let user = answer_soon(approvals.clone(), true);
    let answer = ops
        .agent_run(
            "acme",
            &agent(),
            "env-fetch",
            None,
            &given,
            &approvals,
            APPROVAL_WAIT,
            &|_, _| {},
            &|| false,
        )
        .unwrap();
    user.join().unwrap().unwrap();
    assert_eq!(
        answer["keys"],
        serde_json::json!(["DATABASE_URL", "STRIPE_KEY", "APP_ENV"])
    );
    let said = answer.to_string();
    for value in ["Pg-4f8a9c2e7b1d", "sk_live_51HabcdefGHIJ", "production"] {
        assert!(!said.contains(value), "{value} given to the agent: {said}");
    }
    assert!(answer.get("output").is_none());
}

#[cfg(unix)]
#[test]
fn a_value_written_into_a_run_worktree_is_private_hidden_from_git_and_goes_with_it() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    let env = Fixture::new("envwrite");
    let repo = env.root.join("acme-api");
    let git = |dir: &Path, args: &[&str]| {
        let out = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=alex", "-c", "user.email=alex@acme.test"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "start"]);
    let worktree = env.root.join("wt-42");
    let path = worktree.to_string_lossy().into_owned();
    git(&repo, &["worktree", "add", "-q", "-b", "termf/42", &path]);
    // A quoted value over several lines, replaced whole: none of its lines is left to break the file.
    fs::write(
        worktree.join(".env"),
        "APP_ENV=development\nPEM=\"-----BEGIN KEY-----\nAAAA\n-----END KEY-----\"\nDEBUG=1\n",
    )
    .unwrap();

    env::write(
        &worktree,
        "STRIPE_KEY",
        &Secret::new("sk_live 51H'x".into()),
    )
    .unwrap();
    env::write(&worktree, "APP_ENV", &Secret::new("production".into())).unwrap();
    env::write(&worktree, "PEM", &Secret::new("line 1\nline 2".into())).unwrap();
    let file = worktree.join(".env");
    let written = fs::read_to_string(&file).unwrap();
    assert!(
        !written.contains("AAAA") && !written.contains("END KEY"),
        "{written}"
    );
    assert_eq!(
        env::parse(&written),
        [
            ("DEBUG".to_owned(), "1".to_owned()),
            ("STRIPE_KEY".to_owned(), "sk_live 51H'x".to_owned()),
            ("APP_ENV".to_owned(), "production".to_owned()),
            ("PEM".to_owned(), "line 1\nline 2".to_owned()),
        ]
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        fs::read_to_string(repo.join(".git/info/exclude"))
            .unwrap()
            .lines()
            .any(|l| l == "/.env")
    );
    assert_eq!(
        git(&worktree, &["status", "--porcelain"]),
        "",
        "git sees it"
    );
    // The run's cleanup removes its worktree, and the .env with it.
    git(&repo, &["worktree", "remove", &path]);
    assert!(!file.exists());

    fs::write(repo.join(".env"), "X=1\n").unwrap();
    git(&repo, &["add", "-f", ".env"]);
    let tracked = env::write(&repo, "X", &Secret::new("2".into())).unwrap_err();
    assert!(tracked.to_string().contains("tracked by git"), "{tracked}");
}

/// Waits for `done`, a minute at most: the fake clients are shell scripts, slow to start on a loaded machine.
#[cfg(unix)]
fn eventually(done: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while !done() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(unix)]
#[test]
fn logs_are_followed_masked_until_stopped_and_never_by_an_agent() {
    let env = Fixture::new("follow");
    env.fake("ssh", SSH);
    env.fake(
        "journalctl",
        r#"echo $$ > "@DIR@/journal.pid"
echo "10:00:01 api: login ok for alice@example.com"
echo "10:00:02 api: ERROR payment failed"
exec sleep 30"#,
    );
    env.connection(PROD_SSH);
    let vault = env.vault();
    let runner = env.runner();
    let (config, data) = (env.config(), env.data());
    let ops = Ops {
        config_dir: &config,
        data_dir: &data,
        vault: &vault,
        runner: &runner,
    };
    let got = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&got);
    let (plan, following) = ops
        .follow(
            "acme",
            "logs-follow",
            "acme-prod",
            &params(&[("unit", "api.service")]),
            move |s| sink.lock().unwrap().push(s),
        )
        .unwrap();
    assert!(
        plan.remote
            .as_deref()
            .unwrap()
            .contains("-f -u api.service"),
        "{:?}",
        plan.remote
    );
    let lines = || -> Vec<String> {
        got.lock()
            .unwrap()
            .iter()
            .flat_map(|s| match s {
                Streamed::Lines { lines, .. } => lines.clone(),
                Streamed::End { .. } => Vec::new(),
            })
            .collect()
    };
    eventually(|| lines().iter().any(|l| l.contains("ERROR")));
    let seen = lines();
    assert!(
        seen.iter().any(|l| l.contains("ERROR payment failed")),
        "{seen:?}"
    );
    assert!(
        seen.iter().any(|l| l.contains("[email]")) && !seen.concat().contains("alice@"),
        "{seen:?}"
    );
    let pid = fs::read_to_string(env.root.join("journal.pid")).unwrap();
    let alive = || {
        std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    };
    assert!(alive());
    assert!(following.stop(), "a running client is stopped");
    drop(following);
    eventually(|| !alive());
    assert!(!alive(), "the followed client outlived its stop");
    let history = ops.history("acme", 5).unwrap();
    assert!(
        history[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("\"event\":\"follow\"")
    );

    let seen = ops.seen("acme").unwrap();
    assert!(!seen.recipes.iter().any(|r| r.name == "logs-follow"));
    let refused = ops
        .agent_run(
            "acme",
            &agent(),
            "logs-follow",
            None,
            &params(&[("unit", "api.service")]),
            &Approvals::default(),
            APPROVAL_WAIT,
            &|_, _| {},
            &|| false,
        )
        .unwrap_err();
    assert!(
        refused.to_string().contains("no recipe logs-follow"),
        "{refused}"
    );
}

#[cfg(unix)]
#[test]
fn a_stream_that_ended_is_never_signalled_and_one_that_cannot_start_is_not_audited() {
    let env = Fixture::new("ended");
    env.fake("ssh", SSH);
    env.fake("journalctl", r#"echo "10:00:01 api: started"; exit 0"#);
    env.connection(PROD_SSH);
    env.connection(
        r#"
name = "acme-loki"
kind = "loki"
env = "prod"
url = "https://loki.acme.test"
"#,
    );
    let vault = env.vault();
    let runner = env.runner();
    let (config, data) = (env.config(), env.data());
    let ops = Ops {
        config_dir: &config,
        data_dir: &data,
        vault: &vault,
        runner: &runner,
    };
    let got = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&got);
    let (_, following) = ops
        .follow(
            "acme",
            "logs-follow",
            "acme-prod",
            &BTreeMap::new(),
            move |s| sink.lock().unwrap().push(s),
        )
        .unwrap();
    let ended = || {
        got.lock().unwrap().iter().find_map(|s| match s {
            Streamed::End { exit, .. } => Some(*exit),
            Streamed::Lines { .. } => None,
        })
    };
    eventually(|| ended().is_some());
    assert_eq!(ended(), Some(Some(0)), "{:?}", got.lock().unwrap());
    // Its number (and its group's) may be another program's by now.
    assert!(!following.stop(), "an ended client was signalled");
    drop(following);

    let entries = ops.history("acme", 50).unwrap().len();
    let missing = ops
        .follow(
            "acme",
            "logs-follow",
            "acme-loki",
            &params(&[("query", "{app=\"web\"}")]),
            |_| {},
        )
        .err()
        .expect("no logcli in the test's PATH");
    assert!(
        missing.to_string().contains("logcli is not installed"),
        "{missing}"
    );
    assert_eq!(
        ops.history("acme", 50).unwrap().len(),
        entries,
        "audited a stream that never started"
    );
    let plan = ops
        .plan(
            "acme",
            "env-fetch",
            "acme-prod",
            &params(&[("path", "/srv/.env")]),
        )
        .unwrap();
    ops.record_use(
        "acme",
        &plan,
        Use::Write(42),
        "APP_ENV",
        10,
        Some(".env is tracked by git".into()),
    )
    .unwrap();
    let last = ops.history("acme", 1).unwrap().remove(0);
    let detail = last.detail.unwrap();
    assert!(
        detail.contains("\"event\":\"write\"") && detail.contains("tracked by git"),
        "{detail}"
    );
}

#[test]
fn a_query_without_rows_is_an_empty_table_and_a_cut_one_says_so() {
    let limits = Limits {
        rows: Some(500),
        ..Limits::default()
    };
    let raw = |stdout: &str, exit: Option<i32>, stopped: Option<&str>| Raw {
        stdout: stdout.into(),
        exit,
        stopped: stopped.map(Into::into),
        ..Raw::default()
    };
    let none = redact::table(raw("", Some(0), None), Kind::Mysql, None, &[], &limits);
    let table = none.table.expect("a query without rows has a table");
    assert!(table.rows.is_empty() && !none.truncated);
    let failed = redact::table(raw("", Some(1), None), Kind::Mysql, None, &[], &limits);
    assert!(failed.table.is_none(), "a failed query has no table");
    let cut = redact::table(
        raw("id,total\n1,1240.00\n2,12", Some(0), None),
        Kind::Postgres,
        None,
        &[],
        &limits,
    );
    assert_eq!(cut.table.unwrap().rows, [vec!["1", "1240.00"]]);
    assert!(cut.truncated);
    // A long last line stopped in pieces: each looks whole, the last is not shown.
    let stopped = redact::table(
        raw(
            "id\tnote\n1\tshort\n2\taaaaaaaa\n",
            None,
            Some("stopped past 20 bytes"),
        ),
        Kind::Mysql,
        None,
        &[],
        &limits,
    );
    assert_eq!(stopped.table.unwrap().rows, [vec!["1", "short"]]);
    assert!(stopped.truncated);
}

/// The system's own `sqlite3` on a throwaway file: a query, a write refused by the analyzer and by the read-only
/// file, the schema, a full export past the console's rows, a plan for the DBA, each audited with its event.
#[cfg(unix)]
#[test]
fn a_real_sqlite_database_reads_and_never_writes() {
    if !Path::new("/usr/bin/sqlite3").is_file() {
        eprintln!("no /usr/bin/sqlite3: skipped");
        return;
    }
    let env = Fixture::new("sqlite");
    let db = env.root.join("acme.db");
    let mut seed = String::from(
        "CREATE TABLE users(id INTEGER PRIMARY KEY, email TEXT NOT NULL UNIQUE, note TEXT);\
         CREATE INDEX users_note ON users(note);",
    );
    for i in 1..=600 {
        seed.push_str(&format!(
            "INSERT INTO users VALUES ({i}, 'u{i}@acme.dev', 'n, \"{i}\"');"
        ));
    }
    let made = std::process::Command::new("/usr/bin/sqlite3")
        .arg(&db)
        .arg(&seed)
        .status()
        .unwrap();
    assert!(made.success());
    let conn = env.connection(&format!(
        "name = \"local\"\nkind = \"sqlite\"\nenv = \"dev\"\ndatabase = \"{}\"\n",
        db.display()
    ));
    let vault = env.vault();
    vault.init(&Actor::human(), PASSWORD, false).unwrap();
    let runner = env.runner();
    let (config, data) = (env.config(), env.data());
    let ops = Ops {
        config_dir: &config,
        data_dir: &data,
        vault: &vault,
        runner: &runner,
    };
    let human = Actor::human();
    let query = params(&[("query", "SELECT id, email, note FROM users ORDER BY id")]);
    let (plan, out) = ops
        .run("acme", &human, "sql-readonly", "local", &query, None)
        .unwrap();
    assert!(
        plan.command.contains("sqlite3 -readonly -safe"),
        "{}",
        plan.command
    );
    let table = out.table.unwrap();
    assert_eq!(table.columns, ["id", "email", "note"]);
    assert_eq!(table.rows.len(), 500, "the console's LIMIT");
    assert_eq!(table.rows[0], ["1", "[email]", "n, \"1\""]);

    for write in [
        "DELETE FROM users",
        "SELECT writefile('x.txt', 'y')",
        "ATTACH 'other.db' AS o",
        "PRAGMA journal_mode = DELETE",
    ] {
        let given = params(&[("query", write)]);
        assert!(
            ops.run("acme", &human, "sql-readonly", "local", &given, None)
                .is_err(),
            "{write}"
        );
    }
    // Past the analyzer, the file opened read-only still refuses.
    let mut forced = plan.clone();
    forced.args = vec!["DELETE FROM users".into()];
    let raw = runner.run(&forced, &|_| unreachable!()).unwrap();
    assert_ne!(raw.exit, Some(0));
    assert!(raw.stderr.contains("readonly"), "{}", raw.stderr);

    let schema = ops.schema("acme", "local").unwrap();
    assert_eq!(schema.len(), 1);
    let users = &schema[0];
    assert_eq!(users.name, "users");
    let cols: Vec<(&str, &str, &str)> = users
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.kind.as_str(), c.key.as_str()))
        .collect();
    assert_eq!(
        cols,
        [
            ("id", "INTEGER", "pk"),
            ("email", "TEXT", ""),
            ("note", "TEXT", "")
        ]
    );
    assert!(!users.columns[1].nullable && users.columns[2].nullable);
    assert!(
        users
            .indexes
            .iter()
            .any(|i| i.name == "users_note" && i.columns == "note" && !i.unique)
    );
    assert!(
        users
            .indexes
            .iter()
            .any(|i| i.unique && i.columns == "email")
    );

    let (_, all) = ops.export("acme", "local", "SELECT id FROM users").unwrap();
    assert_eq!(
        all.table.unwrap().rows.len(),
        600,
        "an export reads past the console's rows"
    );

    let explained = ops
        .explain("acme", "local", "SELECT * FROM users WHERE note = 'x';")
        .unwrap();
    assert!(explained.text.contains("users_note"), "{}", explained.text);

    let checks = runner
        .test(&conn, std::slice::from_ref(&conn), &|_| unreachable!())
        .unwrap();
    assert!(checks.iter().all(|c| c.ok && !c.warn), "{checks:?}");

    let events: Vec<String> = ops
        .history("acme", 50)
        .unwrap()
        .iter()
        .map(|e| {
            let d: serde_json::Value = serde_json::from_str(e.detail.as_deref().unwrap()).unwrap();
            d["event"].as_str().unwrap_or("query").to_owned()
        })
        .collect();
    for event in ["schema", "export", "dba", "query"] {
        assert!(events.iter().any(|e| e == event), "{event} in {events:?}");
    }
}

#[test]
fn saved_queries_live_in_the_project_file_and_one_becomes_a_recipe() {
    let env = Fixture::new("queries");
    env.connection(
        "name = \"shop\"\nkind = \"mysql\"\nenv = \"prod\"\nhost = \"db\"\nuser = \"ro\"\ndatabase = \"shop\"\nai_access = \"redacted\"\n",
    );
    let config = env.config();
    let q = Query {
        name: "Orders of the day".into(),
        description: "Today's paid orders".into(),
        folder: "Sales".into(),
        tags: vec!["daily".into()],
        connection: Some("shop".into()),
        sql: "SELECT id, total\nFROM orders\nWHERE paid_at > CURDATE()".into(),
    };
    let saved = save_query(&config, "acme", &q, None).unwrap();
    assert_eq!(saved, std::slice::from_ref(&q));
    let file = fs::read_to_string(config.join("projects/acme.toml")).unwrap();
    assert!(
        file.starts_with("# Acme's project"),
        "comments kept: {file}"
    );
    assert!(file.contains("[[ops.queries]]"), "{file}");
    let twice = save_query(&config, "acme", &q, None).unwrap_err();
    assert!(twice.to_string().contains("already named"));
    let copy = Query {
        name: "Orders of the day (copy)".into(),
        ..q.clone()
    };
    assert_eq!(save_query(&config, "acme", &copy, None).unwrap().len(), 2);
    let renamed = Query {
        name: "Daily orders".into(),
        ..copy.clone()
    };
    let after = save_query(&config, "acme", &renamed, Some(&copy.name)).unwrap();
    let names: Vec<&str> = after.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, ["Orders of the day", "Daily orders"]);
    let elsewhere = Query {
        connection: Some("nope".into()),
        ..renamed.clone()
    };
    assert!(save_query(&config, "acme", &elsewhere, Some("Daily orders")).is_err());
    assert_eq!(
        delete_query(&config, "acme", "Daily orders").unwrap().len(),
        1
    );
    assert_eq!(connections(&config, "acme").unwrap().len(), 1);

    let name = query_recipe(&config, "acme", "Orders of the day").unwrap();
    assert_eq!(name, "query-orders-of-the-day");
    assert!(
        query_recipe(&config, "acme", "Orders of the day").is_err(),
        "never overwritten"
    );
    let (all, errors) = recipes(&config, Some("acme"));
    assert!(errors.is_empty(), "{errors:?}");
    let recipe = all.iter().find(|r| r.name == name).unwrap();
    assert_eq!(
        (recipe.mode, recipe.approval, recipe.ai_access),
        (Mode::Read, Approval::Always, AiAccess::Redacted)
    );
    let conns = connections(&config, "acme").unwrap();
    let plan = crate::plan(recipe, &conns[0], &conns, &BTreeMap::new()).unwrap();
    assert!(
        plan.params["query"].starts_with("SELECT id, total"),
        "its saved query by default"
    );
    let other = params(&[("query", "SELECT * FROM users")]);
    let e = crate::plan(recipe, &conns[0], &conns, &other).unwrap_err();
    assert!(format!("{e:#}").contains("only its saved query"), "{e:#}");
}

#[test]
fn a_failure_says_what_is_missing() {
    for (said, hint) in [
        ("alex@bastion: Permission denied (publickey).", "keyRefused"),
        (
            "ssh: Could not resolve hostname db.acme.dev: nodename nor servname provided",
            "unknownHost",
        ),
        (
            "ssh: connect to host 10.0.0.4 port 22: Connection refused",
            "portClosed",
        ),
        (
            "ssh: connect to host 10.0.0.4 port 22: Operation timed out",
            "timeout",
        ),
        ("Host key verification failed.", "hostKey"),
        (
            "psql: error: connection to server at \"db\" failed: FATAL:  role \"reader\" does not exist",
            "passwordRefused",
        ),
        (
            "ERROR 2003 (HY000): Can't connect to MySQL server on 'db.acme.dev:3306' (110)",
            "timeout",
        ),
        (
            "ERROR 2003 (HY000): Can't connect to MySQL server on 'db.acme.dev:3306' (111)",
            "portClosed",
        ),
        (
            "psql: error: FATAL:  password authentication failed for user \"ro\"",
            "passwordRefused",
        ),
        (
            "ERROR 1045 (28000): Access denied for user 'ro'@'10.0.0.1' (using password: YES)",
            "passwordRefused",
        ),
        (
            "psql: error: FATAL:  database \"acme\" does not exist",
            "noDatabase",
        ),
        (
            "ERROR 2002 (HY000): Can't connect to MySQL server on '127.0.0.1' (61)",
            "portClosed",
        ),
        (
            "Error: unable to open database \"/srv/x.db\": unable to open database file",
            "noFile",
        ),
    ] {
        assert_eq!(diagnose(said), Some(hint), "{said}");
    }
    assert_eq!(diagnose("something else"), None);
}

/// A shop's SQLite database, its connection `shop`, and Ops over it.
#[cfg(unix)]
fn shop(env: &Fixture) -> Connection {
    let db = env.root.join("shop.db");
    let seed = "CREATE TABLE customers(id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT UNIQUE, country TEXT);\
        CREATE TABLE orders(id INTEGER PRIMARY KEY, customer_id INTEGER NOT NULL REFERENCES customers(id), status TEXT NOT NULL, total_eur NUMERIC NOT NULL, paid_at TEXT);\
        CREATE INDEX orders_customer ON orders(customer_id);\
        INSERT INTO customers VALUES (1, 'Northwind', 'a@northwind.test', 'FR'), (2, 'Kestrel', 'b@kestrel.test', 'DE'), (3, 'Vega', 'c@vega.test', 'FR');\
        INSERT INTO orders VALUES (1, 1, 'paid', 120.5, '2026-11-02'), (2, 1, 'paid', 80, '2026-11-12'), (3, 2, 'paid', 300, '2026-11-20'), (4, 3, 'refunded', 50, '2026-11-21'), (5, 3, 'paid', 10, '2026-10-30');";
    let made = std::process::Command::new("/usr/bin/sqlite3")
        .arg(&db)
        .arg(seed)
        .status()
        .unwrap();
    assert!(made.success());
    env.connection(&format!(
        "name = \"shop\"\nkind = \"sqlite\"\nenv = \"dev\"\ndatabase = \"{}\"\n",
        db.display()
    ))
}

/// One question to the console's DBA as the app asks it (`ops_ask`): its schema in the first message, the `dba`
/// role, no tool that writes or reaches out; then its query run by the console. `claude`: the agent's program,
/// `env`: added to its environment. The answer, its cost, and the table its query gave.
#[cfg(unix)]
fn ask_dba(
    env: &Fixture,
    claude: &Path,
    extra: Vec<(String, String)>,
) -> (String, f64, crate::sql::Table) {
    let conn = shop(env);
    let vault = env.vault();
    let runner = env.runner();
    let (config, data) = (env.config(), env.data());
    let ops = Ops {
        config_dir: &config,
        data_dir: &data,
        vault: &vault,
        runner: &runner,
    };
    let schema = ops.schema("acme", "shop").unwrap();
    let ask = crate::dba::Ask::Write(
        "le chiffre d'affaires payé par pays en novembre 2026, du plus grand au plus petit",
    );
    let prompt = crate::dba::first(&conn, &schema, false, &ask, "fr");
    let repo = env.root.join("repo");
    let agents_config = env.root.join("agents-config");
    fs::create_dir_all(&repo).unwrap();
    fs::create_dir_all(&agents_config).unwrap();
    fs::write(
        agents_config.join("termf.toml"),
        format!("[agents]\nclaude = {:?}\n", claude.display().to_string()),
    )
    .unwrap();
    let agents =
        termf_agents::Agents::build(agents_config, env.root.join("agents-data"), extra, |_| {});
    let spec = termf_agents::SessionSpec {
        cwd: repo,
        title: "DBA · shop".into(),
        role: Some("dba".into()),
        model: Some("haiku".into()),
        effort: Some("low".into()),
        budget: Some(0.30),
        allowed_tools: ["Read", "Grep", "Glob"].map(String::from).to_vec(),
        disallowed_tools: [
            "Bash",
            "PowerShell",
            "Edit",
            "Write",
            "NotebookEdit",
            "WebFetch",
            "WebSearch",
        ]
        .map(String::from)
        .to_vec(),
        ..termf_agents::SessionSpec::default()
    };
    let session = agents.start(spec, Some(&prompt)).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    let (answer, cost) = loop {
        let events = agents.events(session.id, 0).unwrap();
        let end = events.iter().find_map(|e| match &e.event {
            termf_agents::AgentEvent::TurnEnd { cost, .. } => Some(*cost),
            _ => None,
        });
        if let Some(cost) = end {
            let text = events
                .iter()
                .filter_map(|e| match &e.event {
                    termf_agents::AgentEvent::Text { text, parent: None } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            break (text, cost);
        }
        assert!(std::time::Instant::now() < deadline, "{events:#?}");
        std::thread::sleep(Duration::from_millis(200));
    };
    let _ = agents.stop(session.id);
    let fence = answer.find("```sql").expect("a ```sql block");
    let rest = &answer[fence + 6..];
    let query = rest[..rest.find("```").expect("its end")].trim();
    let (_, out) = ops
        .run(
            "acme",
            &Actor::human(),
            "sql-readonly",
            "shop",
            &params(&[("query", query)]),
            None,
        )
        .unwrap();
    assert_eq!(out.exit, Some(0), "{}", out.errors);
    (answer, cost, out.table.unwrap())
}

/// With a stand-in for Claude Code: what the DBA gets (the schema, the language, the role's limits), and its
/// answer's query, which the console runs on the real database.
#[cfg(unix)]
#[test]
fn the_dba_gets_the_schema_and_its_query_runs_in_the_console() {
    if !Path::new("/usr/bin/sqlite3").is_file() {
        eprintln!("no /usr/bin/sqlite3: skipped");
        return;
    }
    let env = Fixture::new("dba");
    let sql = "SELECT c.country, sum(o.total_eur) AS total FROM orders o JOIN customers c ON c.id = o.customer_id WHERE o.status = 'paid' AND o.paid_at >= '2026-11-01' GROUP BY c.country ORDER BY total DESC";
    let text = format!("```sql\n{sql}\n```\nLe chiffre d'affaires payé, par pays.");
    let usage = serde_json::json!({"input_tokens": 5, "output_tokens": 7, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0});
    let lines = [
        serde_json::json!({"type": "system", "subtype": "init", "session_id": "@SID@", "model": "claude-haiku-5-5", "permissionMode": "default"}),
        serde_json::json!({"type": "stream_event", "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "```sql\nSELECT"}}, "parent_tool_use_id": null}),
        serde_json::json!({"type": "assistant", "message": {"model": "claude-haiku-5-5", "content": [{"type": "text", "text": text}], "usage": usage}, "parent_tool_use_id": null}),
        serde_json::json!({"type": "result", "subtype": "success", "total_cost_usd": 0.01, "usage": usage, "modelUsage": {"claude-haiku-5-5": {"contextWindow": 200000}}, "session_id": "@SID@"}),
    ];
    let answer_file = env.root.join("answer.jsonl");
    fs::write(
        &answer_file,
        lines.iter().map(|l| format!("{l}\n")).collect::<String>(),
    )
    .unwrap();
    env.fake(
        "claude",
        r#"[ "$1" = --version ] && { echo '2.1.330 (Claude Code)'; exit 0; }
printf '%s\n' "$@" > "@DIR@/claude.args"
prev=
for a in "$@"; do
  if [ "$prev" = --session-id ] || [ "$prev" = --resume ]; then sid=$a; fi
  prev=$a
done
while IFS= read -r line; do
  printf '%s\n' "$line" >> "@DIR@/claude.stdin"
  case "$line" in *'"type":"user"'*) ;; *) continue ;; esac
  sed "s/@SID@/$sid/g" "@DIR@/answer.jsonl"
done"#,
    );
    let (answer, _, table) = ask_dba(&env, &env.bin().join("claude"), Vec::new());
    assert!(answer.contains("```sql"), "{answer}");
    assert_eq!(table.rows, [vec!["DE", "300"], vec!["FR", "200.5"]]);
    let stdin = fs::read_to_string(env.root.join("claude.stdin")).unwrap();
    assert!(
        stdin.contains("orders(id INTEGER pk, customer_id INTEGER fk not null"),
        "{stdin}"
    );
    assert!(stdin.contains("Answer in French") && stdin.contains("You see no rows"));
    let args = fs::read_to_string(env.root.join("claude.args")).unwrap();
    for word in [
        "--model\nhaiku",
        "--disallowedTools\nBash",
        "--disallowedTools\nWebFetch",
    ] {
        assert!(args.contains(word), "{word} in {args}");
    }
}

/// The same with the real Claude Code (Haiku, 0.30 $ at most). Not run by the automatic checks: it needs the
/// user's Claude Code login. `TERMF_REAL_CLAUDE=<claude binary> DISABLE_AUTOUPDATER=1 cargo test -p termf-ops --
/// --ignored real_dba --nocapture`
#[cfg(unix)]
#[test]
#[ignore]
fn real_dba() {
    let claude = std::env::var("TERMF_REAL_CLAUDE").expect("TERMF_REAL_CLAUDE: a claude binary");
    let env = Fixture::new("real-dba");
    let (answer, cost, table) = ask_dba(
        &env,
        Path::new(&claude),
        vec![("DISABLE_AUTOUPDATER".into(), "1".into())],
    );
    eprintln!(
        "DBA ({cost:.4} $):\n{answer}\n{}",
        crate::sql::render(&table)
    );
    assert_eq!(table.rows.len(), 2, "DE then FR: {table:?}");
}

/// A full export reaches its 100 000 rows, cells on several lines included, and says what it left.
#[cfg(unix)]
#[test]
fn a_full_export_reaches_its_limit_of_rows() {
    if !Path::new("/usr/bin/sqlite3").is_file() {
        eprintln!("no /usr/bin/sqlite3: skipped");
        return;
    }
    let env = Fixture::new("export-limit");
    let db = env.root.join("big.db");
    let seed = "CREATE TABLE t(id INTEGER PRIMARY KEY, note TEXT);\
        WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 100005)\
        INSERT INTO t SELECT x, 'a' || char(10) || 'b' FROM n;";
    let made = std::process::Command::new("/usr/bin/sqlite3")
        .arg(&db)
        .arg(seed)
        .status()
        .unwrap();
    assert!(made.success());
    env.connection(&format!(
        "name = \"big\"\nkind = \"sqlite\"\nenv = \"dev\"\ndatabase = \"{}\"\n",
        db.display()
    ));
    let vault = env.vault();
    let runner = env.runner();
    let (config, data) = (env.config(), env.data());
    let ops = Ops {
        config_dir: &config,
        data_dir: &data,
        vault: &vault,
        runner: &runner,
    };
    let (_, out) = ops.export("acme", "big", "SELECT id, note FROM t").unwrap();
    let table = out.table.unwrap();
    assert_eq!(table.rows.len(), EXPORT_ROWS);
    assert_eq!(table.rows[0], ["1", "a\nb"]);
    assert!(out.truncated, "rows were left");
}

#[test]
fn a_saved_query_that_does_not_read_hides_nothing_else() {
    let env = Fixture::new("bad-query");
    env.connection(
        "name = \"shop\"\nkind = \"mysql\"\nenv = \"prod\"\nhost = \"db\"\nuser = \"ro\"\ndatabase = \"shop\"\n",
    );
    let config = env.config();
    let file = config.join("projects/acme.toml");
    let mut text = fs::read_to_string(&file).unwrap();
    text.push_str(
        "\n[[ops.queries]]\nname = \"Good\"\nsql = \"SELECT 1\"\n\n[[ops.queries]]\nname = \"Typo\"\nsql = \"SELECT 2\"\nfoldr = \"x\"\n\n[[ops.queries]]\nsql = \"SELECT 3\"\n",
    );
    fs::write(&file, text).unwrap();
    assert_eq!(connections(&config, "acme").unwrap().len(), 1);
    let (found, errors) = queries(&config, "acme").unwrap();
    assert_eq!(
        found.iter().map(|q| q.name.as_str()).collect::<Vec<_>>(),
        ["Good"]
    );
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(
        errors[0].contains("#2") && errors[0].contains("foldr"),
        "{errors:?}"
    );
    // The project still takes new ones.
    let q = Query {
        name: "Another".into(),
        description: String::new(),
        folder: String::new(),
        tags: Vec::new(),
        connection: None,
        sql: "SELECT 4".into(),
    };
    assert_eq!(save_query(&config, "acme", &q, None).unwrap().len(), 2);
}

#[test]
fn a_connection_that_cannot_save_is_refused_before_anything_is_written() {
    let env = Fixture::new("check-save");
    env.connection(
        "name = \"shop\"\nkind = \"mysql\"\nenv = \"prod\"\nhost = \"db\"\nuser = \"ro\"\ndatabase = \"shop\"\n",
    );
    let config = env.config();
    let before = fs::read_to_string(config.join("projects/acme.toml")).unwrap();
    let same: Connection = toml::from_str(
        "name = \"shop\"\nkind = \"postgres\"\nenv = \"prod\"\nhost = \"db2\"\nuser = \"ro\"\ndatabase = \"shop\"\n",
    )
    .unwrap();
    let e = check_save(&config, "acme", &same, None).unwrap_err();
    assert!(
        e.to_string()
            .contains("already has a connection named shop"),
        "{e:#}"
    );
    let via: Connection = toml::from_str(
        "name = \"other\"\nkind = \"postgres\"\nenv = \"prod\"\nvia = \"nowhere\"\nuser = \"ro\"\ndatabase = \"shop\"\n",
    )
    .unwrap();
    assert!(check_save(&config, "acme", &via, None).is_err());
    assert!(check_save(&config, "acme", &via, Some("gone")).is_err());
    assert_eq!(
        fs::read_to_string(config.join("projects/acme.toml")).unwrap(),
        before
    );
    // The secret another connection's password lives in, in the same environment.
    let mut shop = connections(&config, "acme").unwrap();
    shop[0]
        .secrets
        .insert("password".into(), "SHOP_PASSWORD".into());
    let mut twin = shop[0].clone();
    twin.name = "shop_2".into();
    assert_eq!(
        secret_owner(&shop, &twin, None).map(|c| c.name.as_str()),
        Some("shop")
    );
    assert!(secret_owner(&shop, &shop[0].clone(), Some("shop")).is_none());
}
