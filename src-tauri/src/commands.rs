//! Tauri commands invoked by the frontend.

use crate::accounts::{AccountStatus, ShareMode};
use crate::attachfiles::AttachedFile;
use crate::closed::ClosedProjectInfo;
use crate::core::{Attachment, Core, NotOnBase, SyncOp};
use crate::fsedit;
use crate::i18n::{self, Lang};
use crate::integrations::{
    Account, AccountView, Container, ExternalIssue, IssuePage, Query, StatesView,
};
use crate::model::Account as ClaudeAccount;
use crate::model::*;
use crate::pty::{self, ShellInfo, TermInfo, TermReport};
use crate::search;
use crate::stats::StatsView;
use crate::telemetry::{self, Event};
use crate::tickets::TicketDraft;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;

type Res<T> = Result<T, String>;

/// A command's result, counted in the usage statistics when it went well (`telemetry`).
fn counted<T, R: tauri::Runtime>(core: &Core<R>, event: Event, r: Res<T>) -> Res<T> {
    if r.is_ok() {
        core.telemetry.track(event);
    }
    r
}

/// The project a folder was added as, counted as created unless a closed project's history came
/// back with it (`Core::add_project`): that one is not new.
fn project_added<R: tauri::Runtime>(core: &Core<R>, r: Res<(Project, bool)>) -> Res<Project> {
    match r {
        Ok((project, false)) => counted(core, Event::ProjectCreated, Ok(project)),
        other => other.map(|(project, _)| project),
    }
}
type CoreState<'a> = State<'a, Arc<Core>>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// Refused: the shell `id` asked for is not one this system has.
fn shell_not_found(lang: Lang, id: &str) -> String {
    tr_in!(lang, "shell « {id} » introuvable", "shell “{id}” not found")
}

fn no_shell() -> String {
    tr!("aucun shell détecté", "no shell found")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialState {
    projects: Vec<Project>,
    tickets: Vec<Ticket>,
    agents: Vec<AgentView>,
    ui: UiState,
    settings: Settings,
    usage: UsageSnapshot,
    git: HashMap<String, GitInfo>,
    shells: Vec<ShellInfo>,
    terminals: Vec<TermInfo>,
    /// Claude Code is found for the account new agents go to.
    claude_found: bool,
    /// The settings' own path to Claude Code leads to it (what the field's hint says).
    claude_path_found: bool,
    version: String,
    models: Vec<ModelInfo>,
    /// Why no ticket of a project's board starts, by project.
    board_issues: HashMap<String, String>,
    /// Why no ticket of a project that goes to any account starts for now (a quota, a usage
    /// limit), if none does.
    autopilot_pause: Option<AutopilotPause>,
    /// The same for the projects that prefer an account, by project.
    project_pauses: BTreeMap<String, AutopilotPause>,
    /// The external ticket systems' accounts, as the window knows them.
    accounts: Vec<AccountView>,
    /// The update installed since the app's last start, told once (« Voir les nouveautés »).
    installed: Option<crate::updates::Installed>,
    /// The automatic restart planned for an update: when it comes.
    restart_at: Option<i64>,
    /// A version that did not install at the last try (the window offers to try again).
    failed_update: Option<String>,
    /// What the step running of each worktree setup under way wrote, its last lines, by agent.
    setup_output: HashMap<String, SetupOutput>,
    /// The languages of the interface, of the system and of the texts Claude writes.
    lang: crate::i18n::LangInfo,
}

// Off the main thread: finding a `claude` of WSL may start its distribution (seconds).
#[tauri::command(async)]
pub fn subscribe(
    core: CoreState,
    updates: State<crate::updates::Updates>,
    channel: Channel<UiEvent>,
) -> InitialState {
    core.hub.set_channel(channel);
    core.reset_unsaved();
    // One lock at a time, each guard ending with its statement: in a struct literal the guards
    // would live together (the lock order is projects before tickets, see `Core`).
    let settings = core.settings.read().clone();
    let projects = core.projects.read().clone();
    let tickets = core.tickets.read().clone();
    let agents = core.agent_views();
    let setup_output = core.setup_outputs();
    let ui = core.ui.read().clone();
    let usage = core.usage.lock().clone();
    let git = core.git_cache.read().clone();
    let terminals = core.pty.list();
    let models = core.models.read().clone();
    let board_issues = core.board_issues.lock().clone();
    // As it is now: the timer tells this window when it ends (and the board goes on then).
    let autopilot_pause = core.autopilot_pause();
    let project_pauses = core.project_pauses();
    let accounts = core.integration_accounts();
    let version = core.app.package_info().version.to_string();
    let lang = core.lang();
    let claude_path_found =
        crate::claude::resolve_binary(&settings.claude_path, &settings.wsl_distro).is_some();
    InitialState {
        lang,
        projects,
        tickets,
        agents,
        ui,
        shells: pty::detect_shells(&settings),
        claude_found: core.claude_found(),
        claude_path_found,
        settings,
        usage,
        git,
        terminals,
        installed: updates.take_installed(),
        restart_at: updates.restart_at(),
        failed_update: updates.failed(),
        version,
        models,
        board_issues,
        autopilot_pause,
        project_pauses,
        accounts,
        setup_output,
    }
}

#[tauri::command]
pub fn set_ui(core: CoreState, ui: UiState) {
    *core.ui.write() = ui;
    core.request_save();
}

/// The settings as the window saves them; the Claude accounts are left as they are (their tab
/// changes them with the `account_*` commands).
#[tauri::command(async)]
pub fn save_settings(core: CoreState, settings: Settings) -> Res<Vec<ShellInfo>> {
    let remote_was_on = core.settings.read().remote_control_all;
    core.save_window_settings(settings.clone()).map_err(err)?;
    if settings.remote_control_all && !remote_was_on {
        core.telemetry
            .track(Event::RemoteControlEnabled { from: "global" });
    }
    Ok(pty::detect_shells(&settings))
}

/// Where the `claude` of a path of the settings is (empty: the one found by itself), the
/// distribution of the settings (`wsl_distro`) for one of WSL: the line under the field.
#[tauri::command(async)]
pub fn claude_locate(path: String, wsl_distro: String) -> crate::claude::Located {
    // Asked by the user: what was not found is looked for again (maybe just installed).
    crate::wsl::forget(false);
    crate::claude::locate(&path, &wsl_distro)
}

/// Whether Claude Code is found, as the start tells it (`claude_found`, `claude_path_found` of
/// `InitialState`): asked again once its path is saved.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeFound {
    claude_found: bool,
    claude_path_found: bool,
}

#[tauri::command(async)]
pub fn claude_found(core: CoreState) -> ClaudeFound {
    // Asked once a path is saved: every `claude` of WSL is looked for again (installed, removed).
    crate::wsl::forget(true);
    let settings = core.settings.read().clone();
    ClaudeFound {
        claude_found: core.claude_found(),
        claude_path_found: crate::claude::resolve_binary(
            &settings.claude_path,
            &settings.wsl_distro,
        )
        .is_some(),
    }
}

// ---------- Claude accounts ----------

/// The items of Principal's folder a new account may share (those it has).
#[tauri::command(async)]
pub fn account_shareable(core: CoreState) -> Vec<String> {
    core.claude_shareable()
}

/// « Créer et se connecter »: a new account, its folder made and the items `share` of Principal's
/// linked or copied into it (`mode`), saved last in the settings.
#[tauri::command(async)]
pub fn account_create(
    core: CoreState,
    name: String,
    share: Vec<String>,
    mode: ShareMode,
) -> Res<ClaudeAccount> {
    let r = core.create_claude_account(&name, &share, mode).map_err(err);
    counted(&core, Event::AccountCreated, r)
}

/// The account's name, `claude` and « Actif »; the accounts as they then are.
#[tauri::command(async)]
pub fn account_update(core: CoreState, account: ClaudeAccount) -> Res<Vec<ClaudeAccount>> {
    core.update_claude_account(account).map_err(err)
}

/// The accounts in the order of `ids`, as they then are.
#[tauri::command(async)]
pub fn account_reorder(core: CoreState, ids: Vec<String>) -> Res<Vec<ClaudeAccount>> {
    core.reorder_claude_accounts(&ids).map_err(err)
}

/// The account removed (its folder stays on the disk); the accounts as they then are.
#[tauri::command(async)]
pub fn account_remove(core: CoreState, id: String) -> Res<Vec<ClaudeAccount>> {
    core.remove_claude_account(&id).map_err(err)
}

/// Whether the account is signed in, with which email, and its folder.
#[tauri::command]
pub async fn account_status(core: CoreState<'_>, id: String) -> Res<AccountStatus> {
    core.claude_account_status(&id).await.map_err(err)
}

/// « Se connecter… »: the account's `claude` in an interactive terminal (written to, resized and
/// killed as the others: `term_write`, `term_resize`, `term_kill`).
#[tauri::command(async)]
pub fn account_login(
    core: CoreState,
    id: String,
    cols: u16,
    rows: u16,
    output: Channel<InvokeResponseBody>,
) -> Res<TermInfo> {
    core.claude_login(&id, (cols, rows), move |bytes| {
        let _ = output.send(InvokeResponseBody::Raw(bytes));
    })
    .map_err(err)
}

/// « Vue Claude Code »: the agent runs in Claude Code's own interface, or in Escouade's view.
#[tauri::command]
pub async fn set_agent_view(core: CoreState<'_>, id: String, terminal: bool) -> Res<()> {
    core.set_agent_view(&id, terminal).await.map_err(err)?;
    if terminal {
        core.telemetry.track(Event::ClaudeCodeView);
    }
    Ok(())
}

/// The agent's Claude Code interface, in an interactive terminal (written to, resized and killed
/// as the others: `term_write`, `term_resize`, `term_kill`). `manual`: the user asked for it
/// (« Relancer »), as opposed to the window opening the view: an automatic start is refused once
/// the interface stopped as soon as it started twice in a row.
#[tauri::command]
pub async fn agent_terminal(
    core: CoreState<'_>,
    id: String,
    cols: u16,
    rows: u16,
    manual: Option<bool>,
    output: Channel<InvokeResponseBody>,
) -> Res<TermInfo> {
    core.open_agent_terminal(&id, (cols, rows), manual.unwrap_or(false), move |bytes| {
        let _ = output.send(InvokeResponseBody::Raw(bytes));
    })
    .await
    .map_err(err)
}

/// An image pasted in the agent's Claude Code interface, written to a file: the path to type there.
#[tauri::command(async)]
pub fn paste_image(core: CoreState, id: String, media_type: String, data: String) -> Res<String> {
    core.paste_image(&id, &media_type, &data).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderInfo {
    exists: bool,
    is_repo: bool,
    branch: String,
    dirty: u32,
    name: String,
    /// The project closed in this folder, whose history adding a project there gives back.
    history: Option<ClosedProjectInfo>,
}

/// Always `Ok`: an async command that borrows the state must return a `Result`. `history_id`: the
/// closed project of this folder the window means to bring back, if it has several.
#[tauri::command]
pub async fn inspect_folder(
    core: CoreState<'_>,
    path: String,
    history_id: Option<String>,
) -> Res<FolderInfo> {
    let p = std::path::Path::new(path.trim());
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if !p.is_dir() {
        return Ok(FolderInfo {
            exists: false,
            is_repo: false,
            branch: String::new(),
            dirty: 0,
            name,
            history: None,
        });
    }
    let path = path.trim().to_string();
    let history = core
        .closed_project_info_for(&path, history_id.as_deref())
        .await;
    Ok(match crate::git::toplevel(&path).await {
        Some(_) => {
            let st = crate::git::status(&path).await.unwrap_or_default();
            FolderInfo {
                exists: true,
                is_repo: true,
                branch: st.branch,
                dirty: st.entries.len() as u32,
                name,
                history,
            }
        }
        None => FolderInfo {
            exists: true,
            is_repo: false,
            branch: String::new(),
            dirty: 0,
            name,
            history,
        },
    })
}

/// `restore`: the history of a project closed in this folder comes back with it (the default);
/// false starts a project of its own, the history staying among the closed projects.
/// `history_id`: which of the folder's histories comes back, when it has several (else the latest
/// closed).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_project(
    core: CoreState<'_>,
    path: String,
    name: String,
    color: String,
    worktree_per_agent: bool,
    first_agent: Option<String>,
    restore: Option<bool>,
    history_id: Option<String>,
) -> Res<Project> {
    let r = core
        .add_project(
            &path,
            &name,
            &color,
            worktree_per_agent,
            first_agent,
            restore.unwrap_or(true),
            history_id,
        )
        .await
        .map_err(err);
    project_added(&core, r)
}

#[tauri::command(async)]
pub fn update_project(core: CoreState, project: Project) -> Res<()> {
    let remote_was_on = core.project(&project.id).is_ok_and(|p| p.remote_control);
    let remote_on = project.remote_control;
    core.update_project(project).map_err(err)?;
    if remote_on && !remote_was_on {
        core.telemetry
            .track(Event::RemoteControlEnabled { from: "project" });
    }
    Ok(())
}

#[tauri::command(async)]
pub fn reorder_projects(core: CoreState, ids: Vec<String>) {
    core.reorder_projects(&ids);
}

/// `erase`: its history goes too; by default it is kept (`closed_projects`).
#[tauri::command]
pub async fn remove_project(core: CoreState<'_>, id: String, erase: Option<bool>) -> Res<()> {
    core.remove_project(&id, erase.unwrap_or(false))
        .await
        .map_err(err)
}

/// The projects closed with their history kept, the latest closed first.
#[tauri::command(async)]
pub fn closed_projects(core: CoreState) -> Vec<ClosedProjectInfo> {
    core.closed_projects()
}

/// « Effacer l'historique » of a closed project (the file of an unreadable one alone), away from
/// the async workers: it reads the history and removes its logs.
#[tauri::command]
pub async fn erase_closed_project(core: CoreState<'_>, id: String) -> Res<()> {
    let core = core.inner().clone();
    tokio::task::spawn_blocking(move || core.erase_closed_project(&id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[tauri::command]
pub async fn create_agent(
    core: CoreState<'_>,
    project_id: String,
    model: Option<String>,
) -> Res<AgentView> {
    let agent = core.create_agent(&project_id, model).await.map_err(err)?;
    core.telemetry.track(Event::AgentCreated {
        worktree: agent.meta.worktree.is_some(),
    });
    Ok(agent)
}

#[tauri::command]
pub fn warm_agent(core: CoreState, id: String) {
    core.warm(&id);
}

/// « Relancer » (« Outils d'Escouade au prochain démarrage »): the agent's process, at rest,
/// started again on the same session.
#[tauri::command]
pub async fn restart_agent_process(core: CoreState<'_>, id: String) -> Res<()> {
    core.restart_process(&id).await.map_err(err)
}

#[tauri::command(async)]
pub fn get_conversation(core: CoreState, id: String) -> Res<Vec<Value>> {
    // Buffered deltas are already in the snapshot: send them before it, not after.
    core.flush_conv();
    let h = core.agent(&id).map_err(err)?;
    let items = h.lock().conv.items();
    Ok(items)
}

/// Searches the conversations (Ctrl+K) away from the window's threads: it reads every log.
#[tauri::command]
pub async fn search_conversations(
    core: CoreState<'_>,
    query: String,
    project_id: Option<String>,
    archived: bool,
) -> Res<crate::convsearch::Found> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        core.search_conversations(&query, project_id.as_deref(), archived)
    })
    .await
    .map_err(|e| e.to_string())
}

/// A message to the agent. `files`: the files joined by their path (`attachfiles`); a window that
/// does not send any (an older one) sends none.
#[tauri::command]
pub async fn send_message(
    core: CoreState<'_>,
    id: String,
    text: String,
    attachments: Vec<Attachment>,
    files: Option<Vec<AttachedFile>>,
) -> Res<()> {
    let r = core
        .send_message_files(&id, text, attachments, files.unwrap_or_default())
        .await
        .map_err(err);
    counted(&core, Event::MessageSent, r)
}

/// A file to copy for `owner` (an agent's id, or `"draft"`): its handle, for its chunks.
#[tauri::command(async)]
pub fn attach_open(core: CoreState, owner: String, name: String, size: u64) -> Res<String> {
    core.attach_open(&owner, &name, size).map_err(err)
}

/// The next bytes of a copy, as they are (`invoke('attach_chunk', bytes, { headers: { 'x-handle':
/// handle } })`: no base64 round trip through JSON).
#[tauri::command(async)]
pub fn attach_chunk(core: CoreState, request: tauri::ipc::Request) -> Res<()> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(tr!(
            "Les octets d’une copie viennent bruts, pas en JSON.",
            "The bytes of a copy come raw, not as JSON."
        ));
    };
    let handle = request
        .headers()
        .get("x-handle")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    core.attach_chunk(handle, bytes).map_err(err)
}

/// The end of a copy: the file, once all its bytes are there.
#[tauri::command(async)]
pub fn attach_finish(core: CoreState, handle: String) -> Res<AttachedFile> {
    core.attach_finish(&handle).map_err(err)
}

/// A copy given up: what was written goes.
#[tauri::command(async)]
pub fn attach_cancel(core: CoreState, handle: String) -> Res<()> {
    core.attach_cancel(&handle);
    Ok(())
}

/// A finished copy removed (its chip removed); never a path out of its owner's folder.
#[tauri::command(async)]
pub fn attach_discard(core: CoreState, owner: String, path: String) -> Res<()> {
    core.attach_discard(&owner, &path).map_err(err)
}

/// A file of the user's own, joined as it is: checked, nothing copied. `id` is the one the window
/// asked `file_origin` the path of, never a path: only a path the host itself read is joined.
#[tauri::command(async)]
pub fn attach_origin(core: CoreState, owner: String, id: String) -> Res<AttachedFile> {
    core.attach_origin(&owner, &id).map_err(err)
}

/// The image on the system's clipboard, as PNG (empty without one), for a paste WebKitGTK leaves
/// without it.
#[tauri::command]
pub async fn clipboard_image(app: tauri::AppHandle) -> Res<tauri::ipc::Response> {
    crate::clipboard::image_png(&app)
        .await
        .map(tauri::ipc::Response::new)
        .map_err(err)
}

#[tauri::command]
pub async fn interrupt(core: CoreState<'_>, id: String) -> Res<()> {
    core.interrupt(&id).await.map_err(err)
}

/// A message to a subagent of the agent `id`, by the agent (`call`: the call that launched it).
#[tauri::command]
pub async fn message_subagent(
    core: CoreState<'_>,
    id: String,
    call: String,
    text: String,
) -> Res<()> {
    core.message_subagent(&id, &call, &text).await.map_err(err)
}

/// « Arrêter » on a subagent of the agent `id`, `call` the call that launched it.
#[tauri::command]
pub async fn stop_subagent(core: CoreState<'_>, id: String, call: String) -> Res<()> {
    core.stop_subagent(&id, &call).await.map_err(err)
}

/// « Arrêter » on a task Claude Code runs in the background for the agent `id` (a command, a
/// monitor, a remote agent, a teammate: a `BgTask` of its plan; or a workflow): `stop_task`.
#[tauri::command]
pub async fn stop_bg_task(core: CoreState<'_>, id: String, task: String) -> Res<()> {
    core.stop_bg_task(&id, &task).await.map_err(err)
}

/// The end of the output of a background task of the agent `id`: 8 KiB at most while it runs (its
/// process's, else its output file's), up to 256 KiB of its output file once it is over.
#[tauri::command]
pub async fn task_output(
    core: CoreState<'_>,
    id: String,
    task: String,
) -> Res<crate::taskout::TaskOutput> {
    core.bg_task_output(&id, &task).await.map_err(err)
}

/// The conversation of a subagent of the agent `id` in the Claude Code view (`call` launched it),
/// read from its transcript: a file read, off the async workers. `cursor` is the one the read
/// before gave: null back when its files did not change since.
#[tauri::command]
pub async fn subagent_transcript(
    core: CoreState<'_>,
    id: String,
    call: String,
    cursor: Option<crate::agent::sidechain::Cursor>,
) -> Res<Option<crate::agent::sidechain::SidechainRead>> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.subagent_transcript(&id, &call, cursor))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[tauri::command(async)]
pub fn answer_question(core: CoreState, id: String, request_id: String, answers: Value) -> Res<()> {
    core.answer_question(&id, &request_id, answers).map_err(err)
}

#[tauri::command(async)]
pub fn answer_permission(
    core: CoreState,
    id: String,
    request_id: String,
    decision: String,
    message: Option<String>,
) -> Res<()> {
    core.answer_permission(&id, &request_id, &decision, message)
        .map_err(err)
}

/// « Envoyer » of a review's page: the user's answers, sent to the agent that published it.
#[tauri::command]
pub async fn answer_review(
    core: CoreState<'_>,
    agent_id: String,
    review_id: String,
    answers: crate::review::Answers,
) -> Res<()> {
    core.answer_review(&agent_id, &review_id, answers)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn set_agent_options(
    core: CoreState<'_>,
    id: String,
    model: Option<String>,
    effort: Option<String>,
    mode: Option<String>,
) -> Res<()> {
    core.set_agent_options(&id, model, effort, mode)
        .await
        .map_err(err)
}

/// The Claude account of an agent that has not started (the Composer's chip); empty is
/// « Automatique ».
#[tauri::command]
pub async fn set_agent_account(core: CoreState<'_>, id: String, account: String) -> Res<()> {
    core.set_agent_account(&id, &account).await.map_err(err)
}

/// « Reprendre sur <compte> »: the agent stopped by the usage limit goes on, with its session, on
/// another account.
#[tauri::command]
pub async fn resume_on_account(core: CoreState<'_>, agent_id: String, account: String) -> Res<()> {
    core.resume_on_account(&agent_id, &account)
        .await
        .map_err(err)
}

/// « Revenir sur <compte> »: a resume on another account failed; the agent goes back.
#[tauri::command]
pub async fn back_to_previous_account(core: CoreState<'_>, agent_id: String) -> Res<()> {
    core.back_to_previous_account(&agent_id).await.map_err(err)
}

#[tauri::command]
pub async fn rename_agent(core: CoreState<'_>, id: String, name: String) -> Res<()> {
    core.rename_agent(&id, &name).await.map_err(err)
}

#[tauri::command]
pub async fn duplicate_agent(core: CoreState<'_>, id: String) -> Res<AgentView> {
    core.duplicate_agent(&id).await.map_err(err)
}

#[tauri::command]
pub async fn archive_agent(core: CoreState<'_>, id: String, archived: bool) -> Res<()> {
    core.archive_agent(&id, archived).await.map_err(err)
}

#[tauri::command]
pub async fn delete_agent(
    core: CoreState<'_>,
    id: String,
    remove_worktree: bool,
) -> Res<Option<String>> {
    core.delete_agent(&id, remove_worktree).await.map_err(err)
}

#[tauri::command]
pub async fn merge_agent(
    core: CoreState<'_>,
    id: String,
    squash: bool,
    switch_to_base: bool,
) -> Res<String> {
    let r = core
        .merge_agent(&id, squash, switch_to_base)
        .await
        .map_err(|e| match e.downcast_ref::<NotOnBase>() {
            // The one refusal the frontend turns into a question rather than an error.
            Some(refused) => refused.wire(),
            None => err(e),
        });
    counted(&core, Event::Merged, r)
}

#[tauri::command]
pub async fn get_commands(core: CoreState<'_>, id: String) -> Res<Vec<Value>> {
    let h = core.agent(&id).map_err(err)?;
    let cached = h.lock().commands.clone();
    if !cached.is_empty() {
        return Ok(cached);
    }
    core.ensure_process(&id).await.map_err(err)?;
    let cmds = h.lock().commands.clone();
    Ok(cmds)
}

#[tauri::command]
pub async fn file_suggestions(core: CoreState<'_>, id: String, query: String) -> Res<Vec<String>> {
    core.file_suggestions(&id, &query).await.map_err(err)
}

#[tauri::command]
pub async fn git_files(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
) -> Res<Vec<FileChange>> {
    core.git_files(&project_id, agent_id).await.map_err(err)
}

#[tauri::command]
pub async fn git_diff(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    paths: Vec<String>,
) -> Res<String> {
    core.git_diff(&project_id, agent_id, paths)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn git_project_diff(core: CoreState<'_>, project_id: String) -> Res<Vec<OwnedDiff>> {
    core.git_project_diff(&project_id).await.map_err(err)
}

#[tauri::command]
pub async fn git_log(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
) -> Res<GitLog> {
    core.git_log(&project_id, agent_id).await.map_err(err)
}

#[tauri::command]
pub async fn git_show(core: CoreState<'_>, project_id: String, hash: String) -> Res<String> {
    core.git_show(&project_id, &hash).await.map_err(err)
}

#[tauri::command]
pub async fn git_fetch(core: CoreState<'_>, project_id: String) -> Res<String> {
    let r = core.git_sync(&project_id, SyncOp::Fetch).await.map_err(err);
    counted(&core, Event::GitSync { op: "fetch" }, r)
}

#[tauri::command]
pub async fn git_pull(core: CoreState<'_>, project_id: String) -> Res<String> {
    let r = core.git_sync(&project_id, SyncOp::Pull).await.map_err(err);
    counted(&core, Event::GitSync { op: "pull" }, r)
}

#[tauri::command]
pub async fn git_push(core: CoreState<'_>, project_id: String) -> Res<String> {
    let r = core.git_sync(&project_id, SyncOp::Push).await.map_err(err);
    counted(&core, Event::GitSync { op: "push" }, r)
}

/// A branch operation's error: the refusals the window turns into a question or a message of its
/// own by their code (`BranchRefusal::wire`: `DIRTY`, `IN_WORKTREE:…`, `AGENT_WORKING:…`,
/// `UNMERGED:…`), any other in words.
fn branch_err(e: anyhow::Error) -> String {
    match e.downcast_ref::<crate::core::BranchRefusal>() {
        Some(refused) => refused.wire(),
        None => err(e),
    }
}

/// The project's branches: local ones (the folder's first), then remote ones.
#[tauri::command]
pub async fn branch_list(
    core: CoreState<'_>,
    project_id: String,
) -> Res<Vec<crate::git::BranchInfo>> {
    core.branches(&project_id).await.map_err(err)
}

/// Checks a name for a new branch: as git does, and not taken already.
#[tauri::command]
pub async fn branch_check(core: CoreState<'_>, project_id: String, name: String) -> Res<()> {
    core.branch_check(&project_id, &name).await.map_err(err)
}

/// Switches the project's folder to a branch; the stash's name when its changes were put aside.
#[tauri::command]
pub async fn branch_switch(
    core: CoreState<'_>,
    project_id: String,
    name: String,
    stash: bool,
) -> Res<Option<String>> {
    core.branch_switch(&project_id, &name, stash)
        .await
        .map_err(branch_err)
}

/// Creates a branch at `start` (HEAD when empty), and switches the folder to it with `switch`.
#[tauri::command]
pub async fn branch_create(
    core: CoreState<'_>,
    project_id: String,
    name: String,
    start: String,
    switch: bool,
    stash: bool,
) -> Res<Option<String>> {
    let r = core
        .branch_create(&project_id, &name, &start, switch, stash)
        .await
        .map_err(branch_err);
    counted(&core, Event::BranchCreated, r)
}

/// Deletes a branch (its remote one too with `remote`); unmerged, only with `force`. Returns what
/// is left to tell the user: why a remote branch was kept.
#[tauri::command]
pub async fn branch_delete(
    core: CoreState<'_>,
    project_id: String,
    name: String,
    remote: bool,
    force: bool,
) -> Res<Option<String>> {
    core.branch_delete(&project_id, &name, remote, force)
        .await
        .map_err(branch_err)
}

/// The local branches « Branches mergées » offers to delete.
#[tauri::command]
pub async fn branches_merged(core: CoreState<'_>, project_id: String) -> Res<Vec<String>> {
    core.branches_merged(&project_id).await.map_err(err)
}

/// The diff from `a` to `b`, two branches or commits of the project's repository.
#[tauri::command]
pub async fn git_diff_refs(
    core: CoreState<'_>,
    project_id: String,
    a: String,
    b: String,
) -> Res<String> {
    core.diff_refs(&project_id, &a, &b).await.map_err(err)
}

/// A new agent in a worktree on an existing branch (a local one, or `origin/feat`). Refused for
/// the folder's branch and for one a worktree has: `IN_WORKTREE:…` when an agent's.
#[tauri::command]
pub async fn create_agent_on_branch(
    core: CoreState<'_>,
    project_id: String,
    branch: String,
    model: Option<String>,
) -> Res<AgentView> {
    let agent = core
        .create_agent_on_branch(&project_id, &branch, model)
        .await
        .map_err(branch_err)?;
    core.telemetry.track(Event::AgentCreated {
        worktree: agent.meta.worktree.is_some(),
    });
    Ok(agent)
}

/// « Intégrer <base> » on an agent with a worktree: its base branch goes into its branch.
#[tauri::command]
pub async fn integrate_base(core: CoreState<'_>, id: String) -> Res<crate::core::Integration> {
    core.integrate_base(&id).await.map_err(err)
}

/// The statistics of a period, of every account or (`account`) of one.
#[tauri::command(async)]
pub fn stats(core: CoreState, range: String, account: Option<String>) -> StatsView {
    core.stats_view(&range, account.as_deref().filter(|a| !a.is_empty()))
}

#[tauri::command]
pub async fn refresh_usage(core: CoreState<'_>) -> Res<()> {
    core.inner().refresh_usage().await;
    Ok(())
}

#[tauri::command]
pub async fn git_discard(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    path: String,
) -> Res<()> {
    core.git_discard(&project_id, agent_id, &path)
        .await
        .map_err(err)
}

/// What a direct commit of the agent's changes (the project's own checkout without one) takes.
#[tauri::command]
pub async fn commit_preview(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
) -> Res<CommitScope> {
    core.commit_preview(&project_id, agent_id)
        .await
        .map_err(err)
}

/// Haiku's message for a direct commit of `paths`: only proposed.
#[tauri::command]
pub async fn commit_propose(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    paths: Vec<String>,
) -> Res<String> {
    core.commit_propose(&project_id, agent_id, paths)
        .await
        .map_err(err)
}

/// Commits `paths` with the user's message; the commit's short hash.
#[tauri::command]
pub async fn commit_direct(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    paths: Vec<String>,
    message: String,
) -> Res<String> {
    let r = core
        .commit_direct(&project_id, agent_id, paths, message)
        .await
        .map_err(err);
    counted(&core, Event::Committed, r)
}

// ---------- embedded editor ----------

#[tauri::command]
pub async fn fs_tree(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
) -> Res<fsedit::Tree> {
    core.fs_tree(&project_id, agent_id).await.map_err(err)
}

#[tauri::command]
pub async fn fs_read(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    path: String,
) -> Res<fsedit::FileText> {
    let (root, _) = core.edit_root(&project_id, agent_id).await.map_err(err)?;
    fsedit::read(std::path::Path::new(&root), &path).map_err(|e| {
        match e.downcast_ref::<fsedit::NotFound>() {
            // The editor shows a missing file rather than an error: a code it tells apart.
            Some(missing) => missing.wire(),
            None => err(e),
        }
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn fs_write(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    path: String,
    text: String,
    eol: String,
    bom: bool,
    expected_hash: Option<String>,
) -> Res<String> {
    let (root, worktrees) = core
        .edit_worktrees(&project_id, agent_id)
        .await
        .map_err(err)?;
    let hash = fsedit::write(
        std::path::Path::new(&root),
        &path,
        &text,
        &eol,
        bom,
        expected_hash.as_deref(),
        &worktrees,
    )
    .map_err(err)?;
    core.git.refresh(&project_id);
    core.telemetry.track(Event::FileSaved);
    Ok(hash)
}

#[tauri::command]
pub async fn fs_create(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    path: String,
) -> Res<()> {
    let (root, worktrees) = core
        .edit_worktrees(&project_id, agent_id)
        .await
        .map_err(err)?;
    fsedit::create(std::path::Path::new(&root), &path, &worktrees).map_err(err)?;
    core.git.refresh(&project_id);
    Ok(())
}

/// Renames or moves a file or a folder of the source; refused when something else is at `to`, or
/// when only the case of what git tracks changes.
#[tauri::command]
pub async fn fs_rename(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    from: String,
    to: String,
) -> Res<()> {
    let (root, kept) = core.edit_kept(&project_id, agent_id).await.map_err(err)?;
    // An update's restart waits: a folder on a slow disk can take a moment.
    let _working = core.working();
    fsedit::rename_entry(root, from, to, kept)
        .await
        .map_err(err)?;
    core.git.refresh(&project_id);
    Ok(())
}

/// Sends a file or a folder of the source to the system's trash.
#[tauri::command]
pub async fn fs_delete(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    path: String,
) -> Res<()> {
    let (root, kept) = core.edit_kept(&project_id, agent_id).await.map_err(err)?;
    let _working = core.working();
    // A big folder takes a while to go: off the async workers.
    tokio::task::spawn_blocking(move || {
        fsedit::delete(std::path::Path::new(&root), &path, &kept, fsedit::to_trash)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)?;
    core.git.refresh(&project_id);
    Ok(())
}

/// Shows a file or a folder of the source in the system's file manager, selected; the source's
/// folder opened when no path is given.
#[tauri::command]
pub async fn fs_reveal(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    path: Option<String>,
) -> Res<()> {
    let what = core
        .reveal_target(&project_id, agent_id, path.as_deref())
        .await
        .map_err(err)?;
    // The file manager can take a moment to answer (WSL's share starts its distribution, D-Bus
    // waits for the file manager): off the async workers.
    tokio::task::spawn_blocking(move || fsedit::reveal(what))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

/// Creates an empty folder, its parents with it; refused when something is already there.
#[tauri::command]
pub async fn fs_mkdir(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    path: String,
) -> Res<()> {
    let (root, worktrees) = core
        .edit_worktrees(&project_id, agent_id)
        .await
        .map_err(err)?;
    fsedit::mkdir(std::path::Path::new(&root), &path, &worktrees).map_err(err)
}

#[tauri::command]
pub async fn fs_base(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    path: String,
) -> Res<Option<fsedit::Base>> {
    let (root, base) = core.edit_root(&project_id, agent_id).await.map_err(err)?;
    fsedit::base(&root, base.as_deref(), &path)
        .await
        .map_err(err)
}

/// The lines of the source's files matching `query` (the agent's worktree, else the project).
#[tauri::command]
pub async fn code_search(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    query: search::SearchQuery,
) -> Res<search::SearchResult> {
    let (root, worktrees) = core
        .edit_worktrees(&project_id, agent_id)
        .await
        .map_err(err)?;
    let r = search::search(&root, &query, &worktrees).await.map_err(err);
    counted(&core, Event::CodeSearched, r)
}

#[tauri::command(async)]
pub fn cancel_resume(core: CoreState, id: String) -> Res<()> {
    core.cancel_resume(&id).map_err(err)
}

// ---------- terminals ----------

/// An interactive terminal in the project's folder; in the agent's worktree when `agent_id` has
/// one, and in the folder `subdir` of that (a path of the editor's tree) when it is given.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn term_spawn(
    core: CoreState<'_>,
    project_id: String,
    agent_id: Option<String>,
    subdir: Option<String>,
    shell: String,
    name: String,
    cols: u16,
    rows: u16,
    output: Channel<InvokeResponseBody>,
) -> Res<TermInfo> {
    let cwd = core
        .term_cwd(&project_id, agent_id, subdir.as_deref())
        .await
        .map_err(err)?;
    let settings = core.settings.read().clone();
    let shells = pty::detect_shells(&settings);
    let sh = shells
        .iter()
        .find(|s| s.id == shell)
        .ok_or_else(|| shell_not_found(i18n::ui(), &shell))?;
    let info = TermInfo {
        id: new_id(),
        project_id,
        name,
        shell: sh.id.clone(),
    };
    let env = if settings.proxy_terminals {
        settings.proxy_env()
    } else {
        Vec::new()
    };
    let hub_core = Arc::downgrade(core.inner());
    let id = info.id.clone();
    core.pty
        .spawn(
            info.clone(),
            sh,
            &settings.wsl_distro,
            &cwd,
            (cols, rows),
            env,
            move |bytes| {
                let _ = output.send(InvokeResponseBody::Raw(bytes));
            },
            move |code| {
                if let Some(c) = hub_core.upgrade() {
                    c.hub.emit(UiEvent::TerminalExit { id, code });
                }
            },
        )
        .map_err(err)?;
    core.telemetry.track(Event::TerminalOpened);
    Ok(info)
}

/// Runs one of the project's launch commands in its own terminal; it ends with the command
/// (TerminalExit carries its exit code). Stopped with term_kill, like a terminal.
/// `cursor_row` is the row of its log where it starts.
#[tauri::command(async)]
pub fn run_start(
    core: CoreState,
    project_id: String,
    command_id: String,
    cols: u16,
    rows: u16,
    cursor_row: Option<u16>,
    output: Channel<InvokeResponseBody>,
) -> Res<TermInfo> {
    let project = core.project(&project_id).map_err(err)?;
    let run = project
        .run_commands
        .iter()
        .find(|c| c.id == command_id)
        .ok_or_else(|| {
            tr!(
                "commande de lancement introuvable",
                "launch command not found"
            )
        })?;
    let settings = core.settings.read().clone();
    let shells = pty::detect_shells(&settings);
    // A command set up on another system (PowerShell on Windows, zsh on macOS) runs in the
    // default shell of this one.
    let sh = shells
        .iter()
        .find(|s| s.id == run.shell)
        .or_else(|| shells.first())
        .ok_or_else(|| shell_not_found(i18n::ui(), &run.shell))?;
    let cwd = pty::run_cwd(&project.path, &run.cwd).map_err(err)?;
    let info = TermInfo {
        id: new_id(),
        project_id,
        name: run.name.clone(),
        shell: sh.id.clone(),
    };
    let launch = Launch {
        info,
        shell: sh,
        cwd: &cwd,
        env: Vec::new(),
        command: &run.command,
    };
    let r = spawn_launch(
        core.inner(),
        &settings,
        launch,
        (cols, rows),
        cursor_row,
        output,
    );
    counted(&core, Event::LaunchStarted, r)
}

/// What a launch terminal runs: one of the project's launch commands, or a step of an agent's
/// recipe.
struct Launch<'a> {
    info: TermInfo,
    shell: &'a ShellInfo,
    cwd: &'a str,
    /// Its own variables, after the proxy's when terminals go through it.
    env: Vec<(String, String)>,
    command: &'a str,
}

/// Starts `launch` in a read-only terminal of its own, which ends with its command
/// (TerminalExit carries its exit code). `cursor_row` is the row of its log where it starts.
fn spawn_launch(
    core: &Arc<Core>,
    settings: &Settings,
    launch: Launch,
    size: (u16, u16),
    cursor_row: Option<u16>,
    output: Channel<InvokeResponseBody>,
) -> Res<TermInfo> {
    let mut env = if settings.proxy_terminals {
        settings.proxy_env()
    } else {
        Vec::new()
    };
    env.extend(launch.env);
    let hub_core = Arc::downgrade(core);
    let id = launch.info.id.clone();
    core.pty
        .spawn_command(
            launch.info.clone(),
            launch.shell,
            &settings.wsl_distro,
            launch.cwd,
            size,
            env,
            cursor_row.unwrap_or(1),
            launch.command,
            move |bytes| {
                let _ = output.send(InvokeResponseBody::Raw(bytes));
            },
            move |code| {
                if let Some(c) = hub_core.upgrade() {
                    c.hub.emit(UiEvent::TerminalExit { id, code });
                }
            },
        )
        .map_err(err)?;
    Ok(launch.info)
}

/// Synchronous on purpose: Tauri handles these on the window's main thread, in the order the
/// requests arrive, which keeps the bytes of a terminal in the order they were typed. It costs
/// nothing there: `write` only queues the bytes for the terminal's input thread. Returns the
/// write's number in the terminal's log of writes, which the window sends back with the report of
/// a slow echo (`term_report`).
#[tauri::command]
pub fn term_write(core: CoreState, id: String, data: String) -> Res<u64> {
    core.pty.write(&id, data.as_bytes()).map_err(err)
}

/// Synchronous, so that resizes keep their order among themselves (the last one wins); it waits
/// on nothing heavy. It is not in order with the writes queued before it: the terminal's input
/// thread writes those whenever the program reads.
#[tauri::command]
pub fn term_resize(core: CoreState, id: String, cols: u16, rows: u16) -> Res<()> {
    core.pty.resize(&id, cols, rows).map_err(err)
}

/// Off the main thread, and off the async runtime's workers: ending a process tree (listing the
/// machine's processes, its windows…) and closing the pseudo-console take their time.
#[tauri::command]
pub async fn term_kill(core: CoreState<'_>, id: String) -> Res<()> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.kill_terminal(&id))
        .await
        .map_err(|e| err(e.into()))
}

/// What the window measured of a terminal (the renderer it draws with, an echo that was slow to
/// show), for the log.
#[tauri::command(async)]
pub fn term_report(core: CoreState, id: String, report: TermReport) {
    core.pty.report(&id, report);
}

#[tauri::command]
pub fn play_chime() {
    crate::notify::play_chime();
}

/// « Quitter »: with an update downloaded, it installs silently and the app stays closed.
#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    crate::updates::quit(&app);
}

/// How many files the editor holds unsaved: "Quitter" asks the window first when there are some.
#[tauri::command]
pub fn set_unsaved(core: CoreState, count: usize) {
    core.unsaved
        .store(count, std::sync::atomic::Ordering::Release);
}

/// Turns Remote Control on or off for the agent. True when, in the Claude Code view, the change
/// waits for the interface's next start: it runs, and was started with the other state.
#[tauri::command]
pub async fn set_remote_control(core: CoreState<'_>, id: String, enabled: bool) -> Res<bool> {
    let at_next_start = core.set_remote_control(&id, enabled).await.map_err(err)?;
    if enabled {
        core.telemetry
            .track(Event::RemoteControlEnabled { from: "agent" });
    }
    Ok(at_next_start)
}

// ---------- board ----------

#[tauri::command]
pub async fn ticket_create(
    core: CoreState<'_>,
    project_id: String,
    draft: TicketDraft,
) -> Res<Ticket> {
    let r = core.ticket_create(&project_id, draft).await.map_err(err);
    counted(&core, Event::TicketCreated, r)
}

/// Its pasted images are written: off the async workers.
#[tauri::command]
pub async fn ticket_update(core: CoreState<'_>, id: String, draft: TicketDraft) -> Res<Ticket> {
    let core = core.inner().clone();
    tokio::task::spawn_blocking(move || core.ticket_update(&id, draft))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

/// The images of a ticket, with their content, for its form to show: read off the async workers.
#[tauri::command]
pub async fn ticket_images(
    core: CoreState<'_>,
    id: String,
) -> Res<Vec<crate::ticketimages::ImageData>> {
    let core = core.inner().clone();
    tokio::task::spawn_blocking(move || core.ticket_images(&id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[tauri::command]
pub async fn ticket_delete(core: CoreState<'_>, id: String) -> Res<()> {
    core.ticket_delete(&id).await.map_err(err)
}

#[tauri::command(async)]
pub fn ticket_prioritize(core: CoreState, id: String) -> Res<()> {
    core.ticket_prioritize(&id).map_err(err)
}

#[tauri::command(async)]
pub fn ticket_start(core: CoreState, id: String) -> Res<()> {
    core.ticket_start(&id).map_err(err)
}

#[tauri::command]
pub async fn ticket_resume(core: CoreState<'_>, id: String) -> Res<()> {
    core.ticket_resume(&id).await.map_err(err)
}

#[tauri::command]
pub async fn ticket_approve(core: CoreState<'_>, id: String) -> Res<()> {
    let r = core.ticket_approve(&id).await.map_err(err);
    counted(&core, Event::TicketApproved, r)
}

#[tauri::command]
pub async fn ticket_reject(core: CoreState<'_>, id: String, comment: String) -> Res<()> {
    core.ticket_reject(&id, &comment).await.map_err(err)
}

#[tauri::command]
pub async fn ticket_resolve_conflict(core: CoreState<'_>, id: String) -> Res<()> {
    core.ticket_resolve_conflict(&id).await.map_err(err)
}

#[tauri::command(async)]
pub fn ticket_dismiss(core: CoreState, id: String) -> Res<()> {
    core.ticket_dismiss(&id).map_err(err)
}

#[tauri::command(async)]
pub fn board_set(core: CoreState, project_id: String, settings: BoardSettings) -> Res<Project> {
    core.board_set(&project_id, settings).map_err(err)
}

/// « Reprendre maintenant » in the board's header: the autopilot's pause is lifted.
#[tauri::command(async)]
pub fn autopilot_resume(core: CoreState) {
    core.autopilot_resume();
}

#[tauri::command]
pub async fn git_branches(core: CoreState<'_>, project_id: String) -> Res<Vec<String>> {
    core.git_branches(&project_id).await.map_err(err)
}

#[tauri::command]
pub async fn agent_prepare_launch(core: CoreState<'_>, id: String) -> Res<()> {
    core.agent_prepare_launch(&id).await.map_err(err)
}

/// « Lancer » in the test modal: the user read `recipe`, the agent's recipe as the modal showed it,
/// and lets it run. Refused when the agent's recipe is no longer that one.
#[tauri::command(async)]
pub fn test_recipe_approve(core: CoreState, agent_id: String, recipe: TestRecipe) -> Res<()> {
    core.approve_recipe(&agent_id, recipe).map_err(err)
}

/// Runs a step of an agent's recipe ("prep" or "run", its index) in its own read-only terminal,
/// like a launch command, with the system's default shell and the reserved ports exported. One
/// whose agent was archived, deleted or put to validation meanwhile is stopped at once, and the
/// error tells why. A recipe the user did not approve (`test_recipe_approve`) runs nothing.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn test_run_start(
    core: CoreState,
    agent_id: String,
    kind: String,
    index: usize,
    cols: u16,
    rows: u16,
    cursor_row: Option<u16>,
    output: Channel<InvokeResponseBody>,
) -> Res<TermInfo> {
    let spec = core.test_run_spec(&agent_id, &kind, index).map_err(err)?;
    let project_id = core
        .agent(&agent_id)
        .map_err(err)?
        .lock()
        .meta
        .project_id
        .clone();
    let settings = core.settings.read().clone();
    let shells = pty::detect_shells(&settings);
    let sh = shells.first().ok_or_else(no_shell)?;
    let info = TermInfo {
        id: new_id(),
        project_id,
        name: spec.name,
        shell: sh.id.clone(),
    };
    let launch = Launch {
        info,
        shell: sh,
        cwd: &spec.cwd,
        env: spec.env,
        command: &spec.command,
    };
    let info = spawn_launch(
        core.inner(),
        &settings,
        launch,
        (cols, rows),
        cursor_row,
        output,
    )?;
    core.track_test_run(&agent_id, &info.id).map_err(err)?;
    core.telemetry.track(Event::TestRunStarted);
    Ok(info)
}

/// The `.isola.toml` of the agent's worktree, whole, as `isola up` will read it: what the test modal
/// shows before the services start.
#[tauri::command(async)]
pub fn isola_config(core: CoreState, agent_id: String) -> Res<String> {
    core.isola_config(&agent_id).map_err(err)
}

/// « Lancer » in the test modal of an agent whose services isola runs: the user read `config` and the
/// address to open `open`. Refused when the file or the address is no longer what the modal showed.
#[tauri::command(async)]
pub fn isola_approve(core: CoreState, agent_id: String, config: String, open: String) -> Res<()> {
    core.approve_isola(&agent_id, config, open).map_err(err)
}

/// The services isola runs for the agent's worktree, with their addresses.
#[tauri::command]
pub async fn isola_services(
    core: CoreState<'_>,
    agent_id: String,
) -> Res<Vec<crate::isola::Service>> {
    core.isola_services(&agent_id).await.map_err(err)
}

/// Stops the services isola runs for the agent's worktree.
#[tauri::command]
pub async fn isola_down(core: CoreState<'_>, agent_id: String) -> Res<()> {
    core.isola_down(&agent_id).await.map_err(err)
}

/// The setup and teardown Claude suggests for the project's worktrees, from what it reads of it.
#[tauri::command]
pub async fn suggest_worktree_steps(
    core: CoreState<'_>,
    project_id: String,
) -> Res<crate::worktrees::WorktreeSuggestion> {
    core.suggest_worktree_steps(&project_id).await.map_err(err)
}

/// The launch commands Claude suggests for the project, from what it reads of it (nothing is saved), with how many it gave that were refused.
#[tauri::command]
pub async fn suggest_run_commands(
    core: CoreState<'_>,
    project_id: String,
) -> Res<crate::worktrees::RunSuggestion> {
    core.suggest_run_commands(&project_id).await.map_err(err)
}

/// Looks for a newer release, through the network settings as they are now. `manual`: the user
/// asked, a failure is shown whatever the checks before it did; else it is kept quiet until it goes
/// on. A failure is an answer (`failed`, with its cause), not an error of the command.
#[tauri::command]
pub async fn update_check(
    app: tauri::AppHandle,
    core: CoreState<'_>,
    updates: State<'_, crate::updates::Updates>,
    manual: bool,
) -> Res<crate::updates::Checked> {
    let settings = core.settings.read().clone();
    Ok(crate::updates::check(&app, &settings, &updates, manual).await)
}

/// Downloads the update found as `id`, its signature checked, and keeps it until it installs. A
/// download that fails reads latest.json again and is tried once more before it is told (`failed`,
/// with its cause and the release's page, for the browser), under the rule of `update_check`
/// for `manual`.
#[tauri::command]
pub async fn update_download(
    updates: State<'_, crate::updates::Updates>,
    id: u32,
    manual: bool,
) -> Res<crate::updates::Downloaded> {
    Ok(updates.downloaded(id, manual).await)
}

/// « Redémarrer maintenant »: the update downloaded installs, the app stopped cleanly first, and
/// the app starts again on it.
#[tauri::command]
pub async fn update_restart(app: tauri::AppHandle) -> Res<()> {
    crate::updates::restart_now(app).await.map_err(err)
}

/// « Plus tard »: the automatic restart planned is called off.
#[tauri::command]
pub fn update_postpone(app: tauri::AppHandle) {
    crate::updates::postpone(&app);
}

/// What the window says of itself for an automatic restart: a modal open, a test launch being
/// prepared, its last key or mouse input.
#[tauri::command]
pub fn update_presence(
    updates: State<crate::updates::Updates>,
    presence: crate::updates::Presence,
) {
    updates.set_presence(presence);
}

/// The update found as `id` is no longer offered.
#[tauri::command]
pub fn update_close(updates: State<crate::updates::Updates>, id: u32) {
    updates.free(id);
}

/// One HTTP request to `url` (http or https only) without proxy, 2 s at most: true for any
/// answer.
#[tauri::command]
pub async fn http_ready(url: String) -> bool {
    crate::testlaunch::http_ready(&url).await
}

// ---------- external ticket systems ----------

#[tauri::command]
pub async fn integration_connect(
    core: CoreState<'_>,
    service: Service,
    account: Account,
) -> Res<AccountView> {
    let r = core
        .integration_connect(service, account)
        .await
        .map_err(err);
    let service = telemetry::service(service);
    counted(&core, Event::IntegrationConnected { service }, r)
}

#[tauri::command]
pub fn integration_disconnect(core: CoreState, service: Service) -> Res<Vec<AccountView>> {
    core.integration_disconnect(service).map_err(err)
}

#[tauri::command]
pub async fn integration_containers(
    core: CoreState<'_>,
    service: Service,
    project_id: Option<String>,
) -> Res<Vec<Container>> {
    core.integration_containers(service, project_id.as_deref())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn integration_states(
    core: CoreState<'_>,
    service: Service,
    container: String,
) -> Res<StatesView> {
    core.integration_states(service, &container)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn integration_issues(
    core: CoreState<'_>,
    project_id: String,
    service: Service,
    text: String,
    filters: Vec<String>,
    page: Option<String>,
) -> Res<IssuePage> {
    let q = Query {
        text,
        filters,
        label: None,
        page,
    };
    core.integration_issues(&project_id, service, q)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn integration_import(
    core: CoreState<'_>,
    project_id: String,
    issues: Vec<ExternalIssue>,
    max_loops: u32,
) -> Res<Vec<Ticket>> {
    let service = issues.first().map(|i| telemetry::service(i.service));
    let r = core
        .integration_import(&project_id, issues, max_loops)
        .await
        .map_err(err);
    match service {
        Some(service) => counted(&core, Event::TicketsImported { service }, r),
        None => r,
    }
}

/// « Resynchroniser » (the ⚠ of an imported ticket's card): its failed syncs go again at once.
#[tauri::command]
pub async fn integration_resync(core: CoreState<'_>, ticket_id: String) -> Res<()> {
    core.integration_resync(&ticket_id).await.map_err(err)
}

/// The MCP server (« Claude et les agents peuvent piloter Escouade »): running or not, its port, why it failed.
// Off the main thread: a start waiting for its port is no reason for the window to wait.
#[tauri::command(async)]
pub fn mcp_status(core: CoreState) -> crate::mcp::McpStatus {
    core.mcp.status()
}

/// The MCP server's activity log, oldest first (the last 200 entries).
#[tauri::command]
pub fn mcp_activity(core: CoreState) -> Vec<crate::mcp::activity::ActivityEntry> {
    core.mcp.activity.entries()
}

/// « Effacer » of the MCP activity log.
#[tauri::command]
pub fn mcp_clear_activity(core: CoreState) {
    core.mcp.activity.clear();
}

/// « Claude et les agents peuvent piloter Escouade » turned on or off: saved at once, not through the settings
/// modal's draft (it writes into the Claude Code config of each account, which « Annuler » could
/// not take back). The declaration follows by itself (`mcp_declare_status`, `mcpDeclared`).
#[tauri::command(async)]
pub fn mcp_set_enabled(core: CoreState, enabled: bool) -> Res<()> {
    core.set_mcp_enabled(enabled).map_err(err)?;
    if enabled {
        core.telemetry.track(Event::McpEnabled);
    }
    Ok(())
}

/// The answer to « Statistiques d’usage », at the first start.
#[tauri::command(async)]
pub fn telemetry_set(core: CoreState, consent: bool) -> Res<()> {
    core.set_telemetry(consent).map_err(err)
}

/// Where the server stands in each active account's Claude Code: declared, or why not and the
/// command to run by hand, its token hidden.
#[tauri::command]
pub fn mcp_declare_status(core: CoreState) -> Vec<crate::mcp::install::Declaration> {
    core.mcp.declared()
}

/// The command that declares the server by hand in the account, with the real token: asked for
/// when the user copies it, never sent with the state.
#[tauri::command(async)]
pub fn mcp_manual_command(core: CoreState, account: String) -> Res<String> {
    core.mcp_manual_command(&account).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_refusals_of_a_terminal_read_in_english() {
        use crate::i18n::Lang::{En, Fr};
        assert_eq!(shell_not_found(En, "zsh"), "shell “zsh” not found");
        assert_eq!(shell_not_found(Fr, "zsh"), "shell « zsh » introuvable");
    }

    #[test]
    fn the_commands_of_the_background_tasks_are_in_the_handler_the_window_calls() {
        // A command that is written and not listed there is never reachable from the window.
        let lib = include_str!("lib.rs");
        let handler = lib
            .split_once("generate_handler![")
            .and_then(|(_, rest)| rest.split_once("])"))
            .map(|(list, _)| list)
            .expect("the handler list");
        for name in ["stop_bg_task", "task_output"] {
            assert_eq!(
                handler.matches(&format!("commands::{name},")).count(),
                1,
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn adding_a_folder_is_counted_as_a_project_created_unless_a_history_came_back() {
        let h = crate::core_tests::harness("commands-project-counted");
        h.core.set_telemetry(true).unwrap();
        let created = || {
            h.core
                .telemetry
                .pending()
                .iter()
                .filter(|e| e["eventName"] == "project_created")
                .count()
        };
        let (p, r) = h.project(false).await;
        assert_eq!(created(), 0, "the core itself counts nothing");

        // A new project is counted; so is the next one.
        assert!(project_added(&h.core, Ok((p.clone(), false))).is_ok());
        assert_eq!(created(), 1);
        assert!(project_added(&h.core, Ok((p.clone(), false))).is_ok());
        assert_eq!(created(), 2);
        // One that was refused is not, and neither is a history that came back.
        assert!(project_added(&h.core, Err("non".into())).is_err());
        assert!(project_added(&h.core, Ok((p.clone(), true))).is_ok());
        assert_eq!(created(), 2);

        // What the core says of a real adding: a blank project, then the same folder closed and
        // added again.
        let (_, back) = h
            .core
            .add_project(
                &r.to_string_lossy(),
                "",
                "oklch(0.7 0.1 20)",
                false,
                None,
                false,
                None,
            )
            .await
            .unwrap();
        assert!(!back);
        h.core.remove_project(&p.id, false).await.unwrap();
        let (again, back) = h
            .core
            .add_project(
                &r.to_string_lossy(),
                "",
                "oklch(0.7 0.1 20)",
                false,
                None,
                true,
                None,
            )
            .await
            .unwrap();
        assert!(back && again.id == p.id);
    }
}
