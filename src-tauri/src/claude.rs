//! A `claude` process driven over the stream-json protocol (see docs/PROTOCOL.md).

use crate::i18n::{self, Lang};
use crate::job::{Job, JobUsage};
use crate::wsl;
use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>;

pub struct SpawnOpts {
    pub program: Program,
    pub cwd: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub struct ClaudeProcess {
    tx: Mutex<Option<mpsc::UnboundedSender<String>>>,
    pending: Pending,
    next_id: AtomicU64,
    kill_tx: Mutex<Option<oneshot::Sender<()>>>,
    exited: Arc<AtomicBool>,
    job: Option<Arc<Job>>,
}

impl ClaudeProcess {
    /// Spawns the process. `on_frame` receives every stdout frame except responses to our own
    /// control requests; `on_exit` runs once with the exit code and the tail of stderr.
    pub fn spawn<F, E>(opts: SpawnOpts, on_frame: F, on_exit: E) -> Result<Arc<Self>>
    where
        F: Fn(Value) + Send + Sync + 'static,
        E: FnOnce(Option<i32>, String) + Send + 'static,
    {
        let launch = opts.program.launch(&opts.cwd, &opts.args, &opts.env)?;
        let mut cmd = Command::new(&launch.program);
        cmd.args(&launch.args)
            .current_dir(&opts.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (k, v) in &launch.env {
            cmd.env(k, v);
        }
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        crate::job::isolate(&mut cmd);
        crate::proc::no_inherited_fds(&mut cmd);

        let mut child = cmd
            .spawn()
            .with_context(|| cannot_start(i18n::ui(), &opts.program.describe()))?;
        let job = Job::for_child(&child).map(Arc::new);
        if job.is_none() {
            // Killing it would then leave what it started running.
            log::warn!(
                "claude (pid {:?}) is not in a job: its tree is not killed as a whole",
                child.id()
            );
        }
        let mut stdin = child.stdin.take().context("stdin")?;
        let stdout = child.stdout.take().context("stdout")?;
        let stderr = child.stderr.take().context("stderr")?;

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err()
                    || stdin.write_all(b"\n").await.is_err()
                    || stdin.flush().await.is_err()
                {
                    break;
                }
            }
            // Dropping stdin closes the input: the CLI exits once the current turn is over.
        });

        // What the process said besides its frames, for its exit: its stderr, and the lines of its
        // stdout that are no frame (`wsl.exe` writes its own errors there). Lines are read as
        // bytes: one that is not UTF-8 must not stop the reading, or the process would block.
        let stderr_tail = Arc::new(Mutex::new(VecDeque::<String>::new()));
        {
            let tail = stderr_tail.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).split(b'\n');
                while let Ok(Some(line)) = lines.next_segment().await {
                    let line = text_line(&line);
                    log::debug!("claude stderr: {line}");
                    note(&tail, line);
                }
            });
        }
        let stdout_tail = stderr_tail.clone();

        let pending: Pending = Arc::default();
        let (kill_tx, mut kill_rx) = oneshot::channel::<()>();
        let exited = Arc::new(AtomicBool::new(false));
        let proc = Arc::new(Self {
            tx: Mutex::new(Some(tx)),
            pending: pending.clone(),
            next_id: AtomicU64::new(1),
            kill_tx: Mutex::new(Some(kill_tx)),
            exited: exited.clone(),
            job: job.clone(),
        });

        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).split(b'\n');
            loop {
                tokio::select! {
                    line = lines.next_segment() => match line {
                        Ok(Some(line)) => {
                            let line = text_line(&line);
                            if line.trim().is_empty() {
                                continue;
                            }
                            match serde_json::from_str::<Value>(&line) {
                                Ok(frame) => {
                                    if frame["type"] == "control_response" {
                                        resolve(&pending, &frame["response"]);
                                    } else {
                                        on_frame(frame);
                                    }
                                }
                                Err(e) => {
                                    log::warn!("unparsable frame ({e}): {}", truncate(&line, 200));
                                    note(&stdout_tail, truncate(&line, 500));
                                }
                            }
                        }
                        _ => break,
                    },
                    _ = &mut kill_rx => {
                        let _ = child.start_kill();
                        break;
                    }
                }
            }
            let code = child.wait().await.ok().and_then(|s| s.code());
            exited.store(true, Ordering::Release);
            let tail = stderr_tail
                .lock()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            // The exit is handled before pending requests fail, so a caller seeing a failed
            // request also sees the consequences of the exit (status, cleared session…).
            on_exit(code, tail);
            for (_, tx) in pending.lock().drain() {
                let _ = tx.send(Err("process exited".into()));
            }
            // Closing the job ends whatever the process left running.
            drop(job);
        });

        Ok(proc)
    }

    pub fn is_alive(&self) -> bool {
        !self.exited.load(Ordering::Acquire)
    }

    /// What the process and everything it started use.
    pub fn usage(&self) -> Option<JobUsage> {
        self.job.as_ref()?.usage()
    }

    /// How many processes of the tree run: those of its job, and at least one while the process
    /// itself has not been reaped.
    pub fn still_running(&self) -> u32 {
        let in_job = self.usage().map_or(0, |u| u.processes);
        in_job.max(u32::from(self.is_alive()))
    }

    /// The ids of the processes of its job (to say what keeps a folder).
    pub fn pids(&self) -> Vec<u32> {
        self.job.as_ref().map(|j| j.pids()).unwrap_or_default()
    }

    pub fn send(&self, frame: &Value) -> Result<()> {
        if !self.is_alive() {
            return Err(anyhow!("process exited"));
        }
        let guard = self.tx.lock();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("process input closed"))?;
        tx.send(frame.to_string())
            .map_err(|_| anyhow!("process input closed"))
    }

    pub async fn control(&self, request: Value, timeout: Duration) -> Result<Value> {
        let id = format!("ccm_{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(id.clone(), tx);
        if let Err(e) =
            self.send(&json!({ "type": "control_request", "request_id": id, "request": request }))
        {
            self.pending.lock().remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(anyhow!(e)),
            Ok(Err(_)) => Err(anyhow!("control request dropped")),
            Err(_) => {
                self.pending.lock().remove(&id);
                Err(anyhow!("control request timed out"))
            }
        }
    }

    /// Answers a control_request issued by the CLI.
    pub fn respond(&self, request_id: &str, response: Value) -> Result<()> {
        self.send(&json!({
            "type": "control_response",
            "response": { "subtype": "success", "request_id": request_id, "response": response }
        }))
    }

    pub fn respond_error(&self, request_id: &str, error: &str) -> Result<()> {
        self.send(&json!({
            "type": "control_response",
            "response": { "subtype": "error", "request_id": request_id, "error": error }
        }))
    }

    /// Closes stdin: the CLI finishes its current work and exits (session stays resumable).
    pub fn close_input(&self) {
        self.tx.lock().take();
    }

    /// Waits until the process is gone (its output read to the end and the process reaped), at
    /// most `within`; false when it still runs.
    pub async fn wait_exit(&self, within: Duration) -> bool {
        let started = std::time::Instant::now();
        while self.is_alive() {
            if started.elapsed() >= within {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        true
    }

    /// Waits until the process and everything it started are gone, at most `within`; false when
    /// something still runs. Under Windows, a process that lives keeps its folder (and the files in
    /// it) however the process that started it ended, and a killed tree takes a moment to go: the
    /// job must be empty before the folder is removed. (Not elsewhere: a folder can be removed
    /// while a program has it open, and `ps` is too heavy to ask every few milliseconds.)
    pub async fn wait_gone(&self, within: Duration) -> bool {
        let started = std::time::Instant::now();
        if !self.wait_exit(within).await {
            return false;
        }
        #[cfg(windows)]
        while self.usage().is_some_and(|u| u.processes > 0) {
            if started.elapsed() >= within {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        #[cfg(not(windows))]
        let _ = started;
        true
    }

    /// Kills the process and its whole tree.
    pub fn kill(&self) {
        self.tx.lock().take();
        if let Some(job) = &self.job {
            job.terminate();
        }
        if let Some(k) = self.kill_tx.lock().take() {
            let _ = k.send(());
        }
    }
}

fn resolve(pending: &Pending, response: &Value) {
    let Some(id) = response["request_id"].as_str() else {
        return;
    };
    let Some(tx) = pending.lock().remove(id) else {
        return;
    };
    let result = if response["subtype"] == "success" {
        Ok(response
            .get("response")
            .cloned()
            .unwrap_or_else(|| json!({})))
    } else {
        Err(response["error"].as_str().unwrap_or("error").to_string())
    };
    let _ = tx.send(result);
}

/// A line a process wrote, as text: invalid UTF-8 replaced, without its `\r` nor the NULs of a
/// message written in UTF-16.
fn text_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches('\r')
        .replace('\0', "")
}

/// Keeps `line` among the last 30 of `tail`.
fn note(tail: &Mutex<VecDeque<String>>, line: String) {
    let mut t = tail.lock();
    t.push_back(line);
    if t.len() > 30 {
        t.pop_front();
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// The `claude` an account runs: a program of the system, or, under Windows, one installed in WSL
/// and run through `wsl.exe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Program {
    Native(PathBuf),
    Wsl(wsl::Program),
}

/// A process to start: its program, its arguments and the variables it gets on top of the app's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The options whose value is a file Escouade wrote for Claude Code (in its data folder, on
/// Windows): a `claude` of WSL is given its Linux path.
const PATH_OPTIONS: [&str; 2] = ["--mcp-config", "--settings"];

/// Stands for an empty argument on `wsl.exe`'s command line, which it refuses (E_INVALIDARG):
/// `WSL_RUN` puts the empty argument back.
const WSL_EMPTY: &str = "escouade-empty-argument";

/// What `sh` runs in WSL: in the folder `$1` (or not at all: `wsl.exe` would fall back on the home
/// folder, or on `/`, without a word), the program (`$0`) with its arguments, empty ones put back,
/// and its own folder first on the PATH (an npm install under nvm finds its `node` beside it).
const WSL_RUN: &str = r#"p=$0; cd -- "$1" || exit 126; shift; n=$#; while [ "$n" -gt 0 ]; do a=$1; shift; [ "$a" = 'escouade-empty-argument' ] && a=; set -- "$@" "$a"; n=$((n - 1)); done; PATH="${p%/*}:$PATH" exec "$p" "$@""#;

impl Program {
    /// How to start it in the folder `cwd` with `args` and the variables `env` (the caller still
    /// starts the process in `cwd`). In WSL, a variable set on `wsl.exe` reaches Linux only when
    /// `WSLENV` names it, so each one is named there. The files Escouade wrote, `CLAUDE_CONFIG_DIR`
    /// and the folder are given by their Linux paths, and a folder WSL cannot reach is refused.
    /// Without a shell in between (`--exec`), nothing in the arguments is read as shell code.
    /// `WSL_UTF8` makes `wsl.exe` write its own messages in UTF-8 rather than UTF-16.
    pub fn launch(&self, cwd: &str, args: &[String], env: &[(String, String)]) -> Result<Launch> {
        let wsl = match self {
            Program::Native(path) => {
                return Ok(Launch {
                    program: path.clone(),
                    args: args.to_vec(),
                    env: env.to_vec(),
                })
            }
            Program::Wsl(wsl) => wsl,
        };
        let dir = wsl
            .linux_path(cwd)
            .ok_or_else(|| anyhow!(out_of_wsl(i18n::ui(), cwd, &wsl.distro)))?;
        let linux = |value: &str| wsl.linux_path(value).unwrap_or_else(|| value.to_string());
        let mut vars: Vec<(String, String)> = env
            .iter()
            .map(|(k, v)| match k.as_str() {
                crate::accounts::CONFIG_DIR_VAR => (k.clone(), linux(v)),
                _ => (k.clone(), v.clone()),
            })
            .collect();
        let names: Vec<&str> = vars.iter().map(|(k, _)| k.as_str()).collect();
        if let Some(listed) = wslenv(std::env::var("WSLENV").ok().as_deref(), &names) {
            vars.push(("WSLENV".into(), listed));
        }
        vars.push(("WSL_UTF8".into(), "1".into()));
        let mut out: Vec<String> = ["-d", &wsl.distro, "--exec", "/bin/sh", "-c", WSL_RUN]
            .map(String::from)
            .to_vec();
        out.extend([wsl.path.clone(), dir]);
        let mut path_next = false;
        for arg in args {
            let arg = if path_next {
                linux(arg)
            } else {
                match arg.split_once('=') {
                    Some((option, value)) if PATH_OPTIONS.contains(&option) => {
                        format!("{option}={}", linux(value))
                    }
                    _ => arg.clone(),
                }
            };
            path_next = PATH_OPTIONS.contains(&arg.as_str());
            out.push(if arg.is_empty() {
                WSL_EMPTY.into()
            } else {
                arg
            });
        }
        Ok(Launch {
            program: wsl.exe.clone(),
            args: out,
            env: vars,
        })
    }

    /// The path it reads a file Escouade wrote by: its Linux one in WSL.
    pub fn file_path(&self, file: &Path) -> String {
        let path = file.to_string_lossy();
        match self {
            Program::Wsl(wsl) => wsl.linux_path(&path),
            Program::Native(_) => None,
        }
        .unwrap_or_else(|| path.into_owned())
    }

    /// What the user knows it by, in messages: its path, and its distribution for one of WSL.
    pub fn describe(&self) -> String {
        match self {
            Program::Native(path) => path.display().to_string(),
            Program::Wsl(wsl) => format!("{} (WSL {})", wsl.path, wsl.distro),
        }
    }

    /// The program of WSL, for one that runs there.
    pub fn wsl(&self) -> Option<&wsl::Program> {
        match self {
            Program::Wsl(wsl) => Some(wsl),
            Program::Native(_) => None,
        }
    }
}

/// Locates the `claude` executable: explicit setting, PATH, then the native installer location;
/// under Windows, then one installed in WSL (`wsl_distro`: the distribution of the settings,
/// empty for the default one). A setting may start with `~` (the home folder); under Windows it
/// may leave out `.exe` or `.cmd`, and name a program of WSL: `\\wsl.localhost\<distro>\…` (or
/// `\\wsl$\…`), a Linux path, or `~/…` that names no program of Windows.
pub fn resolve_binary(configured: &str, wsl_distro: &str) -> Option<Program> {
    let configured = configured.trim();
    if !configured.is_empty() {
        return resolve_in(configured, dirs::home_dir().as_deref(), |spec| {
            wsl::find(spec, wsl_distro)
        });
    }
    // Not from the tests: a developer's WSL would answer for a machine that has no claude.
    detect_native().map(Program::Native).or_else(|| {
        if cfg!(all(windows, not(test))) {
            wsl::detect(wsl_distro).map(Program::Wsl)
        } else {
            None
        }
    })
}

/// Where the `claude` of a setting is, as the line under its field tells it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Located {
    /// Its path (its Linux path for one of WSL); None when it is not found.
    pub path: Option<String>,
    /// Its distribution, for one of WSL.
    pub wsl: Option<String>,
    /// For one of WSL: WSL does not share the network of Windows (`wsl::mirrored`), so Escouade's
    /// MCP server and the hooks of the Claude Code view, on 127.0.0.1 of Windows, are out of its
    /// reach.
    pub nat: bool,
    /// A `claude` of WSL may be named (under Windows).
    pub can_wsl: bool,
}

/// `resolve_binary`, told as `Located`.
pub fn locate(configured: &str, wsl_distro: &str) -> Located {
    let located = match resolve_binary(configured, wsl_distro) {
        Some(Program::Native(path)) => Located {
            path: Some(path.display().to_string()),
            ..Default::default()
        },
        Some(Program::Wsl(wsl)) => Located {
            path: Some(wsl.path),
            wsl: Some(wsl.distro),
            nat: !wsl::mirrored_here(),
            ..Default::default()
        },
        None => Located::default(),
    };
    Located {
        can_wsl: cfg!(windows),
        ..located
    }
}

/// `resolve_binary` of a setting, `home` the home folder and `find_wsl` the look into WSL.
fn resolve_in(
    configured: &str,
    home: Option<&Path>,
    find_wsl: impl Fn(&wsl::Spec) -> Option<wsl::Program>,
) -> Option<Program> {
    let spec = if cfg!(windows) {
        wsl::parse(configured)
    } else {
        None
    };
    // The share is a program of Linux, which Windows can read but not start.
    if let Some(spec) = spec.as_ref().filter(|s| s.distro.is_some()) {
        return find_wsl(spec).map(Program::Wsl);
    }
    native(configured, home)
        .map(Program::Native)
        .or_else(|| find_wsl(spec.as_ref()?).map(Program::Wsl))
}

/// The program of the system a setting names: `~` for the home folder and, under Windows, its
/// extension left out.
fn native(configured: &str, home: Option<&Path>) -> Option<PathBuf> {
    let path = match configured.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            // One separator throughout: the path shows under the field of the settings.
            let rest = rest.trim_start_matches(['/', '\\']);
            home?.join(rest.replace(['/', '\\'], std::path::MAIN_SEPARATOR_STR))
        }
        _ => PathBuf::from(configured),
    };
    if path.exists() && !path.is_dir() {
        return Some(path);
    }
    if !cfg!(windows) || path.extension().is_some() {
        return None;
    }
    ["exe", "cmd"]
        .iter()
        .map(|ext| path.with_extension(ext))
        .find(|p| p.is_file())
}

/// `claude` where it usually is on this system: the PATH, then the installers' locations.
fn detect_native() -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) {
        &["exe", "cmd"]
    } else {
        &[""]
    };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for ext in exts {
                let mut candidate = dir.join("claude");
                if !ext.is_empty() {
                    candidate.set_extension(ext);
                }
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    let home = dirs::home_dir()?;
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        vec![
            home.join(".local").join("bin").join("claude.exe"),
            home.join(".claude").join("local").join("claude.exe"),
            home.join("AppData")
                .join("Roaming")
                .join("npm")
                .join("claude.cmd"),
        ]
    } else {
        // The native installer, the old local install, Homebrew (Apple Silicon, then Intel).
        vec![
            home.join(".local").join("bin").join("claude"),
            home.join(".claude").join("local").join("claude"),
            PathBuf::from("/opt/homebrew/bin/claude"),
            PathBuf::from("/usr/local/bin/claude"),
        ]
    };
    candidates.into_iter().find(|p| p.is_file())
}

/// `WSLENV` naming the variables `names` for Linux after those the app's (`existing`) names; None
/// when there is none to name.
fn wslenv(existing: Option<&str>, names: &[&str]) -> Option<String> {
    let all: Vec<&str> = existing
        .into_iter()
        .flat_map(|e| e.split(':'))
        .chain(names.iter().copied())
        .filter(|n| !n.is_empty())
        .collect();
    (!all.is_empty()).then(|| all.join(":"))
}

/// Refused, in `lang`: the folder `cwd` has no path in WSL (a network share, another
/// distribution's folder), where the `claude` of `distro` would run in another one.
fn out_of_wsl(lang: Lang, cwd: &str, distro: &str) -> String {
    tr_in!(
        lang,
        "Le dossier {cwd} n’est pas joignable depuis WSL ({distro}) : le Claude Code qui y est installé ne peut pas y travailler.",
        "The folder {cwd} can’t be reached from WSL ({distro}): the Claude Code installed there can’t work in it."
    )
}

/// The program of an agent could not be started (not there, not allowed to run).
fn cannot_start(lang: Lang, program: &str) -> String {
    tr_in!(
        lang,
        "impossible de lancer {program}",
        "can’t start {program}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_that_does_not_start_is_named_in_english() {
        assert_eq!(
            cannot_start(crate::i18n::Lang::En, "C:/bin/claude.exe"),
            "can’t start C:/bin/claude.exe"
        );
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn ubuntu_claude() -> wsl::Program {
        wsl::Program {
            exe: PathBuf::from(r"C:\Windows\System32\wsl.exe"),
            distro: "Ubuntu-24.04".into(),
            path: "/home/me/.local/bin/claude".into(),
            home: "/home/me".into(),
            mount_root: "/mnt/".into(),
        }
    }

    #[test]
    fn a_file_escouade_wrote_is_given_to_a_claude_of_wsl_by_its_linux_path() {
        let file = Path::new(r"C:\Users\Jean Dupont\.escouade\tui\pastes\4f2a.png");
        assert_eq!(
            Program::Wsl(ubuntu_claude()).file_path(file),
            "/mnt/c/Users/Jean Dupont/.escouade/tui/pastes/4f2a.png"
        );
        assert_eq!(
            Program::Native(PathBuf::from(r"C:\bin\claude.exe")).file_path(file),
            r"C:\Users\Jean Dupont\.escouade\tui\pastes\4f2a.png"
        );
    }

    #[test]
    fn a_claude_of_wsl_runs_through_wsl_exe_its_files_and_variables_in_linux() {
        let launch = Program::Wsl(ubuntu_claude())
            .launch(
                r"C:\Users\me\proj",
                &strings(&[
                    "-p",
                    "--mcp-config",
                    r"C:\Users\me\.escouade\mcp\a1.json",
                    r"--settings=C:\Users\me\.escouade\tui\a1.json",
                    "--tools",
                    "",
                    "--system-prompt",
                    "l'été, $HOME et `ls` restent du texte\n",
                    "--settings",
                    r#"{"hooks":{}}"#,
                ]),
                &[
                    (
                        "CLAUDE_CONFIG_DIR".into(),
                        r"C:\Users\me\.escouade\claude\pro".into(),
                    ),
                    ("HTTPS_PROXY".into(), "http://proxy:3128".into()),
                ],
            )
            .unwrap();
        assert_eq!(
            launch.program,
            PathBuf::from(r"C:\Windows\System32\wsl.exe")
        );
        assert_eq!(
            launch.args,
            strings(&[
                "-d",
                "Ubuntu-24.04",
                "--exec",
                "/bin/sh",
                "-c",
                WSL_RUN,
                "/home/me/.local/bin/claude",
                // The folder it works in, which the script goes into or refuses.
                "/mnt/c/Users/me/proj",
                "-p",
                "--mcp-config",
                "/mnt/c/Users/me/.escouade/mcp/a1.json",
                "--settings=/mnt/c/Users/me/.escouade/tui/a1.json",
                "--tools",
                WSL_EMPTY,
                "--system-prompt",
                "l'été, $HOME et `ls` restent du texte\n",
                // Settings given inline are no file.
                "--settings",
                r#"{"hooks":{}}"#,
            ])
        );
        // The variables reach Linux by WSLENV, not on the command line (a token, a proxy's
        // password), and wsl.exe says what it has to say in UTF-8.
        let names = ["CLAUDE_CONFIG_DIR", "HTTPS_PROXY"];
        assert_eq!(
            launch.env,
            vec![
                (
                    "CLAUDE_CONFIG_DIR".to_string(),
                    "/mnt/c/Users/me/.escouade/claude/pro".to_string()
                ),
                ("HTTPS_PROXY".into(), "http://proxy:3128".into()),
                (
                    "WSLENV".into(),
                    wslenv(std::env::var("WSLENV").ok().as_deref(), &names).unwrap()
                ),
                ("WSL_UTF8".into(), "1".into()),
            ]
        );
        assert!(WSL_RUN.contains(&format!("'{WSL_EMPTY}'")));
    }

    #[test]
    fn a_folder_wsl_cannot_reach_is_refused_rather_than_another_used() {
        let refused = Program::Wsl(ubuntu_claude())
            .launch(r"\\server\share\proj", &[], &[])
            .unwrap_err();
        assert!(
            refused.to_string().contains(r"\\server\share\proj"),
            "{refused}"
        );
        assert_eq!(
            out_of_wsl(Lang::En, r"\\server\share\proj", "Ubuntu"),
            r"The folder \\server\share\proj can’t be reached from WSL (Ubuntu): the Claude Code installed there can’t work in it."
        );
    }

    #[test]
    fn wslenv_names_the_variables_after_those_the_app_names() {
        assert_eq!(wslenv(None, &["A", "B"]).as_deref(), Some("A:B"));
        assert_eq!(
            wslenv(Some("USERPROFILE/p:"), &["A"]).as_deref(),
            Some("USERPROFILE/p:A")
        );
        assert_eq!(wslenv(Some(""), &[]), None);
    }

    #[test]
    fn a_program_of_the_system_starts_as_it_is() {
        let path = PathBuf::from(r"C:\tools\claude.exe");
        let args = strings(&["--tools", "", "--mcp-config", r"C:\x.json"]);
        let env = vec![("CLAUDE_CONFIG_DIR".to_string(), r"C:\cfg".to_string())];
        assert_eq!(
            Program::Native(path.clone())
                .launch(r"\\server\share", &args, &env)
                .unwrap(),
            Launch {
                program: path,
                args,
                env
            }
        );
    }

    #[test]
    fn reads_the_settings_forms_of_the_issue() {
        let dir = crate::paths::test_dir("claude-resolve");
        let bin = dir.join(".local").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let exe = if cfg!(windows) {
            "claude.exe"
        } else {
            "claude"
        };
        std::fs::write(bin.join(exe), "").unwrap();
        let in_wsl = |spec: &wsl::Spec| {
            (spec.path == "~/.local/bin/claude-linux"
                || spec.path == "/home/me/.local/bin/claude"
                || spec.distro.as_deref() == Some("Ubuntu-24.04"))
            .then(ubuntu_claude)
        };
        let resolve = |configured: &str| resolve_in(configured, Some(&dir), in_wsl);
        // `~` is the home folder (also outside Windows); a folder is no program.
        assert_eq!(resolve("~/.local/bin"), None);
        assert_eq!(
            resolve(&format!("~/.local/bin/{exe}")),
            Some(Program::Native(bin.join(exe)))
        );
        if cfg!(windows) {
            // Under Windows, `.exe` may be left out: the native installer's `claude.exe`.
            assert_eq!(
                resolve(r"~\.local\bin\claude"),
                Some(Program::Native(bin.join("claude.exe")))
            );
            assert_eq!(
                resolve(&bin.join("claude").to_string_lossy()),
                Some(Program::Native(bin.join("claude.exe")))
            );
            // Shown with one separator throughout, however it was typed.
            let Some(Program::Native(shown)) = resolve("~/.local/bin/claude.exe") else {
                panic!("not found");
            };
            assert!(!shown.to_string_lossy().contains('/'), "{shown:?}");
            // The three forms of the issue name a program of WSL.
            let wsl = Some(Program::Wsl(ubuntu_claude()));
            assert_eq!(
                resolve(r"\\wsl.localhost\Ubuntu-24.04\home\me\.local\bin\claude"),
                wsl
            );
            assert_eq!(resolve("~/.local/bin/claude-linux"), wsl);
            assert_eq!(resolve("/home/me/.local/bin/claude"), wsl);
            assert_eq!(resolve(r"\\wsl.localhost\Debian\usr\bin\claude"), None);
        } else {
            assert_eq!(resolve("~/.local/bin/claude-linux"), None);
        }
        assert_eq!(resolve(&dir.join("absent").to_string_lossy()), None);
    }

    #[test]
    fn tells_where_the_claude_of_a_setting_is() {
        let dir = crate::paths::test_dir("claude-locate");
        let exe = dir.join(if cfg!(windows) {
            "claude.cmd"
        } else {
            "claude"
        });
        std::fs::write(&exe, "").unwrap();
        assert_eq!(
            locate(&exe.to_string_lossy(), ""),
            Located {
                path: Some(exe.display().to_string()),
                wsl: None,
                nat: false,
                can_wsl: cfg!(windows)
            }
        );
        assert_eq!(
            locate(&dir.join("absent").to_string_lossy(), ""),
            Located {
                can_wsl: cfg!(windows),
                ..Default::default()
            }
        );
        // One of WSL, where WSL has a distribution: any program of it will do.
        let machine = wsl::Machine::this();
        if let (Some(_), Some(distro)) = (machine.exe, machine.default) {
            assert_eq!(
                locate("/usr/bin/env", ""),
                Located {
                    path: Some("/usr/bin/env".into()),
                    wsl: Some(distro),
                    nat: !wsl::mirrored_here(),
                    can_wsl: true
                }
            );
        }
    }

    #[test]
    fn a_line_is_read_whatever_its_bytes() {
        assert_eq!(text_line(b"{\"type\":\"x\"}\r"), "{\"type\":\"x\"}");
        // Not UTF-8 (a message of wsl.exe in UTF-16: « détectée »): read all the same.
        assert_eq!(text_line(b"d\0\xe9\0t\0"), "d\u{FFFD}t");
        let tail = Mutex::new(VecDeque::new());
        for n in 0..35 {
            note(&tail, n.to_string());
        }
        assert_eq!(tail.lock().len(), 30);
        assert_eq!(tail.lock().front().map(String::as_str), Some("5"));
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("héllo", 2), "h…");
        assert_eq!(truncate("abc", 5), "abc");
    }
}
