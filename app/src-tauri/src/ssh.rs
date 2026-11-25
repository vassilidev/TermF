use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use termf_files::Files;
use termf_ssh::{
    Algo, Check, Generated, HostEntry, HostPreview, HostTest, Inventory, KeyType, Rotation, Spec,
    Ssh,
};

use crate::{CmdResult, api, err};

/// Key files outside ~/.ssh the key commands may read: those TermF wrote, or the user picked in a native
/// dialog, during this session (canonical paths). The webview cannot add to it.
#[derive(Default)]
pub struct KeyFiles(Mutex<HashSet<PathBuf>>);

impl KeyFiles {
    fn allowed(&self) -> HashSet<PathBuf> {
        self.0.lock().map(|set| set.clone()).unwrap_or_default()
    }

    fn remember(&self, paths: impl IntoIterator<Item = PathBuf>) {
        if let Ok(mut set) = self.0.lock() {
            set.extend(paths.into_iter().filter_map(|p| p.canonicalize().ok()));
        }
    }

    fn remember_written(&self, made: &Generated) {
        self.remember(
            made.files
                .iter()
                .filter_map(|f| f.path.as_ref().map(PathBuf::from)),
        );
    }
}

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

#[tauri::command]
pub async fn ssh_host_preview(host: HostEntry) -> CmdResult<HostPreview> {
    blocking(move |ssh| ssh.host_preview(&host)).await
}

#[tauri::command]
pub async fn ssh_host_save(host: HostEntry) -> CmdResult<()> {
    blocking(move |ssh| ssh.host_save(&host)).await
}

#[tauri::command]
pub async fn ssh_host_delete_preview(alias: String) -> CmdResult<HostPreview> {
    blocking(move |ssh| ssh.host_delete_preview(&alias)).await
}

/// Asked to the user first, in the UI, with the lines it takes out.
#[tauri::command]
pub async fn ssh_host_delete(alias: String) -> CmdResult<()> {
    blocking(move |ssh| ssh.host_delete(&alias)).await
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

/// With `path`, only that key is offered.
#[tauri::command]
pub async fn ssh_test(host: String, path: Option<String>) -> CmdResult<HostTest> {
    blocking(move |ssh| ssh.test_host(&host, path.as_deref())).await
}

#[tauri::command]
pub async fn keys_generate(
    keys: State<'_, KeyFiles>,
    algorithm: Algo,
    spec: Spec,
) -> CmdResult<Generated> {
    let made = blocking(move |ssh| ssh.create(algorithm, &spec)).await?;
    keys.remember_written(&made);
    Ok(made)
}

/// `path`: in ~/.ssh, written by TermF or picked with `keys_pick_file`, like every key command below.
#[tauri::command]
pub async fn keys_convert(
    keys: State<'_, KeyFiles>,
    path: String,
    passphrase: Option<String>,
    spec: Spec,
) -> CmdResult<Generated> {
    let allowed = keys.allowed();
    let made =
        blocking(move |ssh| ssh.convert(&path, passphrase.as_deref(), &spec, &allowed)).await?;
    keys.remember_written(&made);
    Ok(made)
}

/// The private key in clear: only on the user's explicit request.
#[tauri::command]
pub async fn keys_reveal(
    keys: State<'_, KeyFiles>,
    path: String,
    passphrase: Option<String>,
) -> CmdResult<String> {
    let allowed = keys.allowed();
    blocking(move |ssh| {
        Ok(ssh
            .reveal(&path, passphrase.as_deref(), &allowed)?
            .to_string())
    })
    .await
}

#[tauri::command]
pub async fn keys_test(
    keys: State<'_, KeyFiles>,
    path: String,
    passphrase: Option<String>,
    public: Option<String>,
) -> CmdResult<Vec<Check>> {
    let allowed = keys.allowed();
    blocking(move |ssh| ssh.test(&path, passphrase.as_deref(), public.as_deref(), &allowed)).await
}

/// A key file chosen in the native dialog, which the key commands may then read. `None`: cancelled.
#[tauri::command]
pub async fn keys_pick_file(
    app: AppHandle,
    keys: State<'_, KeyFiles>,
) -> CmdResult<Option<String>> {
    let picked =
        tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_file())
            .await
            .map_err(|e| e.to_string())?;
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    keys.remember([path.clone()]);
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Returns whether the clipboard still held the copied secret and was emptied.
#[tauri::command]
pub async fn keys_clipboard_clear(digest: String) -> CmdResult<bool> {
    blocking(move |_| termf_ssh::clipboard_clear(&digest)).await
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
