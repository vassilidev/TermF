//! The `termf` CLI in your terminal finds the user's token where TermF now writes it, outside the data folder
//! the sessions are given, and never takes the one TermF used to leave next to the socket.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use interprocess::local_socket::{GenericFilePath, ListenerOptions, prelude::*};
use serde_json::Value;
use termf_core::ipc;

/// A throwaway home: `dirs` reads `$HOME` on macOS and `$XDG_DATA_HOME` on Linux.
fn home(name: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("termf-token-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let data = if cfg!(target_os = "macos") {
        root.join("Library/Application Support")
    } else {
        root.join("share")
    };
    (root, data)
}

fn termf(root: &Path, socket: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_termf"))
        .args(args)
        .env("HOME", root)
        .env("XDG_DATA_HOME", root.join("share"))
        .env(ipc::ENV_SOCKET, socket)
        .env_remove(ipc::ENV_TOKEN)
        .env_remove(ipc::ENV_SESSION)
        .output()
        .unwrap()
}

#[test]
fn your_cli_still_reaches_termf_with_the_new_token() {
    let (root, data) = home("cli");
    let socket = root.join("termf.sock").to_string_lossy().into_owned();
    let listener = ListenerOptions::new()
        .name(socket.as_str().to_fs_name::<GenericFilePath>().unwrap())
        .create_sync()
        .unwrap();
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let requests = seen.clone();
    thread::spawn(move || {
        for conn in listener.incoming().filter_map(Result::ok) {
            let mut conn = BufReader::new(conn);
            let mut line = String::new();
            if conn.read_line(&mut line).unwrap_or(0) > 0 {
                requests
                    .lock()
                    .unwrap()
                    .push(serde_json::from_str(&line).unwrap());
                let _ = conn.get_mut().write_all(b"{\"ok\":[]}\n");
            }
        }
    });

    let missing = termf(&root, &socket, &["ls"]);
    assert!(!missing.status.success());
    let said = String::from_utf8_lossy(&missing.stderr);
    assert!(
        said.contains(ipc::USER_TOKEN_DIR) && said.contains("is TermF running?"),
        "{said}"
    );

    // What TermF wrote before: next to the socket, in the folder every session knows.
    let old = data.join("dev.termf.app/run/token");
    std::fs::create_dir_all(old.parent().unwrap()).unwrap();
    std::fs::write(&old, "old-token").unwrap();
    ipc::write_token(&data.join(ipc::USER_TOKEN_DIR).join("token"), "user-token").unwrap();
    let listed = termf(&root, &socket, &["ls"]);
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["token"], "user-token");
    assert_eq!(seen[0]["method"], "list");
    let _ = std::fs::remove_dir_all(&root);
}
