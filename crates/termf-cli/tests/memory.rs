//! `termf memory` against a stand-in for TermF's socket, in a throwaway home.

#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use interprocess::local_socket::{GenericFilePath, ListenerOptions, prelude::*};
use serde_json::{Value, json};

struct Home {
    root: PathBuf,
    repo: PathBuf,
    socket: String,
    requests: Arc<Mutex<Vec<Value>>>,
}

/// Where `dirs` puts TermF's data and config for this `HOME` (and these XDG folders on Linux).
fn dirs(root: &Path) -> (PathBuf, PathBuf) {
    if cfg!(target_os = "macos") {
        let support = root.join("home/Library/Application Support");
        (support.join("dev.termf.app"), support.join("termf"))
    } else {
        (root.join("data/dev.termf.app"), root.join("config/termf"))
    }
}

fn home(name: &str) -> Home {
    let root = std::env::temp_dir().join(format!("termf-cli-memory-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (data, config) = dirs(&root);
    let repo = root.join("api");
    fs::create_dir_all(data.join("run")).unwrap();
    fs::create_dir_all(config.join("projects")).unwrap();
    fs::create_dir_all(&repo).unwrap();
    fs::write(data.join("run/token"), "tok").unwrap();
    fs::write(config.join("termf.toml"), "").unwrap();
    fs::write(
        config.join("projects/acme.toml"),
        format!(
            "name = \"Acme\"\n[[repos]]\npath = {:?}\n",
            repo.to_string_lossy()
        ),
    )
    .unwrap();
    let socket = root.join("s.sock").to_string_lossy().into_owned();
    let listener = ListenerOptions::new()
        .name(socket.as_str().to_fs_name::<GenericFilePath>().unwrap())
        .create_sync()
        .unwrap();
    let requests: Arc<Mutex<Vec<Value>>> = Arc::default();
    let seen = requests.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming().filter_map(Result::ok) {
            let mut conn = BufReader::new(conn);
            let mut line = String::new();
            if conn.read_line(&mut line).unwrap_or(0) > 0 {
                seen.lock()
                    .unwrap()
                    .push(serde_json::from_str(&line).unwrap());
                let _ = conn.get_mut().write_all(b"{\"ok\": []}\n");
            }
        }
    });
    Home {
        root,
        repo,
        socket,
        requests,
    }
}

fn termf(home: &Home, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_termf"))
        .args(args)
        .current_dir(&home.repo)
        .env("HOME", home.root.join("home"))
        .env("XDG_DATA_HOME", home.root.join("data"))
        .env("XDG_CONFIG_HOME", home.root.join("config"))
        .env("TERMF_SOCKET", &home.socket)
        .output()
        .unwrap()
}

#[test]
fn reindexes_every_project_unless_one_is_named_and_searches_this_folder_s() {
    let home = home("reindex");
    let out = termf(&home, &["memory", "reindex"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = termf(&home, &["memory", "reindex", "--project", "acme"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let params: Vec<Value> = home
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| r["params"].clone())
        .collect();
    assert_eq!(
        params,
        [
            json!({"action": "reindex", "project": null}),
            json!({"action": "reindex", "project": "acme"})
        ],
        "the folder's project is not taken for a reindex"
    );

    // A search needs no TermF, and takes the project of the folder.
    let out = termf(&home, &["memory", "search", "webhooks"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "nothing found\n");
    assert_eq!(home.requests.lock().unwrap().len(), 2);
    let _ = fs::remove_dir_all(&home.root);
}
