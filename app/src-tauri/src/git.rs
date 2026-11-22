//! Git in the project for the UI: every command runs off the UI thread, since git may read a large repository or
//! reach the network.

use std::path::Path;

use tauri::AppHandle;
use termf_git::{Git, Graph, Pr, Pull, Push, State, Unique};

use crate::CmdResult;
use crate::tickets::blocking;

async fn with_git<T: Send + 'static>(
    app: AppHandle,
    repo: String,
    f: impl FnOnce(&Git, &Path) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    blocking(app, move |_| f(&Git::default(), Path::new(&repo))).await
}

#[tauri::command]
pub async fn git_state(app: AppHandle, repo: String) -> CmdResult<State> {
    with_git(app, repo, |git, repo| git.state(repo)).await
}

#[tauri::command]
pub async fn git_graph(app: AppHandle, repo: String, limit: usize) -> CmdResult<Graph> {
    with_git(app, repo, move |git, repo| git.graph(repo, limit)).await
}

#[tauri::command]
pub async fn git_prs(app: AppHandle, repo: String) -> CmdResult<Vec<Pr>> {
    with_git(app, repo, |git, repo| Ok(git.prs(repo))).await
}

#[tauri::command]
pub async fn git_unique(app: AppHandle, repo: String, branch: String) -> CmdResult<Unique> {
    with_git(app, repo, move |git, repo| git.unique(repo, &branch)).await
}

#[tauri::command]
pub async fn git_switch(app: AppHandle, repo: String, branch: String) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| git.switch(repo, &branch)).await
}

#[tauri::command]
pub async fn git_fetch(app: AppHandle, repo: String) -> CmdResult<()> {
    with_git(app, repo, |git, repo| git.fetch(repo)).await
}

#[tauri::command]
pub async fn git_pull(app: AppHandle, repo: String) -> CmdResult<Pull> {
    with_git(app, repo, |git, repo| git.pull(repo)).await
}

#[tauri::command]
pub async fn git_push(app: AppHandle, repo: String, branch: String) -> CmdResult<Push> {
    with_git(app, repo, move |git, repo| git.push(repo, &branch)).await
}

#[tauri::command]
pub async fn git_branch_create(
    app: AppHandle,
    repo: String,
    name: String,
    from: String,
    checkout: bool,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| {
        git.create(repo, &name, &from, checkout)
    })
    .await
}

#[tauri::command]
pub async fn git_branch_rename(
    app: AppHandle,
    repo: String,
    from: String,
    to: String,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| git.rename(repo, &from, &to)).await
}

#[tauri::command]
pub async fn git_branch_delete(
    app: AppHandle,
    repo: String,
    name: String,
    tip: String,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| git.delete(repo, &name, &tip)).await
}

#[tauri::command]
pub async fn git_remote_branch_delete(
    app: AppHandle,
    repo: String,
    remote: String,
    branch: String,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| {
        git.delete_remote(repo, &remote, &branch)
    })
    .await
}
