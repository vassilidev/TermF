//! What an agent's process gets of TermF's environment (ARCHITECTURE §12.2): a headless session run the real way,
//! through a login shell whose profile exports secrets of its own, then the real `termf agent-env`.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use termf_agents::{Agents, Given, SessionSpec};

const PROFILE: &str = "export PROFILE_SECRET=from-the-profile ACME_FROM_PROFILE=yes\n";

fn env_of(agents: &Agents, repo: &Path, log: &Path) -> Vec<String> {
    let _ = fs::remove_file(log);
    let spec = SessionSpec {
        cwd: repo.to_path_buf(),
        ..SessionSpec::default()
    };
    let _ = agents.start(spec, Some("hello"));
    let deadline = Instant::now() + Duration::from_secs(20);
    while !fs::read_to_string(log).is_ok_and(|t| t.ends_with("--end\n")) {
        assert!(Instant::now() < deadline, "the agent never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn an_agent_starts_with_the_allowlist_and_what_termf_gives_it() {
    // TermF's own environment, as an app started from a terminal gets it: this file's only test, so no other thread
    // reads the environment meanwhile.
    unsafe {
        std::env::set_var("STRIPE_SECRET_KEY", "sk_live_from_the_app");
        std::env::set_var("ACME_TOKEN", "acme-from-the-app");
        std::env::set_var("CORP_PROXY_PASS", "corp");
    }
    let root = std::env::temp_dir().join(format!("termf-agent-env-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (repo, config, home) = (root.join("repo"), root.join("config"), root.join("home"));
    for dir in [&repo, &config.join("projects"), &home.join(".config/fish")] {
        fs::create_dir_all(dir).unwrap();
    }
    for profile in [
        ".zshenv",
        ".zprofile",
        ".zshrc",
        ".bash_profile",
        ".bashrc",
        ".profile",
    ] {
        fs::write(home.join(profile), PROFILE).unwrap();
    }
    fs::write(
        home.join(".config/fish/config.fish"),
        "set -gx PROFILE_SECRET from-the-profile\nset -gx ACME_FROM_PROFILE yes\n",
    )
    .unwrap();
    let log = root.join("env");
    let fake = root.join("claude");
    fs::write(
        &fake,
        format!(
            "#!/bin/sh\n[ \"$1\" = --version ] && {{ echo '2.1.330 (Claude Code)'; exit 0; }}\n{{ env; echo --end; }} > '{}'\n",
            log.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let settings = |extra: &str| {
        let text = format!(
            "[agents]\nclaude = \"{}\"\nenv_allow = [\"ACME_*\"]\n{extra}",
            fake.display()
        );
        fs::write(config.join("termf.toml"), text).unwrap();
    };
    settings("");
    fs::write(
        config.join("projects/acme.toml"),
        format!(
            "name = \"Acme\"\n[[repos]]\npath = \"{}\"\n\n[agents]\nenv_allow = [\"CORP_PROXY_PASS\"]\n",
            repo.display()
        ),
    )
    .unwrap();
    let home_var = ("HOME".to_owned(), home.to_string_lossy().into_owned());
    let agents = Agents::build(config.clone(), root.join("data"), vec![home_var], |_| {});
    agents.serve_guard(env!("CARGO_BIN_EXE_termf").into());
    agents.serve_secrets(|_, _| Given {
        vars: vec![("PEXELS_API_KEY".into(), "px-5f3a9c".into())],
        note: None,
    });

    let env = env_of(&agents, &repo, &log);
    let has = |line: &str| env.iter().any(|l| l == line);
    let named = |name: &str| env.iter().any(|l| l.starts_with(&format!("{name}=")));
    assert!(!named("STRIPE_SECRET_KEY"), "the app's secret: {env:?}");
    assert!(!named("PROFILE_SECRET"), "the profile's secret: {env:?}");
    assert!(
        has("ACME_TOKEN=acme-from-the-app"),
        "allowed by prefix: {env:?}"
    );
    assert!(
        has("ACME_FROM_PROFILE=yes"),
        "the profile ran first: {env:?}"
    );
    assert!(
        has("CORP_PROXY_PASS=corp"),
        "allowed by the project: {env:?}"
    );
    assert!(
        has("PEXELS_API_KEY=px-5f3a9c"),
        "given from the vault: {env:?}"
    );
    assert!(has(&format!("HOME={}", home.display())), "{env:?}");
    assert!(named("PATH") && !named("TERMF_AGENT_ENV"), "{env:?}");

    // The escape hatch: everything, as before.
    settings("env = \"inherit\"\n");
    let env = env_of(&agents, &repo, &log);
    let named = |name: &str| env.iter().any(|l| l.starts_with(&format!("{name}=")));
    assert!(
        named("STRIPE_SECRET_KEY") && named("PROFILE_SECRET"),
        "{env:?}"
    );
    let _ = fs::remove_dir_all(&root);
}
