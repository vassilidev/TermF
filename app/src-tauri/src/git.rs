//! Git in the project for the UI: every command runs off the UI thread, since git may read a large repository or
//! reach the network.

use std::path::Path;

use tauri::AppHandle;
use termf_git::{
    Blame, CommitDetail, FileDiff, FileHistory, ForcePlan, Git, Graph, Outcome, Pick, Pr, Pull,
    Push, RebasePlan, ResetMode, ResetPlan, Resolution, State, Step, Unique,
};

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

#[tauri::command]
pub async fn git_diff(
    app: AppHandle,
    repo: String,
    path: String,
    from: Option<String>,
    staged: bool,
) -> CmdResult<FileDiff> {
    with_git(app, repo, move |git, repo| {
        git.diff(repo, &path, from.as_deref(), staged)
    })
    .await
}

#[tauri::command]
pub async fn git_stage(
    app: AppHandle,
    repo: String,
    path: String,
    picks: Option<Vec<Pick>>,
    id: Option<String>,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| {
        git.stage(repo, &path, picks.as_deref(), id.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn git_unstage(
    app: AppHandle,
    repo: String,
    path: String,
    from: Option<String>,
    picks: Option<Vec<Pick>>,
    id: Option<String>,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| {
        git.unstage(
            repo,
            &path,
            from.as_deref(),
            picks.as_deref(),
            id.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn git_discard(
    app: AppHandle,
    repo: String,
    path: String,
    picks: Option<Vec<Pick>>,
    id: String,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| {
        git.discard(repo, &path, picks.as_deref(), &id)
    })
    .await
}

#[tauri::command]
pub async fn git_commit(
    app: AppHandle,
    repo: String,
    message: String,
    amend: bool,
    head: Option<String>,
) -> CmdResult<String> {
    with_git(app, repo, move |git, repo| {
        git.commit(repo, &message, amend, head.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn git_show(app: AppHandle, repo: String, rev: String) -> CmdResult<CommitDetail> {
    with_git(app, repo, move |git, repo| git.show(repo, &rev)).await
}

#[tauri::command]
pub async fn git_stash_push(
    app: AppHandle,
    repo: String,
    message: String,
    untracked: bool,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| {
        git.stash_push(repo, &message, untracked)
    })
    .await
}

#[tauri::command]
pub async fn git_stash_apply(
    app: AppHandle,
    repo: String,
    index: usize,
    oid: String,
    pop: bool,
) -> CmdResult<Outcome> {
    with_git(app, repo, move |git, repo| {
        git.stash_apply(repo, index, &oid, pop)
    })
    .await
}

#[tauri::command]
pub async fn git_stash_drop(
    app: AppHandle,
    repo: String,
    index: usize,
    oid: String,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| {
        git.stash_drop(repo, index, &oid)
    })
    .await
}

#[tauri::command]
pub async fn git_stash_show(
    app: AppHandle,
    repo: String,
    index: usize,
    oid: String,
) -> CmdResult<Vec<FileDiff>> {
    with_git(app, repo, move |git, repo| {
        git.stash_show(repo, index, &oid)
    })
    .await
}

#[tauri::command]
pub async fn git_merge(app: AppHandle, repo: String, branch: String) -> CmdResult<Outcome> {
    with_git(app, repo, move |git, repo| git.merge(repo, &branch)).await
}

#[tauri::command]
pub async fn git_rebase_plan(app: AppHandle, repo: String, onto: String) -> CmdResult<RebasePlan> {
    with_git(app, repo, move |git, repo| git.rebase_plan(repo, &onto)).await
}

#[tauri::command]
pub async fn git_rebase(
    app: AppHandle,
    repo: String,
    onto: String,
    head: String,
) -> CmdResult<Outcome> {
    with_git(app, repo, move |git, repo| git.rebase(repo, &onto, &head)).await
}

#[tauri::command]
pub async fn git_cherry_pick(app: AppHandle, repo: String, oid: String) -> CmdResult<Outcome> {
    with_git(app, repo, move |git, repo| git.cherry_pick(repo, &oid)).await
}

#[tauri::command]
pub async fn git_revert(app: AppHandle, repo: String, oid: String) -> CmdResult<Outcome> {
    with_git(app, repo, move |git, repo| git.revert(repo, &oid)).await
}

#[tauri::command]
pub async fn git_proceed(app: AppHandle, repo: String, step: Step) -> CmdResult<Outcome> {
    with_git(app, repo, move |git, repo| git.proceed(repo, step)).await
}

#[tauri::command]
pub async fn git_resolve(
    app: AppHandle,
    repo: String,
    path: String,
    how: Resolution,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| git.resolve(repo, &path, how)).await
}

#[tauri::command]
pub async fn git_tag_create(
    app: AppHandle,
    repo: String,
    name: String,
    target: String,
    message: Option<String>,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| {
        git.tag_create(repo, &name, &target, message.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn git_tag_unique(app: AppHandle, repo: String, name: String) -> CmdResult<Unique> {
    with_git(app, repo, move |git, repo| git.tag_unique(repo, &name)).await
}

#[tauri::command]
pub async fn git_tag_delete(
    app: AppHandle,
    repo: String,
    name: String,
    oid: String,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| {
        git.tag_delete(repo, &name, &oid)
    })
    .await
}

#[tauri::command]
pub async fn git_tag_push(app: AppHandle, repo: String, name: String) -> CmdResult<String> {
    with_git(app, repo, move |git, repo| git.tag_push(repo, &name)).await
}

#[tauri::command]
pub async fn git_history(
    app: AppHandle,
    repo: String,
    path: String,
    limit: usize,
) -> CmdResult<FileHistory> {
    with_git(app, repo, move |git, repo| git.history(repo, &path, limit)).await
}

#[tauri::command]
pub async fn git_blame(app: AppHandle, repo: String, path: String) -> CmdResult<Blame> {
    with_git(app, repo, move |git, repo| git.blame(repo, &path)).await
}

#[tauri::command]
pub async fn git_reset_plan(app: AppHandle, repo: String, target: String) -> CmdResult<ResetPlan> {
    with_git(app, repo, move |git, repo| git.reset_plan(repo, &target)).await
}

#[tauri::command]
pub async fn git_reset(
    app: AppHandle,
    repo: String,
    target: String,
    mode: ResetMode,
    head: String,
) -> CmdResult<()> {
    with_git(app, repo, move |git, repo| {
        git.reset(repo, &target, mode, &head)
    })
    .await
}

#[tauri::command]
pub async fn git_force_plan(app: AppHandle, repo: String, branch: String) -> CmdResult<ForcePlan> {
    with_git(app, repo, move |git, repo| git.force_plan(repo, &branch)).await
}

#[tauri::command]
pub async fn git_force_push(
    app: AppHandle,
    repo: String,
    branch: String,
    theirs: String,
) -> CmdResult<String> {
    with_git(app, repo, move |git, repo| {
        git.force_push(repo, &branch, &theirs)
    })
    .await
}
