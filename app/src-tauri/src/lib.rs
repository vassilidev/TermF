mod agents;
mod api;
mod browser;
mod ci;
mod claude;
mod claude_native;
mod config;
mod context;
mod cto;
mod files;
mod keep_awake;
mod library;
mod memory;
mod onboarding;
mod quick_terminal;
mod schedule;
mod ssh;
mod sync;
mod team;
mod tickets;
mod vault;
mod visual;
mod worktree;

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody, Response};
use tauri::{AppHandle, Emitter, Manager, State};
use termf_core::ipc::SessionSummary;
use termf_core::{Project, Repo};
use termf_files::Files;
use termf_term::{Output, Sessions};

struct AppState {
    config_dir: PathBuf,
    sessions: Sessions,
    quick_terminal: quick_terminal::QuickTerminal,
    api: api::Api,
    keep_awake: keep_awake::KeepAwake,
}

type CmdResult<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Workspace {
    config_dir: String,
    home: String,
    agents: Vec<String>,
    projects: Vec<Project>,
    repos: Vec<Repo>,
    scan_truncated: bool,
    quick_terminal_error: Option<String>,
    /// Why the local API (CLI, agent status) is off.
    api_error: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ExitEvent {
    id: u32,
    exit_code: Option<u32>,
}

// async so the disk scan runs off the main thread and never freezes the window.
#[tauri::command]
async fn workspace(
    state: State<'_, AppState>,
    files: State<'_, Arc<Files>>,
) -> CmdResult<Workspace> {
    let settings = termf_core::load_settings(&state.config_dir).map_err(err)?;
    let discovery = termf_core::discover_repos(&settings.roots, settings.scan_depth);
    let projects = termf_core::load_projects(&state.config_dir).map_err(err)?;
    let repos = discovery
        .repos
        .iter()
        .chain(projects.iter().flat_map(|p| &p.repos));
    files.set_roots(repos.map(|r| &r.path));
    Ok(Workspace {
        config_dir: state.config_dir.to_string_lossy().into_owned(),
        home: termf_core::expand_tilde("~").to_string_lossy().into_owned(),
        agents: settings.agents.keys().cloned().collect(),
        projects,
        repos: discovery.repos,
        scan_truncated: discovery.truncated,
        quick_terminal_error: state.quick_terminal.error.lock().unwrap().clone(),
        api_error: state.api.error.clone(),
    })
}

/// `agent: None` opens a plain login shell.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn pty_spawn(
    app: AppHandle,
    state: State<AppState>,
    agent: Option<String>,
    cwd: String,
    title: String,
    cols: u16,
    rows: u16,
    remote_control: Option<bool>,
) -> CmdResult<SessionSummary> {
    let remote_control = remote_control.unwrap_or(false);
    let session = api::spawn(
        &state,
        agent.as_deref(),
        cwd.into(),
        title,
        cols,
        rows,
        remote_control,
        None,
        &[],
    )
    .map_err(err)?;
    state.keep_awake.update(&app);
    Ok(session)
}

#[tauri::command]
fn pty_attach(
    app: AppHandle,
    state: State<AppState>,
    id: u32,
    on_data: Channel<InvokeResponseBody>,
) -> CmdResult<Response> {
    let snapshot = state
        .sessions
        .attach(
            id,
            Box::new(move |out| match out {
                // A closed channel only means the view went away; the session keeps running.
                Output::Data(bytes) => {
                    let _ = on_data.send(InvokeResponseBody::Raw(bytes));
                }
                Output::Exit(exit_code) => {
                    let _ = app.emit("pty-exit", ExitEvent { id, exit_code });
                }
            }),
        )
        .map_err(err)?;
    Ok(Response::new(snapshot))
}

#[tauri::command]
fn pty_write(app: AppHandle, state: State<AppState>, id: u32, data: String) -> CmdResult<()> {
    state.sessions.write(id, data.as_bytes()).map_err(err)?;
    state.api.input(&app, id, data.as_bytes());
    Ok(())
}

#[tauri::command]
fn pty_resize(state: State<AppState>, id: u32, cols: u16, rows: u16) -> CmdResult<()> {
    state.sessions.resize(id, cols, rows).map_err(err)
}

#[tauri::command]
fn pty_kill(app: AppHandle, state: State<AppState>, id: u32) -> CmdResult<()> {
    state.api.forget(id);
    state.sessions.kill(id).map_err(err)?;
    state.keep_awake.update(&app);
    Ok(())
}

#[tauri::command]
fn pty_list(state: State<AppState>) -> Vec<SessionSummary> {
    // The quick terminal has its own panel, never a tile.
    let quick = *state.quick_terminal.session.lock().unwrap();
    state
        .api
        .list(&state.sessions)
        .into_iter()
        .filter(|s| Some(s.id) != quick)
        .collect()
}

/// Tauri's default macOS menu without Close Window, whose ⌘W would never reach the webview: there it closes a split.
#[cfg(target_os = "macos")]
fn app_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{
        AboutMetadata, Menu, PredefinedMenuItem as Item, Submenu, WINDOW_SUBMENU_ID,
    };
    let info = app.package_info();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        copyright: app.config().bundle.copyright.clone(),
        authors: app.config().bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };
    let separator = || Item::separator(app);
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                &info.name,
                true,
                &[
                    &Item::about(app, None, Some(about))?,
                    &separator()?,
                    &Item::services(app, None)?,
                    &separator()?,
                    &Item::hide(app, None)?,
                    &Item::hide_others(app, None)?,
                    &separator()?,
                    &Item::quit(app, None)?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &Item::undo(app, None)?,
                    &Item::redo(app, None)?,
                    &separator()?,
                    &Item::cut(app, None)?,
                    &Item::copy(app, None)?,
                    &Item::paste(app, None)?,
                    &Item::select_all(app, None)?,
                ],
            )?,
            &Submenu::with_items(app, "View", true, &[&Item::fullscreen(app, None)?])?,
            &Submenu::with_id_and_items(
                app,
                WINDOW_SUBMENU_ID,
                "Window",
                true,
                &[&Item::minimize(app, None)?, &Item::maximize(app, None)?],
            )?,
        ],
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_dir = termf_core::default_config_dir().expect("cannot locate the config directory");
    let data_dir = termf_core::default_data_dir().expect("cannot locate the data directory");
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu);
    builder
        // First, so a second launch only focuses this window, before setup touches the socket or the sessions.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(Files::default()))
        .manage(ssh::KeyFiles::default())
        .plugin(quick_terminal::plugin())
        // One setup: a second call to `setup` would replace this one.
        .setup(move |app| {
            let settings = termf_core::load_settings(&config_dir).unwrap_or_default();
            // The UI and the terminals work without the local API.
            let (api, listener) = api::Api::new();
            app.manage(ci::CiState::new(config_dir.clone(), data_dir.clone()));
            app.manage(claude::ClaudeState::new(&data_dir));
            app.manage(claude_native::NativeState::new(&data_dir));
            let (notifier, inbox) = termf_team::channel();
            app.manage(agents::state(
                app.handle(),
                config_dir.clone(),
                data_dir.clone(),
                notifier,
            ));
            app.manage(team::state(
                app.handle(),
                config_dir.clone(),
                data_dir.clone(),
                inbox,
            ));
            // After the team, whose visual gate it hooks.
            app.manage(visual::state(
                app.handle(),
                config_dir.clone(),
                data_dir.clone(),
            ));
            app.manage(tickets::TicketsState::new(
                config_dir.clone(),
                data_dir.clone(),
            ));
            app.manage(vault::start(app.handle(), config_dir.clone(), &data_dir));
            app.manage(memory::start(
                app.handle(),
                config_dir.clone(),
                data_dir.clone(),
            ));
            app.manage(AppState {
                config_dir,
                sessions: api.sessions(app.handle().clone()),
                quick_terminal: Default::default(),
                api,
                keep_awake: Default::default(),
            });
            app.state::<AppState>().keep_awake.configure(
                app.handle(),
                settings.keep_awake,
                settings.keep_display_on,
            );
            // After the state its runs use, before the local API that can ask it to run a task.
            let config = app.state::<AppState>().config_dir.clone();
            schedule::start(app.handle(), config, data_dir);
            if let Some(listener) = listener
                && let Err(e) = api::serve(app.handle().clone(), listener)
            {
                eprintln!("termf: local API disabled: {e:#}");
            }
            quick_terminal::register(app.handle());
            claude::watch(app.handle());
            tickets::watch(app.handle());
            let sync = sync::start(app.handle(), app.state::<AppState>().config_dir.clone());
            app.manage(sync);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(true) = event {
                sync::focused(window.app_handle());
            }
        })
        .invoke_handler(sync::hook(tauri::generate_handler![
            workspace,
            pty_spawn,
            pty_attach,
            pty_write,
            pty_resize,
            pty_kill,
            pty_list,
            files::files_list,
            files::files_status,
            files::files_find,
            files::files_search,
            files::files_read,
            files::files_stat,
            files::files_write,
            files::files_head,
            files::files_open_external,
            config::project_save,
            config::project_delete,
            config::layouts,
            config::layout_save,
            config::layout_delete,
            config::keys,
            config::keys_set,
            quick_terminal::quick_terminal,
            keep_awake::keep_awake_state,
            keep_awake::keep_awake_set,
            library::library_list,
            library::library_toggle,
            library::library_explain,
            library::library_import_claude,
            library::config_export,
            library::config_import_preview,
            library::config_import_apply,
            library::conventions_list,
            library::conventions_save,
            library::conventions_delete,
            library::conventions_propose,
            library::conventions_revise,
            library::conventions_review,
            library::conventions_discard,
            library::conventions_apply,
            library::conventions_switch,
            library::conventions_impact,
            library::scout_state,
            library::scout_run,
            library::scout_revise,
            library::scout_review,
            library::scout_apply,
            library::scout_discard,
            ssh::ssh_inventory,
            ssh::ssh_generate,
            ssh::ssh_agent_add,
            ssh::ssh_agent_remove,
            ssh::ssh_github_upload,
            ssh::ssh_host_preview,
            ssh::ssh_host_save,
            ssh::ssh_assign,
            ssh::ssh_test,
            ssh::ssh_fix_permissions,
            ssh::ssh_rotate,
            ssh::ssh_trash,
            ssh::keys_generate,
            ssh::keys_convert,
            ssh::keys_reveal,
            ssh::keys_test,
            ssh::keys_clipboard_clear,
            ssh::keys_pick_file,
            sync::sync_status,
            sync::sync_probe,
            sync::sync_setup,
            sync::sync_unlock,
            sync::sync_recover,
            sync::sync_push,
            sync::sync_pull,
            sync::sync_resolve,
            sync::sync_devices,
            sync::sync_change_password,
            sync::sync_scopes,
            sync::sync_disable,
            tickets::tickets_inbox,
            tickets::tickets_resolve,
            tickets::tickets_run,
            tickets::tickets_runs,
            tickets::tickets_events,
            tickets::tickets_deliver_preview,
            tickets::tickets_deliver,
            tickets::tickets_close,
            tickets::tickets_resume,
            tickets::tickets_cleanup,
            tickets::tickets_check,
            worktree::worktree_recipe,
            worktree::worktree_recipe_save,
            worktree::worktree_recipe_propose,
            worktree::worktree_recipe_discard,
            worktree::worktree_runs,
            worktree::worktree_setup,
            context::context_settings,
            context::context_settings_set,
            context::context_reset,
            tickets::tickets_sources,
            tickets::tickets_sources_save,
            tickets::tickets_source_test,
            tickets::tickets_jira_token_save,
            tickets::tickets_jira_token,
            tickets::tickets_jira_token_forget,
            tickets::tickets_mcp_servers,
            tickets::tickets_configured,
            browser::open_url,
            schedule::schedule_tasks,
            schedule::schedule_save,
            schedule::schedule_delete,
            schedule::schedule_preview,
            schedule::schedule_run,
            schedule::schedule_history,
            schedule::schedule_install,
            schedule::schedule_desktop,
            schedule::schedule_machine,
            ci::ci_runs,
            ci::ci_run,
            ci::ci_log,
            ci::ci_rerun,
            ci::ci_cancel,
            ci::ci_workflows,
            ci::ci_inputs,
            ci::ci_dispatch,
            ci::ci_checks,
            ci::ci_failure,
            ci::ci_send,
            ci::ci_poll,
            ci::ci_notify,
            ci::ci_notify_set,
            claude::claude_overview,
            claude::claude_setting_preview,
            claude::claude_setting_save,
            claude::claude_setting_reveal,
            claude::claude_settings_schema,
            claude::claude_mcp,
            claude::claude_mcp_json,
            claude::claude_mcp_json_preview,
            claude::claude_mcp_json_save,
            claude::claude_plugin,
            claude::claude_export,
            claude::artifacts_list,
            claude::artifacts_refresh,
            claude::artifacts_cancel,
            claude::artifacts_auto,
            claude::artifacts_confirm,
            claude::artifacts_delete,
            claude::artifacts_edit,
            memory::memory_status,
            memory::memory_settings_save,
            memory::memory_reindex,
            memory::expert_open,
            claude_native::claude_files,
            claude_native::claude_file_read,
            claude_native::claude_file_preview,
            claude_native::claude_file_save,
            claude_native::claude_item_create,
            claude_native::claude_file_delete,
            claude_native::claude_export_preview,
            claude_native::claude_export_item,
            claude_native::claude_sessions,
            claude_native::claude_session_open,
            claude_native::claude_purge_preview,
            claude_native::claude_purge,
            claude_native::claude_background,
            claude_native::claude_background_attach,
            claude_native::claude_background_stop,
            claude_native::claude_usage,
            claude_native::claude_auth_login,
            claude_native::claude_auth_logout,
            claude_native::claude_update,
            claude_native::claude_doctor,
            agents::agent_start,
            agents::agent_send,
            agents::agent_interrupt,
            agents::agent_answer,
            agents::agent_stop,
            agents::agent_takeover,
            agents::agent_handback,
            agents::agent_sessions,
            agents::agent_events,
            team::team_start,
            team::team_route,
            team::team_answer,
            team::team_workflows,
            team::team_approve,
            team::team_revise,
            team::team_stop,
            team::team_rules,
            team::team_runs,
            team::team_run,
            team::team_events,
            team::team_squads,
            cto::cto_open,
            cto::cto_address,
            visual::visual_start,
            visual::visual_reviews,
            visual::visual_review,
            visual::visual_decide,
            visual::visual_image,
            visual::visual_pr_comment,
            onboarding::onboarding_state,
            onboarding::onboarding_analyze,
            onboarding::onboarding_answer,
            onboarding::onboarding_step,
            onboarding::onboarding_seen,
            onboarding::onboarding_decide,
            onboarding::onboarding_team,
            onboarding::onboarding_team_preview,
            onboarding::onboarding_team_save,
            onboarding::claude_trust,
            onboarding::claude_trust_set,
            vault::vault_status,
            vault::vault_init,
            vault::vault_unlock,
            vault::vault_lock,
            vault::vault_reauth,
            vault::vault_list,
            vault::vault_set,
            vault::vault_ref,
            vault::vault_reveal,
            vault::vault_delete,
            vault::vault_audit,
            vault::vault_audit_verify,
            vault::vault_audit_export,
            vault::vault_auto_lock_set,
        ]))
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                team::shutdown(app);
                visual::shutdown(app);
                agents::shutdown(app);
            }
        });
}
