use std::sync::Arc;

use tauri::State;
use termf_files::Files;
use termf_ssh::{HostEntry, HostTest, Inventory, KeyType, Rotation, Ssh};

use crate::{CmdResult, api, err};

/// Every command runs on the blocking pool: they read ~/.ssh, run ssh, git or gh, or generate keys.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce(Ssh) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(Ssh::current()?))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[tauri::command]
pub async fn ssh_inventory(files: State<'_, Arc<Files>>) -> CmdResult<Inventory> {
    let repos = files.roots();
    blocking(move |ssh| Ok(ssh.inventory(&repos))).await
}

/// Returns the new private key's path.
#[tauri::command]
pub async fn ssh_generate(
    name: String,
    kind: KeyType,
    comment: String,
    passphrase: Option<String>,
) -> CmdResult<String> {
    blocking(move |ssh| {
        let key = ssh.generate(&name, kind, &comment, passphrase.as_deref())?;
        Ok(key.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
pub async fn ssh_agent_add(path: String, passphrase: Option<String>) -> CmdResult<()> {
    let askpass = api::termf_binary();
    blocking(move |ssh| ssh.agent_add(&path, passphrase.as_deref(), askpass.as_deref())).await
}

#[tauri::command]
pub async fn ssh_agent_remove(path: String) -> CmdResult<()> {
    blocking(move |ssh| ssh.agent_remove(&path)).await
}

#[tauri::command]
pub async fn ssh_github_upload(path: String, title: String) -> CmdResult<()> {
    blocking(move |ssh| ssh.github_upload(&path, &title)).await
}

/// The diff `ssh_host_save` would apply to ~/.ssh/config; empty when nothing changes.
#[tauri::command]
pub async fn ssh_host_preview(host: HostEntry) -> CmdResult<String> {
    blocking(move |ssh| ssh.host_preview(&host)).await
}

#[tauri::command]
pub async fn ssh_host_save(host: HostEntry) -> CmdResult<()> {
    blocking(move |ssh| ssh.host_save(&host)).await
}

/// `path: None` removes the repository's key.
#[tauri::command]
pub async fn ssh_assign(
    files: State<'_, Arc<Files>>,
    repo: String,
    path: Option<String>,
) -> CmdResult<()> {
    let known = files.roots();
    blocking(move |ssh| ssh.assign(&termf_ssh::known_repo(&repo, &known)?, path.as_deref())).await
}

#[tauri::command]
pub async fn ssh_test(host: String) -> CmdResult<HostTest> {
    blocking(move |_| termf_ssh::test_host(&host)).await
}

/// Returns the files whose permissions changed.
#[tauri::command]
pub async fn ssh_fix_permissions() -> CmdResult<Vec<String>> {
    blocking(|ssh| ssh.fix_permissions()).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn ssh_rotate(
    files: State<'_, Arc<Files>>,
    path: String,
    name: String,
    kind: KeyType,
    comment: String,
    passphrase: Option<String>,
    upload: bool,
) -> CmdResult<Rotation> {
    let repos = files.roots();
    blocking(move |ssh| {
        ssh.rotate(
            &path,
            &name,
            kind,
            &comment,
            passphrase.as_deref(),
            upload,
            &repos,
        )
    })
    .await
}

/// Returns the trash folder the key's files were moved to.
#[tauri::command]
pub async fn ssh_trash(path: String) -> CmdResult<String> {
    blocking(move |ssh| Ok(ssh.trash(&path)?.to_string_lossy().into_owned())).await
}
