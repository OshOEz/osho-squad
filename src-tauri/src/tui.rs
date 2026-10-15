//! « Vue Claude Code »: an agent run in Claude Code's own interface (its TUI) in a terminal, in
//! place of the stream that Escouade's conversation view drives. A session has one process at a
//! time: switching stops the one and resumes the session in the other (`--resume`).
//!
//! The window still knows what the agent does, from two sources (docs/PROTOCOL.md, « Vue Claude
//! Code »):
//! - its hooks, given to it with `--settings` as HTTP hooks to a server of Escouade's on
//!   127.0.0.1, behind a token of its own (in its environment, never on its command line): it
//!   runs, it waits for the user, it stopped;
//! - its transcript, read as it is written (`agent::transcript`): the conversation, its turns and
//!   what they cost, the plan.
//!
//! A ticket's agent never runs there: its board drives it over the stream.

use crate::accounts;
use crate::agent::{Effects, TuiRun};
use crate::core::Core;
use crate::model::*;
use crate::pty::TermInfo;
use anyhow::{anyhow, bail, Result};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{header, Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::io::{Read, Seek, SeekFrom};
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tauri::Runtime;

/// The path of the hooks' endpoint.
const PATH: &str = "/hook";
/// The variable that gives an interface its token.
const TOKEN_VAR: &str = "ESCOUADE_HOOK_TOKEN";
/// How often the transcript of a running interface is read.
const READ_EVERY: Duration = Duration::from_millis(400);
/// The most a hook may send (its tool's input, a prompt with a pasted file).
const MAX_BODY: usize = 4 << 20;
/// The hooks Escouade listens to.
const EVENTS: &[&str] = &[
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PostToolUseFailure",
    "Notification",
    "Stop",
    "StopFailure",
];

/// The server the interfaces' hooks call: started with the first interface, until the app quits.
#[derive(Default)]
pub struct Hooks {
    port: Mutex<Option<u16>>,
    /// Each running interface's token, and its agent.
    tokens: Mutex<HashMap<String, String>>,
}

impl Hooks {
    pub(crate) fn grant(&self, agent_id: &str) -> String {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        self.tokens
            .lock()
            .insert(token.clone(), agent_id.to_string());
        token
    }

    pub(crate) fn revoke(&self, agent_id: &str) {
        self.tokens.lock().retain(|_, a| a != agent_id);
    }

    fn agent_of(&self, token: &str) -> Option<String> {
        self.tokens.lock().get(token).cloned()
    }
}

/// The settings an interface is started with (`--settings <file>`): its hooks, each an HTTP call
/// to `port` with the token of its environment.
pub fn hook_settings(port: u16) -> Value {
    let hook = json!([{ "hooks": [{
        "type": "http",
        "url": format!("http://127.0.0.1:{port}{PATH}"),
        "headers": { "Authorization": format!("Bearer ${TOKEN_VAR}") },
        "allowedEnvVars": [TOKEN_VAR],
        "timeout": 5,
    }] }]);
    let hooks: serde_json::Map<String, Value> = EVENTS
        .iter()
        .map(|e| (e.to_string(), hook.clone()))
        .collect();
    json!({ "hooks": hooks })
}

/// The arguments of an agent's interface, as its stream's are (`claude_args`) less the stream's
/// own; `session` is the session it starts or forks into, when it resumes none.
pub fn tui_args(m: &AgentMeta, session: Option<&str>, settings_file: &Path) -> Vec<String> {
    let mut a: Vec<String> = vec!["--model".into(), m.model.clone()];
    if crate::core::supports_effort(&m.model) {
        a.extend(["--effort".into(), m.effort.clone()]);
    }
    a.extend([
        "--permission-mode".into(),
        m.mode.clone(),
        "--allow-dangerously-skip-permissions".into(),
    ]);
    if let Some(p) = m.append_prompt.as_deref().filter(|p| !p.trim().is_empty()) {
        let one_line = p.split_whitespace().collect::<Vec<_>>().join(" ");
        a.extend(["--append-system-prompt".into(), one_line]);
    }
    match (&m.session_id, &m.fork_of) {
        (Some(s), _) => a.push(format!("--resume={s}")),
        (None, Some(f)) => {
            a.extend([format!("--resume={f}"), "--fork-session".into()]);
            if let Some(at) = &m.fork_at {
                a.push(format!("--resume-session-at={at}"));
            }
            a.extend(session.map(|s| format!("--session-id={s}")));
        }
        (None, None) => a.extend(session.map(|s| format!("--session-id={s}"))),
    }
    a.extend([
        "--settings".into(),
        settings_file.to_string_lossy().into_owned(),
    ]);
    a
}

/// The transcript of `session` in an account's folder: `projects/<folder>/<session>.jsonl`, the
/// one written last if several folders have it.
pub fn find_transcript(config_dir: &Path, session: &str) -> Option<PathBuf> {
    let file = format!("{session}.jsonl");
    std::fs::read_dir(config_dir.join("projects"))
        .ok()?
        .flatten()
        .filter_map(|folder| {
            let path = folder.path().join(&file);
            let at = std::fs::metadata(&path).ok()?.modified().ok()?;
            Some((at, path))
        })
        .max_by_key(|(at, _)| *at)
        .map(|(_, path)| path)
}

/// The complete lines of `path` from `offset` on (a line still being written waits for the next
/// read), and the offset after them.
pub fn read_lines(path: &Path, offset: u64) -> std::io::Result<(Vec<Value>, u64)> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    // Written again from the start (another file under the same name): read from there.
    let offset = if len < offset { 0 } else { offset };
    f.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::new();
    f.take(len - offset).read_to_end(&mut buf)?;
    let Some(end) = buf.iter().rposition(|b| *b == b'\n') else {
        return Ok((Vec::new(), offset));
    };
    let lines = buf[..end]
        .split(|b| *b == b'\n')
        .filter_map(|l| serde_json::from_slice::<Value>(l).ok())
        .collect();
    Ok((lines, offset + end as u64 + 1))
}

/// An entry written before `since` (ms): one of the conversation's history (a session forked or
/// resumed with no read kept), which its view already has.
fn before(e: &Value, since: i64) -> bool {
    e["timestamp"]
        .as_str()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .is_some_and(|t| t.timestamp_millis() < since)
}

/// `NO_PROXY` with the loopback in it: the hooks' calls never go through a proxy.
fn no_proxy(env: &mut Vec<(String, String)>) {
    const LOOPBACK: &str = "127.0.0.1,localhost,::1";
    for key in ["NO_PROXY", "no_proxy"] {
        match env.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) if !v.trim().is_empty() => *v = format!("{v},{LOOPBACK}"),
            Some((_, v)) => *v = LOOPBACK.into(),
            None => env.push((key.into(), LOOPBACK.into())),
        }
    }
}

/// How long an image pasted in an interface is kept: Claude Code reads its file again when the
/// session is resumed, and the model may look at it by its path.
const PASTES_KEPT: Duration = Duration::from_secs(7 * 24 * 3600);

/// The extension Claude Code knows an image by, for the images it reads.
fn image_extension(media_type: &str) -> Option<&'static str> {
    match media_type {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

/// Removes the pasted images older than `PASTES_KEPT` from `dir`.
fn clear_pastes(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let aged = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > PASTES_KEPT);
        if aged {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Refused: a ticket's agent is driven by its board, over the stream.
fn ticket_refusal() -> anyhow::Error {
    anyhow!(tr!(
        "Un agent de ticket reste dans la vue d’Escouade : son tableau le pilote.",
        "A ticket’s agent stays in Escouade’s view: its board drives it."
    ))
}

impl<R: Runtime> Core<R> {
    /// The port of the hooks' server, once started.
    #[cfg(test)]
    pub(crate) fn hooks_port(&self) -> Option<u16> {
        *self.hooks.port.lock()
    }

    /// The port of the hooks' server, started the first time.
    fn hook_port(self: &Arc<Self>) -> Result<u16> {
        let mut port = self.hooks.port.lock();
        if let Some(p) = *port {
            return Ok(p);
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let p = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let me = Arc::downgrade(self);
        tauri::async_runtime::spawn(serve(listener, me, p));
        *port = Some(p);
        Ok(p)
    }

    /// « Vue Claude Code » on or off for the agent. On, its stream's process stops (the window
    /// then opens its interface, `open_agent_terminal`); off, its interface stops and the stream
    /// takes its session up again.
    pub async fn set_agent_view(self: &Arc<Self>, id: &str, terminal: bool) -> Result<()> {
        let h = self.agent(id)?;
        let lock = self.spawn_lock(id);
        let _guard = lock.lock().await;
        {
            let rt = h.lock();
            if rt.meta.terminal == terminal {
                return Ok(());
            }
            if terminal && rt.meta.ticket_id.is_some() {
                return Err(ticket_refusal());
            }
            if terminal && rt.meta.remote_control {
                bail!(tr!(
                    "Coupe d’abord le remote control : dans l’interface de Claude Code, c’est sa commande /remote-control qui le fait.",
                    "Turn remote control off first: in Claude Code’s interface, its /remote-control command does it."
                ));
            }
            if rt.meta.status.is_active() && rt.tui.is_none() {
                bail!(tr!(
                    "L’agent travaille encore : change de vue quand son tour est fini.",
                    "The agent is still working: switch views once its turn is over."
                ));
            }
        }
        if terminal {
            let stopped = {
                let mut rt = h.lock();
                rt.meta.terminal = true;
                rt.detach()
            };
            if let Some(p) = stopped {
                p.close_input();
                stop_and_wait(&p).await;
            }
        } else {
            self.read_transcript(id);
            let term = {
                let mut fx = Effects::default();
                let mut rt = h.lock();
                rt.meta.terminal = false;
                rt.gen += 1;
                let term = rt.tui.as_ref().map(|t| t.term_id.clone());
                rt.stop_tui(&mut fx);
                term
            };
            self.hooks.revoke(id);
            if let Some(t) = term {
                self.pty.kill(&t);
            }
        }
        self.emit_agent(&h);
        self.request_save();
        if !terminal {
            self.warm(id);
        }
        Ok(())
    }

    /// Claude Code's interface for the agent, in a terminal the window shows (`on_data`): its
    /// session resumed, or started. One it had already stops first.
    pub async fn open_agent_terminal(
        self: &Arc<Self>,
        id: &str,
        size: (u16, u16),
        on_data: impl Fn(Vec<u8>) + Send + 'static,
    ) -> Result<TermInfo> {
        let h = self.agent(id)?;
        let lock = self.spawn_lock(id);
        let _guard = lock.lock().await;
        if self.quitting.load(std::sync::atomic::Ordering::Acquire) {
            bail!(crate::core::quitting_refusal());
        }
        {
            let rt = h.lock();
            if !rt.meta.terminal {
                bail!(tr!(
                    "L’agent n’est pas en vue Claude Code.",
                    "The agent is not in the Claude Code view."
                ));
            }
            if rt.meta.ticket_id.is_some() {
                return Err(ticket_refusal());
            }
            if rt.meta.archived {
                bail!(tr!("L’agent est archivé.", "The agent is archived."));
            }
        }
        // Never two processes on one session.
        let stopped = h.lock().detach();
        if let Some(p) = stopped {
            p.close_input();
            stop_and_wait(&p).await;
        }
        if let Some(old) = h.lock().tui.take() {
            self.pty.kill(&old.term_id);
        }
        self.hooks.revoke(id);
        let settings = self.settings.read().clone();
        let account = accounts::get(&settings, &h.lock().meta.account);
        let program = accounts::program(&account, &settings).ok_or_else(|| {
            anyhow!(tr!(
                "Claude Code introuvable. Installe-le ou indique son chemin dans les réglages.",
                "Claude Code not found. Install it or give its path in the settings."
            ))
        })?;
        let port = self.hook_port()?;
        let dir = self.data.root().join("tui");
        std::fs::create_dir_all(&dir)?;
        let settings_file = dir.join(format!("{id}.json"));
        std::fs::write(
            &settings_file,
            serde_json::to_vec_pretty(&hook_settings(port))?,
        )?;
        let config_dir = accounts::config_dir(&account, &settings);
        let started = now_ms();
        let (args, cwd, project_id, name, gen, transcript, since) = {
            let mut rt = h.lock();
            // A session of its own from the start: its transcript is known before any hook.
            let new_session = rt
                .meta
                .session_id
                .is_none()
                .then(|| uuid::Uuid::new_v4().to_string());
            let args = tui_args(&rt.meta, new_session.as_deref(), &settings_file);
            if let Some(s) = new_session {
                rt.meta.session_id = Some(s);
                rt.meta.fork_of = None;
                rt.meta.fork_at = None;
            }
            let sid = rt.meta.session_id.clone().unwrap_or_default();
            let file = find_transcript(&config_dir, &sid);
            // Where the last read stopped, when it was this file; else from now on.
            let kept = rt
                .meta
                .transcript_read
                .clone()
                .filter(|(p, _)| file.as_deref() == Some(Path::new(p)));
            let (transcript, offset, since) = match (kept, file) {
                (Some((p, off)), _) => (Some(PathBuf::from(p)), off, None),
                (None, Some(f)) => {
                    let len = std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
                    (Some(f), len, None)
                }
                (None, None) => (None, 0, Some(started - 2_000)),
            };
            rt.gen += 1;
            rt.start_tui();
            (
                args,
                rt.meta.cwd.clone(),
                rt.meta.project_id.clone(),
                rt.meta.name.clone(),
                rt.gen,
                (transcript, offset),
                since,
            )
        };
        if !Path::new(&cwd).is_dir() {
            bail!(tr!(
                "Le dossier {dir} n'existe plus",
                "The folder {dir} no longer exists",
                dir = cwd
            ));
        }
        self.ready_for_wsl(&program, &h);
        let mut args = args;
        let access = self.agent_access(id, &project_id);
        args.extend(access.args());
        let token = self.hooks.grant(id);
        let mut env = [settings.agent_env(), accounts::launch_env(&account)].concat();
        env.push((TOKEN_VAR.into(), token));
        no_proxy(&mut env);
        let info = TermInfo {
            id: new_id(),
            // No project's: the window lists it with no project's terminals.
            project_id: String::new(),
            name,
            shell: "claude".into(),
        };
        log::info!(
            "agent {id}: starting Claude Code's interface {} {} in {cwd} (account {})",
            program.describe(),
            args.join(" "),
            account.id
        );
        let (me, agent, term, granted) = (
            Arc::downgrade(self),
            id.to_string(),
            info.id.clone(),
            access.token().map(str::to_string),
        );
        let spawned = self.pty.spawn_program(
            info.clone(),
            &program,
            &args,
            &cwd,
            size,
            env,
            // Everything it started goes with it.
            false,
            on_data,
            move |code| {
                if let Some(c) = me.upgrade() {
                    if let Some(t) = &granted {
                        c.mcp.release_agent(&agent, t);
                    }
                    c.on_tui_exit(&agent, gen, &term, code);
                }
            },
        );
        if let Err(e) = spawned {
            self.hooks.revoke(id);
            if let Some(t) = access.token() {
                self.mcp.release_agent(id, t);
            }
            return Err(e);
        }
        {
            let mut rt = h.lock();
            rt.tui = Some(TuiRun {
                term_id: info.id.clone(),
                gen,
                transcript: transcript.0,
                offset: transcript.1,
                program,
            });
            rt.tui_since = since;
        }
        self.emit_agent(&h);
        self.request_save();
        let (c, agent) = (self.clone(), id.to_string());
        tauri::async_runtime::spawn(async move { c.read_loop(&agent, gen).await });
        Ok(info)
    }

    /// An image pasted in the agent's interface (base64 `data`), which reads the clipboard's only
    /// on a key of its own: written to a file, whose path the window types there. Claude Code takes
    /// a pasted path to an image for the image, as a file dropped on a terminal. The path is the
    /// one its `claude` reads (in WSL, the Linux one).
    pub fn paste_image(&self, id: &str, media_type: &str, data: &str) -> Result<String> {
        use base64::Engine;
        let program = self
            .agent(id)?
            .lock()
            .tui
            .as_ref()
            .map(|t| t.program.clone())
            .ok_or_else(|| {
                anyhow!(tr!(
                    "L’agent n’est pas en vue Claude Code.",
                    "The agent is not in the Claude Code view."
                ))
            })?;
        let Some(extension) = image_extension(media_type) else {
            bail!(tr!(
                "Claude Code ne lit pas les images {media_type}",
                "Claude Code does not read {media_type} images",
                media_type = media_type
            ));
        };
        let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
        let dir = self.data.root().join("tui").join("pastes");
        std::fs::create_dir_all(&dir)?;
        clear_pastes(&dir);
        let file = dir.join(format!("{}.{extension}", uuid::Uuid::new_v4().simple()));
        std::fs::write(&file, bytes)?;
        Ok(program.file_path(&file))
    }

    /// Reads the transcript of the agent's interface `gen` as it is written, until it stops.
    async fn read_loop(self: Arc<Self>, id: &str, gen: u64) {
        loop {
            tokio::time::sleep(READ_EVERY).await;
            let Ok(h) = self.agent(id) else { return };
            if h.lock().tui.as_ref().is_none_or(|t| t.gen != gen) {
                return;
            }
            self.read_transcript(id);
        }
    }

    /// What the agent's interface wrote in its transcript since the last read.
    pub(crate) fn read_transcript(self: &Arc<Self>, id: &str) {
        let Ok(h) = self.agent(id) else { return };
        let (gen, path, offset, account, sid) = {
            let rt = h.lock();
            let Some(t) = rt.tui.as_ref() else { return };
            (
                t.gen,
                t.transcript.clone(),
                t.offset,
                rt.meta.account.clone(),
                rt.meta.session_id.clone(),
            )
        };
        // Not written yet when the interface started: looked for until it is.
        let path = match path {
            Some(p) => p,
            None => {
                let settings = self.settings.read().clone();
                let account = accounts::get(&settings, &account);
                let Some(found) = sid
                    .and_then(|s| find_transcript(&accounts::config_dir(&account, &settings), &s))
                else {
                    return;
                };
                found
            }
        };
        let (lines, next) = match read_lines(&path, offset) {
            Ok(read) => read,
            Err(e) => {
                log::debug!("agent {id}: transcript not read: {e}");
                return;
            }
        };
        let mut fx = Effects::default();
        let (pid, name, view, naming) = {
            let mut rt = h.lock();
            if rt.tui.as_ref().is_none_or(|t| t.gen != gen) {
                return;
            }
            let mut naming = None;
            for e in &lines {
                if rt.tui_since.is_some_and(|since| before(e, since)) {
                    continue;
                }
                let prompts = rt.meta.prompts;
                rt.on_transcript(e, &mut fx);
                // A message of the user's names the agent, as one sent from the window does: the
                // first that gives a text to name it from (not a command with nothing to say, nor
                // what the app typed by itself).
                if rt.meta.prompts > prompts {
                    let prompt = rt.tui_prompt.clone().unwrap_or_default();
                    if !rt.took_unprompted(&prompt) && naming.is_none() {
                        naming = rt.name_from(&prompt);
                    }
                }
            }
            if let Some(t) = rt.tui.as_mut() {
                t.transcript = Some(path.clone());
                t.offset = next;
            }
            let read = Some((path.to_string_lossy().into_owned(), next));
            if rt.meta.transcript_read != read {
                rt.meta.transcript_read = read;
            }
            let view = fx.agent_changed.then(|| rt.view());
            (
                rt.meta.project_id.clone(),
                rt.meta.name.clone(),
                view,
                naming,
            )
        };
        if lines.is_empty() && fx.ops.is_empty() {
            return;
        }
        self.apply(id, &pid, &name, fx, view);
        if let Some(text) = naming {
            let (c, id) = (self.clone(), id.to_string());
            tauri::async_runtime::spawn(async move { c.auto_name(&id, &text).await });
        }
    }

    /// A hook of the agent's interface: what it tells, after what its transcript said before.
    pub(crate) fn on_tui_hook(self: &Arc<Self>, id: &str, hook: Value) {
        self.read_transcript(id);
        let Ok(h) = self.agent(id) else { return };
        // A `claude` of WSL tells its transcript by its Linux path.
        let wsl = {
            let settings = self.settings.read().clone();
            let account = accounts::get(&settings, &h.lock().meta.account);
            accounts::program(&account, &settings).and_then(|p| p.wsl().cloned())
        };
        let mut fx = Effects::default();
        let (out, pid, name, view, gen) = {
            let mut rt = h.lock();
            let Some(gen) = rt.tui.as_ref().map(|t| t.gen) else {
                return;
            };
            let out = rt.on_hook(&hook, &mut fx);
            if let Some(path) = &out.transcript {
                let path = match &wsl {
                    Some(wsl) if path.starts_with('/') => wsl.windows_path(path),
                    _ => PathBuf::from(path),
                };
                if let Some(t) = rt.tui.as_mut() {
                    t.transcript = Some(path);
                    t.offset = 0;
                }
                rt.tui_since = None;
            }
            let view = fx.agent_changed.then(|| rt.view());
            (
                out,
                rt.meta.project_id.clone(),
                rt.meta.name.clone(),
                view,
                gen,
            )
        };
        self.apply(id, &pid, &name, fx, view);
        if out.stopped {
            // The transcript tells the end of the turn right after the hook; if it still did not
            // after a moment (a version that does not write it), the turn ends here.
            let (c, id) = (self.clone(), id.to_string());
            tauri::async_runtime::spawn(async move {
                for wait in [300, 1500] {
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                    c.read_transcript(&id);
                }
                let _ = c.with_agent(&id, |rt, fx| {
                    if rt.tui.as_ref().is_some_and(|t| t.gen == gen) {
                        rt.end_tui_turn(None, fx);
                    }
                    Ok(())
                });
            });
        }
    }

    /// The agent's interface `gen` ended (`/exit`, closed, killed).
    fn on_tui_exit(self: &Arc<Self>, id: &str, gen: u64, term: &str, code: Option<u32>) {
        self.hub.emit(UiEvent::TerminalExit {
            id: term.to_string(),
            code,
        });
        if self.quitting.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        log::info!("agent {id}: Claude Code's interface exited (code {code:?})");
        if self
            .agent(id)
            .is_ok_and(|h| h.lock().tui.as_ref().is_some_and(|t| t.gen == gen))
        {
            self.read_transcript(id);
            self.hooks.revoke(id);
            let _ = self.with_agent(id, |rt, fx| {
                if rt.tui.as_ref().is_some_and(|t| t.gen == gen) {
                    rt.stop_tui(fx);
                    fx.save = true;
                }
                Ok(())
            });
        }
    }

    /// A message for an agent in the Claude Code view: typed in its interface, as pasted text
    /// then Enter. Refused while it waits for an answer there (the text would go to its dialog).
    pub(crate) fn send_to_tui(&self, id: &str, text: &str) -> Result<()> {
        let h = self.agent(id)?;
        let term = {
            let rt = h.lock();
            if rt.meta.status == AgentStatus::Waiting {
                bail!(tr!(
                    "L’agent attend une réponse dans son terminal : réponds-y d’abord.",
                    "The agent is waiting for an answer in its terminal: answer there first."
                ));
            }
            rt.tui.as_ref().map(|t| t.term_id.clone())
        };
        let Some(term) = term else {
            bail!(tr!(
                "L’agent est en vue Claude Code : ouvre-le pour démarrer son terminal.",
                "The agent is in the Claude Code view: open it to start its terminal."
            ));
        };
        // Line breaks pasted stay in the message (Enter alone would send it).
        let pasted = format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""));
        self.pty.write(&term, pasted.as_bytes())?;
        std::thread::sleep(Duration::from_millis(150));
        self.pty.write(&term, b"\r")
    }

    /// Interrupts the turn of the agent's interface, as Escape does there.
    pub(crate) fn interrupt_tui(&self, id: &str) -> Result<bool> {
        let h = self.agent(id)?;
        let term = h.lock().tui.as_ref().map(|t| t.term_id.clone());
        match term {
            Some(t) => self.pty.write(&t, b"\x1b").map(|()| true),
            None => Ok(false),
        }
    }
}

/// Waits a moment for a stream's process asked to stop to be gone (its session is then free),
/// then kills it.
async fn stop_and_wait(p: &Arc<crate::claude::ClaudeProcess>) {
    for _ in 0..50 {
        if !p.is_alive() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    p.kill();
}

/// Answers the hooks on `listener` until the app quits.
async fn serve<R: Runtime>(listener: TcpListener, core: Weak<Core<R>>, port: u16) {
    let listener = match tokio::net::TcpListener::from_std(listener) {
        Ok(l) => l,
        Err(e) => {
            log::error!("hooks: cannot listen: {e}");
            return;
        }
    };
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        };
        if core.strong_count() == 0 {
            return;
        }
        let core = core.clone();
        tauri::async_runtime::spawn(async move {
            let service = service_fn(move |req| {
                let core = core.clone();
                async move { Ok::<_, Infallible>(answer(req, &core, port).await) }
            });
            let _ = http1::Builder::new()
                .timer(TokioTimer::new())
                .header_read_timeout(Duration::from_secs(10))
                .serve_connection(TokioIo::new(stream), service)
                .await;
        });
    }
}

/// One hook call: from a program of this machine that holds an interface's token (a browser's
/// page gives itself away by its `Origin` or its `Host`). Always answered at once, with nothing
/// decided: the interface goes on as without the hook.
async fn answer<R: Runtime>(
    req: Request<Incoming>,
    core: &Weak<Core<R>>,
    port: u16,
) -> Response<Full<Bytes>> {
    let reply = |status: StatusCode, body: &'static str| {
        let mut r = Response::new(Full::new(Bytes::from_static(body.as_bytes())));
        *r.status_mut() = status;
        r.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        r
    };
    let headers = req.headers();
    let host_ok = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| {
            [format!("127.0.0.1:{port}"), format!("localhost:{port}")]
                .iter()
                .any(|m| m.eq_ignore_ascii_case(h))
        });
    if !host_ok || headers.contains_key(header::ORIGIN) {
        return reply(StatusCode::FORBIDDEN, "{}");
    }
    if req.method() != Method::POST || req.uri().path() != PATH {
        return reply(StatusCode::NOT_FOUND, "{}");
    }
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, t)| t.trim().to_string());
    let Some(core) = core.upgrade() else {
        return reply(StatusCode::SERVICE_UNAVAILABLE, "{}");
    };
    let Some(agent) = token.and_then(|t| core.hooks.agent_of(&t)) else {
        return reply(StatusCode::UNAUTHORIZED, "{}");
    };
    let body = match Limited::new(req.into_body(), MAX_BODY).collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) => return reply(StatusCode::PAYLOAD_TOO_LARGE, "{}"),
    };
    let Ok(hook) = serde_json::from_slice::<Value>(&body) else {
        return reply(StatusCode::BAD_REQUEST, "{}");
    };
    // Off the connection: the interface does not wait for what Escouade makes of it.
    tauri::async_runtime::spawn_blocking(move || core.on_tui_hook(&agent, hook));
    reply(StatusCode::OK, "{}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> AgentMeta {
        AgentMeta {
            model: "sonnet".into(),
            effort: "high".into(),
            mode: "default".into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_new_agent_starts_a_session_of_its_own_and_a_known_one_is_resumed() {
        let f = Path::new("/data/tui/a1.json");
        let a = tui_args(&meta(), Some("s-new"), f);
        assert!(a.contains(&"--session-id=s-new".to_string()));
        assert!(a
            .windows(2)
            .any(|w| w == ["--settings", "/data/tui/a1.json"]));
        assert!(a.windows(2).any(|w| w == ["--permission-mode", "default"]));
        // Nothing of the stream's.
        assert!(!a.iter().any(|x| x.contains("stream-json") || x == "-p"));
        let resumed = AgentMeta {
            session_id: Some("s1".into()),
            ..meta()
        };
        let a = tui_args(&resumed, None, f);
        assert!(a.contains(&"--resume=s1".to_string()));
        assert!(!a.iter().any(|x| x.starts_with("--session-id")));
        // A copy forks its original's into its own.
        let copy = AgentMeta {
            fork_of: Some("s0".into()),
            fork_at: Some("e1".into()),
            ..meta()
        };
        let a = tui_args(&copy, Some("s2"), f);
        for x in [
            "--resume=s0",
            "--fork-session",
            "--resume-session-at=e1",
            "--session-id=s2",
        ] {
            assert!(a.contains(&x.to_string()), "{x} in {a:?}");
        }
    }

    #[test]
    fn the_hooks_call_the_server_with_the_token_of_the_environment() {
        let s = hook_settings(4321);
        let h = &s["hooks"]["PermissionRequest"][0]["hooks"][0];
        assert_eq!(h["type"], "http");
        assert_eq!(h["url"], "http://127.0.0.1:4321/hook");
        assert_eq!(h["headers"]["Authorization"], "Bearer $ESCOUADE_HOOK_TOKEN");
        assert_eq!(h["allowedEnvVars"][0], TOKEN_VAR);
        for e in EVENTS {
            assert!(s["hooks"][e].is_array(), "{e}");
        }
    }

    #[test]
    fn the_transcript_is_read_by_complete_lines() {
        let dir = std::env::temp_dir().join(format!("escouade-tui-read-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        std::fs::write(&path, "{\"a\":1}\n{\"b\":2}\n{\"c\":").unwrap();
        let (lines, off) = read_lines(&path, 0).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(off, 16);
        // The rest of the line comes.
        std::fs::write(&path, "{\"a\":1}\n{\"b\":2}\n{\"c\":3}\n").unwrap();
        let (lines, off) = read_lines(&path, off).unwrap();
        assert_eq!(lines, vec![json!({ "c": 3 })]);
        assert_eq!(off, 24);
        // Shorter than what was read: written again, read from the start.
        std::fs::write(&path, "{\"d\":4}\n").unwrap();
        assert_eq!(read_lines(&path, off).unwrap().0, vec![json!({ "d": 4 })]);
    }

    #[test]
    fn the_transcript_of_a_session_is_found_in_its_accounts_folder() {
        let dir = std::env::temp_dir().join(format!("escouade-tui-find-{}", new_id()));
        let folder = dir.join("projects").join("-work-demo");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("s1.jsonl"), "").unwrap();
        assert_eq!(find_transcript(&dir, "s1"), Some(folder.join("s1.jsonl")));
        assert_eq!(find_transcript(&dir, "s2"), None);
    }

    #[test]
    fn the_loopback_goes_round_the_proxy() {
        let mut env = vec![("NO_PROXY".to_string(), "corp.lan".to_string())];
        no_proxy(&mut env);
        let get = |k: &str| env.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
        assert_eq!(get("NO_PROXY").unwrap(), "corp.lan,127.0.0.1,localhost,::1");
        assert_eq!(get("no_proxy").unwrap(), "127.0.0.1,localhost,::1");
    }

    #[test]
    fn an_entry_of_the_history_is_left_out() {
        let e = json!({ "timestamp": "2026-10-12T11:15:35.144Z" });
        let at = chrono::DateTime::parse_from_rfc3339("2026-10-12T11:15:35.144Z")
            .unwrap()
            .timestamp_millis();
        assert!(before(&e, at + 1));
        assert!(!before(&e, at));
        assert!(!before(&json!({}), at));
    }
}
