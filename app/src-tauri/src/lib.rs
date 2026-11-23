mod agentenv;
mod agents;
mod api;
mod biometry;
mod browser;
mod ci;
mod claude;
mod claude_native;
mod cli_path;
mod config;
mod context;
mod cto;
mod design;
#[cfg(feature = "e2e")]
mod e2e;
mod files;
mod git;
mod keep_awake;
mod library;
mod memory;
mod onboarding;
mod ops;
mod quick_terminal;
mod remote;
mod schedule;
mod ssh;
mod sync;
mod team;
mod tickets;
mod tools;
mod tray;
mod updates;
mod vault;
mod visual;
mod window;
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
    /// What gives a terminal's agent the vault's values for its project.
    vault: Arc<termf_vault::Vault>,
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
    // Finding Codex CLI and Gemini CLI may take seconds the first time: alongside the scan.
    let (discovery, agents) = std::thread::scope(|scope| {
        let agents = scope.spawn(|| settings.agents_found());
        let discovery = termf_core::discover_repos(&settings.roots, settings.scan_depth);
        (
            discovery,
            agents.join().unwrap_or_else(|_| settings.agents.clone()),
        )
    });
    let projects = termf_core::load_projects(&state.config_dir).map_err(err)?;
    let repos = discovery
        .repos
        .iter()
        .chain(projects.iter().flat_map(|p| &p.repos));
    files.set_roots(repos.map(|r| &r.path));
    Ok(Workspace {
        config_dir: state.config_dir.to_string_lossy().into_owned(),
        home: termf_core::expand_tilde("~").to_string_lossy().into_owned(),
        agents: agents.into_keys().collect(),
        projects,
        repos: discovery.repos,
        scan_truncated: discovery.truncated,
        quick_terminal_error: state.quick_terminal.error.lock().unwrap().clone(),
        api_error: state.api.error.clone(),
    })
}

/// `agent: None` opens a plain login shell. Async, so that reading the library of a large repository never holds
/// the window.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn pty_spawn(
    app: AppHandle,
    state: State<'_, AppState>,
    agent: Option<String>,
    cwd: String,
    title: String,
    cols: u16,
    rows: u16,
    remote_control: Option<bool>,
) -> CmdResult<SessionSummary> {
    let session = api::spawn(
        &state,
        agent.as_deref(),
        cwd.into(),
        title,
        cols,
        rows,
        api::Remote {
            on: remote_control,
            ..api::Remote::default()
        },
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
    let view = on_data.id().into();
    let snapshot = state
        .sessions
        .attach(
            id,
            view,
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

/// The terminal went off screen (ARCHITECTURE §15): its output only fills the scrollback, replayed by the next
/// `pty_attach`, and its exit is still told. `view`: the id of the channel its `pty_attach` gave, so that a detach
/// coming after a newer attach changes nothing.
#[tauri::command]
fn pty_detach(app: AppHandle, state: State<AppState>, id: u32, view: u32) -> CmdResult<()> {
    let sink = move |out| {
        if let Output::Exit(exit_code) = out {
            let _ = app.emit("pty-exit", ExitEvent { id, exit_code });
        }
    };
    state
        .sessions
        .detach(id, view.into(), Box::new(sink))
        .map(drop)
        .map_err(err)
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
    tiles(&state)
}

/// The sessions, without the quick terminal: it has its own panel, never a tile, and `termf ls` does not list it.
pub(crate) fn tiles(state: &AppState) -> Vec<SessionSummary> {
    let quick = *state.quick_terminal.session.lock().unwrap();
    state
        .api
        .list(&state.sessions)
        .into_iter()
        .filter(|s| Some(s.id) != quick)
        .collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Started from a Claude Code session, TermF would pass its markers to every terminal: a Claude Code run there
    // would be its child and save no transcript (`SESSION_MARKERS`).
    for marker in termf_core::claude::SESSION_MARKERS {
        // SAFETY: first thing in `main`, before any other thread starts.
        unsafe { std::env::remove_var(marker) };
    }
    let config_dir = termf_core::default_config_dir().expect("cannot locate the config directory");
    let data_dir = termf_core::default_data_dir().expect("cannot locate the data directory");
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(window::menu);
    // First, so a second launch only focuses this window, before setup touches the socket or the sessions. Not in
    // the e2e build: its socket is a fixed path under /tmp, so two test apps in their own sandboxes would close one.
    #[cfg(not(feature = "e2e"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }));
    // After the single instance, where there is one: a second launch never opens a second bridge.
    #[cfg(feature = "e2e")]
    let builder = match e2e::plugin() {
        Some(bridge) => builder.plugin(bridge),
        None => builder,
    };
    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Arc::new(Files::default()))
        .manage(ssh::KeyFiles::default())
        .plugin(quick_terminal::plugin())
        .on_menu_event(window::on_menu)
        // One setup: a second call to `setup` would replace this one.
        .setup(move |app| {
            window::open(app.handle(), &data_dir);
            let settings = termf_core::load_settings(&config_dir).unwrap_or_default();
            termf_core::agent_env::configure(config_dir.clone(), api::termf_binary());
            // The UI and the terminals work without the local API.
            let (api, listener) = api::Api::new(settings.api.confirm_cli);
            app.manage(tray::Tray::new(settings.tray));
            app.manage(ci::CiState::new(config_dir.clone(), data_dir.clone()));
            app.manage(claude::ClaudeState::new(&data_dir));
            app.manage(updates::Updates::new(&data_dir));
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
            app.manage(ops::state(config_dir.clone(), data_dir.clone()));
            app.manage(agentenv::state(config_dir.clone()));
            app.manage(memory::start(
                app.handle(),
                config_dir.clone(),
                data_dir.clone(),
            ));
            app.manage(AppState {
                config_dir,
                vault: vault::shared(app.handle()),
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
            let vault_dir = data_dir.join("vault");
            schedule::start(app.handle(), config, data_dir);
            if let Some(listener) = listener
                && let Err(e) = api::serve(app.handle().clone(), listener)
            {
                eprintln!("termf: local API disabled: {e:#}");
            }
            quick_terminal::register(app.handle());
            claude::watch(app.handle());
            tickets::watch(app.handle());
            let config = app.state::<AppState>().config_dir.clone();
            let sync = sync::start(app.handle(), config, vault_dir);
            app.manage(sync);
            // Looking for Codex CLI and Gemini CLI runs the user's shell: begun while the page loads, the first
            // `workspace` finds the answer kept instead of waiting for it.
            std::thread::spawn(move || drop(settings.agents_found()));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(true) = event {
                sync::focused(window.app_handle());
            }
            window::on_event(window, event);
        })
        .invoke_handler(sync::hook(tauri::generate_handler![
            workspace,
            pty_spawn,
            pty_attach,
            pty_detach,
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
            library::library_stack,
            library::library_lsp,
            library::library_toggle,
            library::library_copy,
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
            sync::sync_forget,
            sync::sync_clone,
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
            tickets::tickets_pr_merge,
            worktree::worktree_recipe,
            worktree::worktree_recipe_save,
            worktree::worktree_recipe_propose,
            worktree::worktree_recipe_discard,
            worktree::worktree_runs,
            worktree::worktree_setup,
            context::context_settings,
            context::context_settings_set,
            context::context_reset,
            api::api_confirms,
            api::api_confirm,
            remote::remote_set,
            remote::remote_cut_all,
            remote::remote_policy_save,
            tray::tray_update,
            tray::tray_settings,
            tray::tray_settings_set,
            tickets::tickets_sources,
            tickets::tickets_sources_save,
            tickets::tickets_source_test,
            tickets::tickets_jira_token_save,
            tickets::tickets_jira_token,
            tickets::tickets_jira_token_forget,
            tickets::tickets_token,
            tickets::tickets_token_save,
            tickets::tickets_token_forget,
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
            ci::ci_graph,
            ci::ci_hooks,
            ci::ci_deliveries,
            ci::ci_redeliver,
            ci::ci_hook_create,
            ci::ci_hook_delete,
            ci::deploy_tools,
            ci::deploy_targets,
            ci::deploy_target_save,
            ci::deploy_target_delete,
            ci::deploy_list,
            ci::deploy_logs,
            ci::deploy_redeploy,
            ci::ci_send,
            ci::ci_poll,
            ci::ci_notify,
            ci::ci_notify_set,
            updates::update_state,
            updates::update_check,
            updates::update_settings_set,
            updates::update_install,
            cli_path::cli_state,
            cli_path::cli_install,
            cli_path::cli_uninstall,
            tools::tools_check,
            tools::tools_chrome_install,
            git::git_state,
            git::git_graph,
            git::git_prs,
            git::git_unique,
            git::git_switch,
            git::git_fetch,
            git::git_pull,
            git::git_push,
            git::git_branch_create,
            git::git_branch_rename,
            git::git_branch_delete,
            git::git_remote_branch_delete,
            git::git_diff,
            git::git_stage,
            git::git_unstage,
            git::git_discard,
            git::git_commit,
            git::git_show,
            git::git_stash_push,
            git::git_stash_apply,
            git::git_stash_drop,
            git::git_stash_show,
            git::git_merge,
            git::git_rebase_plan,
            git::git_rebase,
            git::git_cherry_pick,
            git::git_revert,
            git::git_proceed,
            git::git_resolve,
            git::git_tag_create,
            git::git_tag_unique,
            git::git_tag_delete,
            git::git_tag_push,
            git::git_history,
            git::git_blame,
            git::git_reset_plan,
            git::git_reset,
            git::git_force_plan,
            git::git_force_push,
            claude::claude_overview,
            claude::claude_binary,
            claude::claude_setting_preview,
            claude::claude_setting_save,
            claude::claude_setting_reveal,
            claude::claude_settings_schema,
            claude::claude_mcp,
            claude::claude_mcp_json,
            claude::claude_mcp_json_preview,
            claude::claude_mcp_json_save,
            claude::claude_plugin,
            claude::claude_plugins,
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
            claude_native::claude_billing,
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
            team::team_decisions,
            team::team_revise,
            team::team_stop,
            team::team_rules,
            team::team_runs,
            team::team_run,
            team::team_events,
            team::team_squads,
            team::team_squad_save,
            team::team_squad_delete,
            team::team_update_base,
            cto::cto_open,
            cto::cto_address,
            visual::visual_start,
            visual::visual_reviews,
            visual::visual_review,
            visual::visual_decide,
            visual::visual_image,
            visual::visual_files,
            visual::visual_pr_comment,
            design::design_scan,
            design::design_pdf,
            design::design_font_files,
            design::design_web_fonts,
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
            ops::ops_overview,
            ops::ops_connection_save,
            ops::ops_connection_delete,
            ops::ops_test,
            ops::ops_plan,
            ops::ops_run,
            ops::ops_reveal,
            ops::ops_history,
            ops::ops_requests,
            ops::ops_answer,
            ops::ops_env_copy,
            ops::ops_env_write,
            ops::ops_env_compare,
            ops::ops_sql_export,
            ops::ops_follow_start,
            ops::ops_follow_stop,
            ops::ops_sql_copy,
            ops::ops_sql_export_all,
            ops::ops_parse_url,
            ops::ops_detect,
            ops::ops_schema,
            ops::ops_query_save,
            ops::ops_query_delete,
            ops::ops_query_recipe,
            ops::ops_ask,
            agentenv::agentenv_requests,
            agentenv::agentenv_answer,
            agentenv::agentenv_settings,
            agentenv::agentenv_allow_set,
            agentenv::agentenv_grants,
            agentenv::agentenv_revoke,
            onboarding::setup_repos,
            onboarding::setup_detect,
            onboarding::setup_draft,
            onboarding::setup_preview,
            onboarding::greenfield_plan,
            onboarding::greenfield_create,
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
            vault::vault_items,
            vault::vault_item_save,
            vault::vault_item_delete,
            vault::vault_item_reveal,
            vault::vault_generate,
            vault::vault_methods,
            vault::vault_unlock_biometry,
            vault::vault_reauth_key,
            vault::vault_reauth_biometry,
            vault::vault_keys,
            vault::vault_key_add,
            vault::vault_key_remove,
            vault::vault_password_change,
            vault::vault_recovery_new,
            vault::vault_recover,
            vault::vault_biometry_set,
            vault::vault_join,
            vault::vault_conflicts,
            vault::vault_resolve,
            vault::vault_export,
            vault::vault_export_csv,
            vault::vault_import_read,
            vault::vault_import_preview,
            vault::vault_import_apply,
            vault::vault_import_foreign,
            vault::vault_import_delete,
            #[cfg(feature = "e2e")]
            e2e::e2e_result,
        ]))
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            tauri::RunEvent::Exit => {
                window::save(app);
                team::shutdown(app);
                visual::shutdown(app);
                agents::shutdown(app);
                ops::shutdown(app);
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => window::reopen(app),
            _ => {}
        });
}
