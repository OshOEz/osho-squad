//! Integrated terminals: real pseudo-consoles (ConPTY on Windows, a pty on macOS) streamed to
//! xterm.js.

use crate::i18n::{self, Lang};
use crate::job::Job;
use crate::model::{now_ms, Settings};
use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellInfo {
    pub id: String,
    pub label: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermInfo {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub shell: String,
}

struct Term {
    info: TermInfo,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// The terminal's whole process tree, including what it starts outside its console: killed
    /// when the terminal goes (all of it, or what shows no window: see `Started::spare_windows`).
    job: Option<Job>,
    /// A launch command (or a step of a test launch), not an interactive shell.
    launch: bool,
    /// See `Started::spare_windows`.
    spare_windows: bool,
    /// See `Started::detached_survive`.
    detached_survive: bool,
    /// When it last had input or output (ms).
    io_at: Arc<AtomicI64>,
}

/// What runs in the terminals, as an automatic restart for an update goes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Activity {
    /// Launch commands (and steps of test launches) still running.
    pub launches: usize,
    /// When an interactive terminal last had input or output (ms), if one is open.
    pub shell_io_at: Option<i64>,
}

#[derive(Default, Clone)]
pub struct PtyManager {
    terms: Arc<Mutex<HashMap<String, Term>>>,
}

/// What a terminal starts.
#[derive(Clone, Copy)]
struct Started {
    /// A launch command (or a step of a test launch), not an interactive terminal.
    launch: bool,
    /// What it starts, outside its console too, is killed with it (a `Job`).
    job: bool,
    /// What shows a window (a browser, an editor), with what it started, is not killed with it:
    /// closing the terminal (or, for `detached_survive` false, the end of its program) ends only
    /// the programs that show none, and the Job kills nothing by itself when it is dropped (the
    /// app's quit, crash or restart for an update). Otherwise it kills everything.
    spare_windows: bool,
    /// When its program ends by itself (an `exit`), what the user detached keeps running: only
    /// closing the terminal ends the tree (but for the windows, see `spare_windows`). Otherwise
    /// the end of the program ends what it started.
    detached_survive: bool,
}

/// Where shells are looked for; from the environment in the app, fake folders in tests.
#[cfg(windows)]
pub struct ShellRoots {
    pub path: Vec<PathBuf>,
    pub program_files: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub system_root: Option<PathBuf>,
}

#[cfg(windows)]
impl ShellRoots {
    fn from_env() -> Self {
        let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
        ShellRoots {
            path: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            program_files: var("ProgramFiles").or_else(|| Some(PathBuf::from(r"C:\Program Files"))),
            local_app_data: var("LOCALAPPDATA"),
            system_root: var("SystemRoot").or_else(|| Some(PathBuf::from(r"C:\Windows"))),
        }
    }

    fn on_path(&self, exe: &str) -> Option<PathBuf> {
        self.path.iter().map(|d| d.join(exe)).find(|p| installed(p))
    }
}

/// Also true for the 0-byte "app execution alias" reparse points of Microsoft Store apps.
#[cfg(windows)]
fn installed(p: &Path) -> bool {
    p.is_file()
}

#[cfg(windows)]
fn first_installed(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|p| installed(p))
}

#[cfg(windows)]
pub fn detect_shells(s: &Settings) -> Vec<ShellInfo> {
    detect_shells_in(&ShellRoots::from_env(), s)
}

/// The user's login shell first (zsh by default on macOS), then the other usual ones.
#[cfg(not(windows))]
pub fn detect_shells(s: &Settings) -> Vec<ShellInfo> {
    let path: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let login = std::env::var_os("SHELL").map(PathBuf::from);
    detect_unix_shells(login.as_deref(), &path, s)
}

/// Unix shells: `login` (from $SHELL) first, then zsh, bash and fish, from /bin or the PATH.
#[cfg(not(windows))]
pub fn detect_unix_shells(login: Option<&Path>, path: &[PathBuf], s: &Settings) -> Vec<ShellInfo> {
    const KNOWN: [(&str, &str); 4] = [
        ("zsh", "zsh"),
        ("bash", "bash"),
        ("fish", "fish"),
        ("sh", "sh"),
    ];
    let find = |name: &str| -> Option<PathBuf> {
        if name == "bash" && !s.bash_path.is_empty() {
            return Some(PathBuf::from(&s.bash_path)).filter(|p| p.is_file());
        }
        [Path::new("/bin"), Path::new("/usr/bin")]
            .into_iter()
            .chain(path.iter().map(PathBuf::as_path))
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    };
    let mut found: Vec<(&str, &str, PathBuf)> = Vec::new();
    if let Some(l) = login.filter(|l| l.is_file()) {
        let name = l.file_name().unwrap_or_default().to_string_lossy();
        if let Some((id, label)) = KNOWN.iter().find(|(id, _)| *id == name) {
            found.push((id, label, l.to_path_buf()));
        }
    }
    for (id, label) in KNOWN.iter().take(3) {
        if !found.iter().any(|f| f.0 == *id) {
            if let Some(p) = find(id) {
                found.push((id, label, p));
            }
        }
    }
    if found.is_empty() {
        if let Some(p) = find("sh") {
            found.push(("sh", "sh", p));
        }
    }
    found
        .into_iter()
        .map(|(id, label, p)| ShellInfo {
            id: id.into(),
            label: label.into(),
            path: p.to_string_lossy().into(),
        })
        .collect()
}

#[cfg(windows)]
pub fn detect_shells_in(r: &ShellRoots, s: &Settings) -> Vec<ShellInfo> {
    let mut out = Vec::new();
    let shell = |id: &str, label: &str, p: PathBuf| ShellInfo {
        id: id.into(),
        label: label.into(),
        path: p.to_string_lossy().into(),
    };
    // PowerShell 7: PATH, Program Files (also when not on the PATH), then the Store's alias.
    let pwsh = if !s.pwsh_path.is_empty() {
        Some(PathBuf::from(&s.pwsh_path)).filter(|p| installed(p))
    } else {
        r.on_path("pwsh.exe").or_else(|| {
            let pf = r.program_files.iter().flat_map(|pf| {
                ["7", "7-preview"].map(|v| pf.join("PowerShell").join(v).join("pwsh.exe"))
            });
            let store = r
                .local_app_data
                .iter()
                .map(|d| d.join(r"Microsoft\WindowsApps\pwsh.exe"));
            first_installed(pf.chain(store))
        })
    };
    match pwsh {
        Some(p) => out.push(shell("pwsh", "PowerShell", p)),
        // Windows PowerShell 5.1 ships with every Windows: the fallback when 7 is missing.
        None => {
            let builtin = r
                .system_root
                .iter()
                .map(|w| w.join(r"System32\WindowsPowerShell\v1.0\powershell.exe"));
            if let Some(p) = first_installed(builtin) {
                out.push(shell("powershell", "Windows PowerShell", p));
            }
        }
    }
    let bash = if !s.bash_path.is_empty() {
        Some(PathBuf::from(&s.bash_path)).filter(|p| installed(p))
    } else {
        let from_git = r.on_path("git.exe").and_then(|g| {
            let root = g.parent()?.parent()?.to_path_buf();
            first_installed([
                root.join("bin").join("bash.exe"),
                root.join("usr").join("bin").join("bash.exe"),
            ])
        });
        from_git.or_else(|| {
            first_installed(
                r.program_files
                    .iter()
                    .map(|pf| pf.join(r"Git\bin\bash.exe")),
            )
        })
    };
    if let Some(p) = bash {
        out.push(shell("bash", "Git Bash", p));
    }
    if let Some(wsl) = first_installed(r.system_root.iter().map(|w| w.join(r"System32\wsl.exe"))) {
        let label = if s.wsl_distro.is_empty() {
            "WSL".to_string()
        } else {
            format!("WSL ({})", s.wsl_distro)
        };
        out.push(shell("wsl", &label, wsl));
    }
    out
}

/// PowerShell script running `command` then ending with its exit code: on its own, PowerShell
/// ends with 1 for any failure, whatever the failing program's code.
pub(crate) fn with_exit_code(command: &str) -> String {
    format!("{command}\nif (-not $?) {{ if ($LASTEXITCODE) {{ exit $LASTEXITCODE }} exit 1 }}")
}

/// The arguments making zsh run `command` as the user's terminal would: a login shell, interactive
/// too so that it reads ~/.zshrc, where rbenv, nvm, asdf… set themselves up (a login shell alone
/// runs the system's Ruby or Node: macOS's /etc/zprofile puts the system folders first in the
/// PATH). Without job control (`+m`), which would give what it starts a process group of its own,
/// out of reach of the Job stopping it.
pub(crate) fn zsh_command_args(command: &str) -> [&str; 5] {
    ["-l", "-i", "+m", "-c", command]
}

/// Refused: the folder `sub` a launch command names is not in the project.
fn missing_folder(lang: Lang, sub: &str) -> String {
    tr_in!(
        lang,
        "Le dossier « {sub} » n'existe pas dans le projet",
        "The folder “{sub}” doesn’t exist in the project"
    )
}

/// Refused: the terminal written to or resized is gone (its shell ended, the tab closed).
fn closed(lang: Lang) -> anyhow::Error {
    anyhow!(tr_in!(lang, "terminal fermé", "terminal closed"))
}

/// Working folder of a launch command: the project's, or one of its folders.
pub fn run_cwd(project: &str, sub: &str) -> Result<String> {
    let sub = sub.trim();
    if sub.is_empty() {
        return Ok(project.to_string());
    }
    let dir = Path::new(project).join(sub);
    if !dir.is_dir() {
        anyhow::bail!(missing_folder(i18n::ui(), sub));
    }
    Ok(dir.to_string_lossy().into_owned())
}

/// What tells Escouade's browser opener the user's own `BROWSER` (empty: none).
#[cfg(unix)]
pub const USER_BROWSER_VAR: &str = "ESCOUADE_BROWSER";

/// Escouade's browser opener (`open_browser_apart`): the user's browser, else the system's
/// opener, in a session of its own (macOS has no `setsid`: Perl's). Without the user's, `BROWSER`
/// goes: `xdg-open` would run the opener again.
#[cfg(unix)]
const BROWSER_OPENER: &str = r#"#!/bin/sh
# Escouade: opens the page Claude Code asks for in a session of its own, so that the browser it
# starts outlives the terminal of the sign-in, whose end takes what runs in it.
browser=$ESCOUADE_BROWSER
unset ESCOUADE_BROWSER
if [ -n "$browser" ]; then
  BROWSER=$browser
  export BROWSER
  set -- "$browser" "$@"
else
  unset BROWSER
  if [ "$(uname)" = Darwin ]; then set -- open "$@"; else set -- xdg-open "$@"; fi
fi
if command -v setsid >/dev/null 2>&1; then
  exec setsid "$@"
fi
if command -v perl >/dev/null 2>&1; then
  exec perl -MPOSIX -e 'POSIX::setsid(); exec { $ARGV[0] } @ARGV or exit 127' -- "$@"
fi
exec "$@"
"#;

/// Has a program of a terminal (Claude Code signing in) open its browser through Escouade's
/// opener, written at `opener`, out of the terminal. On Unix, the end of the program hangs up its
/// terminal's session, whose processes die of it (SIGHUP), and its process group is killed: a
/// browser started there (by `xdg-open`, when none runs yet) would go with them, and nothing tells
/// a process with a window from one without. The opener runs the user's own `BROWSER`, kept in
/// `env` for it.
#[cfg(unix)]
pub fn open_browser_apart(opener: &Path, env: &mut Vec<(String, String)>) -> std::io::Result<()> {
    write_executable(opener, BROWSER_OPENER)?;
    let opener = opener.to_string_lossy().into_owned();
    let var = |k: &str| {
        env.iter()
            .rev()
            .find(|(name, _)| name == k)
            .map(|(_, v)| v.clone())
            .or_else(|| std::env::var(k).ok())
    };
    // Already the opener (Escouade started from a sign-in's terminal): the user's is the one it
    // was given.
    let user = match var("BROWSER") {
        Some(b) if b == opener => var(USER_BROWSER_VAR),
        b => b,
    };
    env.retain(|(k, _)| k != "BROWSER" && k != USER_BROWSER_VAR);
    env.push(("BROWSER".into(), opener));
    env.push((USER_BROWSER_VAR.into(), user.unwrap_or_default()));
    Ok(())
}

/// On Windows, nothing to do: the Job of the terminal spares what shows a window.
#[cfg(not(unix))]
pub fn open_browser_apart(_opener: &Path, _env: &mut Vec<(String, String)>) -> std::io::Result<()> {
    Ok(())
}

/// Writes `text` at `path`, executable by its owner alone, whole or not at all: a program may run
/// it at any time.
#[cfg(unix)]
fn write_executable(path: &Path, text: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}-{n}.tmp", std::process::id()));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&tmp)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// Finds the cursor position query (`ESC [ 6 n`) in what a terminal prints, even when it is cut
/// between two reads: the end of each read is kept to join it to the next.
#[derive(Default)]
struct CursorQuery {
    tail: Vec<u8>,
}

impl CursorQuery {
    const QUERY: &'static [u8] = b"\x1b[6n";

    /// Whether the query ends in `read`, the next one of the terminal's output.
    fn feed(&mut self, read: &[u8]) -> bool {
        let mut seen = std::mem::take(&mut self.tail);
        seen.extend_from_slice(read);
        let found = seen.windows(Self::QUERY.len()).any(|w| w == Self::QUERY);
        // One byte short of the query: a whole one never waits for the next read, so it is not
        // found twice.
        let keep = seen.len().min(Self::QUERY.len() - 1);
        self.tail = seen.split_off(seen.len() - keep);
        found
    }
}

impl PtyManager {
    /// An interactive shell.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &self,
        info: TermInfo,
        shell: &ShellInfo,
        wsl_distro: &str,
        cwd: &str,
        size: (u16, u16),
        env: Vec<(String, String)>,
        on_data: impl Fn(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Option<u32>) + Send + 'static,
    ) -> Result<()> {
        self.spawn_inner(
            info, shell, wsl_distro, cwd, size, env, None, on_data, on_exit,
        )
    }

    /// A launch command run by `shell`: the terminal ends with it and reports its exit code.
    /// Its log is read-only, so the terminal answers the pseudo-console's cursor position query
    /// itself (ConPTY waits for it before running anything), with `cursor_row`, the log's row
    /// where the command starts: ConPTY then leaves the lines above alone, even when it repaints.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_command(
        &self,
        info: TermInfo,
        shell: &ShellInfo,
        wsl_distro: &str,
        cwd: &str,
        size: (u16, u16),
        env: Vec<(String, String)>,
        cursor_row: u16,
        command: &str,
        on_data: impl Fn(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Option<u32>) + Send + 'static,
    ) -> Result<()> {
        let (me, id) = (self.clone(), info.id.clone());
        let answer = format!("\x1b[{};1R", cursor_row.max(1));
        let query = Mutex::new(CursorQuery::default());
        let on_data = move |b: Vec<u8>| {
            if query.lock().feed(&b) {
                let _ = me.write(&id, answer.as_bytes());
            }
            on_data(b);
        };
        self.spawn_inner(
            info,
            shell,
            wsl_distro,
            cwd,
            size,
            env,
            Some(command),
            on_data,
            on_exit,
        )
    }

    /// An interactive terminal running `program` itself, with no shell around it (the sign-in of
    /// a Claude account, the Claude Code view): it ends with the program, and what it starts goes
    /// with it; except, with `spare_windows`, what shows a window (the browser of the sign-in). On
    /// Unix, where nothing tells it, such a program starts its browser out of the terminal
    /// (`open_browser_apart`).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_program(
        &self,
        info: TermInfo,
        program: &crate::claude::Program,
        args: &[String],
        cwd: &str,
        size: (u16, u16),
        env: Vec<(String, String)>,
        spare_windows: bool,
        on_data: impl Fn(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Option<u32>) + Send + 'static,
    ) -> Result<()> {
        // The terminal's variables as well: a program of WSL gets none of those set on wsl.exe.
        let terminal = [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")]
            .map(|(k, v)| (k.to_string(), v.to_string()));
        let launch = program.launch(cwd, args, &[terminal.to_vec(), env].concat())?;
        // A `.cmd` launcher (npm's claude.cmd) included: Windows runs it through cmd.exe.
        let mut cmd = CommandBuilder::new(&launch.program);
        cmd.args(&launch.args);
        let env = launch.env;
        if Path::new(cwd).is_dir() {
            cmd.cwd(cwd);
        }
        let started = Started {
            launch: false,
            job: true,
            spare_windows,
            detached_survive: false,
        };
        self.start(info, cmd, size, env, started, on_data, on_exit)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_inner(
        &self,
        info: TermInfo,
        shell: &ShellInfo,
        wsl_distro: &str,
        cwd: &str,
        size: (u16, u16),
        env: Vec<(String, String)>,
        command: Option<&str>,
        on_data: impl Fn(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Option<u32>) + Send + 'static,
    ) -> Result<()> {
        let mut cmd = CommandBuilder::new(&shell.path);
        match (shell.id.as_str(), command) {
            ("pwsh" | "powershell", None) => cmd.arg("-NoLogo"),
            ("pwsh" | "powershell", Some(c)) => {
                cmd.args(["-NoLogo", "-Command", &with_exit_code(c)])
            }
            ("bash", run) => {
                match run {
                    None => cmd.args(["--login", "-i"]),
                    Some(c) => cmd.args(["--login", "-c", c]),
                }
                // Git Bash stays in the working directory instead of going home.
                cmd.env("CHERE_INVOKING", "1");
            }
            ("zsh" | "fish" | "sh", None) => cmd.arg("-l"),
            ("zsh", Some(c)) => {
                cmd.args(zsh_command_args(c));
                // Oh My Zsh would ask whether to update itself in a log that takes no input
                // (unless its update mode is set by zstyle).
                cmd.env("DISABLE_AUTO_UPDATE", "true");
            }
            ("fish" | "sh", Some(c)) => cmd.args(["-l", "-c", c]),
            ("wsl", run) => {
                if !wsl_distro.is_empty() {
                    cmd.args(["-d", wsl_distro]);
                }
                cmd.args(["--cd", cwd]);
                if let Some(c) = run {
                    cmd.args(["--", "bash", "-lc", c]);
                }
            }
            _ => {}
        }
        if Path::new(cwd).is_dir() {
            cmd.cwd(cwd);
        }
        let started = Started {
            launch: command.is_some(),
            // Not on Unix, where the job is the shell's process group, killed when dropped: an
            // `exit` would take what the user detached (nohup…) with it.
            job: command.is_some() || cfg!(windows),
            // Closing an interactive terminal leaves a browser or an editor its user started
            // open; a launch command stops with everything it started.
            spare_windows: command.is_none(),
            detached_survive: command.is_none(),
        };
        self.start(info, cmd, size, env, started, on_data, on_exit)
    }

    /// Starts `cmd` in a pseudo-console of `size`, with `env` added, streaming its output to
    /// `on_data` until it ends (`on_exit`, with its exit code).
    #[allow(clippy::too_many_arguments)]
    fn start(
        &self,
        info: TermInfo,
        mut cmd: CommandBuilder,
        size: (u16, u16),
        env: Vec<(String, String)>,
        started: Started,
        on_data: impl Fn(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Option<u32>) + Send + 'static,
    ) -> Result<()> {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: size.1.max(2),
                cols: size.0.max(10),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| anyhow!("{e}"))?;
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (k, v) in env {
            cmd.env(k, v);
        }

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| anyhow!("{e}"))
            .with_context(|| tr!("lancement du shell", "starting the shell"))?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let job = started.job.then(|| Job::for_pty(child.as_ref())).flatten();
        if let (Some(job), true) = (&job, started.spare_windows) {
            job.release();
        }
        let mut reader = pair.master.try_clone_reader().map_err(|e| anyhow!("{e}"))?;
        let writer = pair.master.take_writer().map_err(|e| anyhow!("{e}"))?;
        let id = info.id.clone();
        let io_at = Arc::new(AtomicI64::new(now_ms()));
        self.terms.lock().insert(
            id.clone(),
            Term {
                info,
                master: pair.master,
                writer,
                killer,
                job,
                launch: started.launch,
                spare_windows: started.spare_windows,
                detached_survive: started.detached_survive,
                io_at: io_at.clone(),
            },
        );

        std::thread::spawn(move || {
            let mut buf = vec![0u8; 32 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        io_at.store(now_ms(), Ordering::Release);
                        on_data(buf[..n].to_vec())
                    }
                }
            }
        });

        let terms = self.terms.clone();
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code());
            // Dropping the master closes the pseudo-console, which ends the reader thread (and the
            // job, what the command left running). Outside the lock: see kill. A term that is not
            // there was closed by `kill`, which ended the tree.
            let term = terms.lock().remove(&id);
            // A shell that ends by itself leaves what its user detached; a program that ends
            // takes what it started, but what shows a window (the Job, released, kills nothing).
            if let Some(Term {
                job: Some(job),
                spare_windows: true,
                detached_survive: false,
                ..
            }) = &term
            {
                job.terminate_windowless();
            }
            drop(term);
            on_exit(code);
        });
        Ok(())
    }

    pub fn write(&self, id: &str, data: &[u8]) -> Result<()> {
        let mut terms = self.terms.lock();
        let t = terms.get_mut(id).ok_or_else(|| closed(i18n::ui()))?;
        t.io_at.store(now_ms(), Ordering::Release);
        t.writer.write_all(data)?;
        t.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        let terms = self.terms.lock();
        let t = terms.get(id).ok_or_else(|| closed(i18n::ui()))?;
        t.master
            .resize(PtySize {
                rows: rows.max(2),
                cols: cols.max(10),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| anyhow!("{e}"))
    }

    pub fn kill(&self, id: &str) {
        // Dropped outside the lock: closing the pseudo-console may wait for its reader, which
        // takes the lock to answer a cursor position query.
        let term = self.terms.lock().remove(id);
        if let Some(mut t) = term {
            // The whole tree at once: the pseudo-console, closed when the term is dropped, would
            // take its time to end what is attached to it, and not end the rest.
            if let Some(job) = &t.job {
                if t.spare_windows {
                    job.terminate_windowless();
                } else {
                    job.terminate();
                }
            }
            let _ = t.killer.kill();
        }
    }

    /// The launch commands running, and the interactive terminals' last input or output.
    pub fn activity(&self) -> Activity {
        let terms = self.terms.lock();
        Activity {
            launches: terms.values().filter(|t| t.launch).count(),
            shell_io_at: terms
                .values()
                .filter(|t| !t.launch)
                .map(|t| t.io_at.load(Ordering::Acquire))
                .max(),
        }
    }

    pub fn list(&self) -> Vec<TermInfo> {
        self.terms.lock().values().map(|t| t.info.clone()).collect()
    }

    pub fn kill_project(&self, project_id: &str) {
        let ids: Vec<String> = self
            .terms
            .lock()
            .values()
            .filter(|t| t.info.project_id == project_id)
            .map(|t| t.info.id.clone())
            .collect();
        for id in ids {
            self.kill(&id);
        }
    }

    pub fn kill_all(&self) {
        let ids: Vec<String> = self.terms.lock().keys().cloned().collect();
        for id in ids {
            self.kill(&id);
        }
    }
}

/// Tests starting a real shell run one at a time: several Windows PowerShells booting at once on a
/// CI runner can outlast what the tests wait for, and the ping tests would see each other's pings.
#[cfg(test)]
fn one_shell_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    static SHELLS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SHELLS.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{Duration, Instant};

    /// Fake system folders under `dir` (PATH entry, Program Files, LocalAppData, Windows).
    fn roots(dir: &Path) -> ShellRoots {
        ShellRoots {
            path: vec![dir.join("bin")],
            program_files: Some(dir.join("pf")),
            local_app_data: Some(dir.join("lad")),
            system_root: Some(dir.join("win")),
        }
    }

    fn touch(p: PathBuf) -> PathBuf {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "").unwrap();
        p
    }

    fn windows_powershell(dir: &Path) -> PathBuf {
        touch(dir.join(r"win\System32\WindowsPowerShell\v1.0\powershell.exe"))
    }

    fn ids(shells: &[ShellInfo]) -> Vec<&str> {
        shells.iter().map(|s| s.id.as_str()).collect()
    }

    #[test]
    fn windows_powershell_is_offered_when_powershell_7_is_not_installed() {
        let d = crate::paths::test_dir("shells-winps");
        let exe = windows_powershell(&d);
        let shells = detect_shells_in(&roots(&d), &Settings::default());
        assert_eq!(ids(&shells), ["powershell"]);
        assert_eq!(shells[0].label, "Windows PowerShell");
        assert_eq!(shells[0].path, exe.to_string_lossy());
    }

    #[test]
    fn powershell_7_is_found_off_the_path_and_preferred() {
        let d = crate::paths::test_dir("shells-pwsh-pf");
        windows_powershell(&d);
        let exe = touch(d.join(r"pf\PowerShell\7\pwsh.exe"));
        let shells = detect_shells_in(&roots(&d), &Settings::default());
        assert_eq!(ids(&shells), ["pwsh"]);
        assert_eq!(shells[0].path, exe.to_string_lossy());
    }

    #[test]
    fn powershell_7_from_the_microsoft_store_is_found() {
        let d = crate::paths::test_dir("shells-pwsh-store");
        windows_powershell(&d);
        let exe = touch(d.join(r"lad\Microsoft\WindowsApps\pwsh.exe"));
        let shells = detect_shells_in(&roots(&d), &Settings::default());
        assert_eq!(ids(&shells), ["pwsh"]);
        assert_eq!(shells[0].path, exe.to_string_lossy());
    }

    #[test]
    fn microsoft_store_app_aliases_count_as_installed() {
        // Store aliases are 0-byte reparse points: they must still count as installed programs.
        let Some(apps) = std::env::var_os("LOCALAPPDATA")
            .map(|d| PathBuf::from(d).join(r"Microsoft\WindowsApps"))
        else {
            return;
        };
        let Some(alias) = std::fs::read_dir(&apps).ok().and_then(|mut it| {
            it.find_map(|e| {
                let e = e.ok()?;
                (e.path().extension()? == "exe").then(|| e.path())
            })
        }) else {
            eprintln!("no Store alias on this machine: skipped");
            return;
        };
        assert!(installed(&alias), "{}", alias.display());
    }

    /// PIDs of running ping.exe processes.
    fn pings() -> Vec<u32> {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq PING.EXE", "/FO", "CSV", "/NH"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.split("\",\"").nth(1)?.parse().ok())
            .collect()
    }

    /// Whether the process `pid` is still running: a handle that can be waited on is signaled
    /// once the process ended (a pid is not proof of life: `tasklist` lags and ids are reused).
    fn alive(pid: u32) -> bool {
        use windows_sys::Win32::Foundation::{
            CloseHandle, GetLastError, ERROR_ACCESS_DENIED, WAIT_TIMEOUT,
        };
        use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: the handle is checked, waited on without blocking and closed.
        unsafe {
            let h = OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                // Out of reach but there; any other failure: no such process.
                return GetLastError() == ERROR_ACCESS_DENIED;
            }
            let running = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
            CloseHandle(h);
            running
        }
    }

    /// Kills the process `pid` if it is still there (a test that failed leaves its ping behind).
    fn kill_process(pid: u32) {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    }

    /// Opens a PowerShell 7 terminal `id` that answers the cursor position queries the way
    /// xterm.js does in the app; returns it with what it printed, once its prompt is there.
    fn open_pwsh(id: &'static str) -> Option<(PtyManager, Arc<Mutex<String>>)> {
        let Some(shell) = detect_shells(&Settings::default())
            .into_iter()
            .find(|s| s.id == "pwsh")
        else {
            eprintln!("PowerShell 7 not installed: skipped");
            return None;
        };
        let pty = PtyManager::default();
        let out = Arc::new(Mutex::new(String::new()));
        let (sink, answer) = (out.clone(), pty.clone());
        let info = TermInfo {
            id: id.into(),
            project_id: "p".into(),
            name: "pwsh-1".into(),
            shell: "pwsh".into(),
        };
        let cwd = std::env::temp_dir().to_string_lossy().to_string();
        let query = Mutex::new(CursorQuery::default());
        let on_data = move |b: Vec<u8>| {
            if query.lock().feed(&b) {
                let _ = answer.write(id, b"\x1b[1;1R");
            }
            sink.lock().push_str(&String::from_utf8_lossy(&b));
        };
        pty.spawn(info, &shell, "", &cwd, (120, 30), vec![], on_data, |_| {})
            .unwrap();
        let start = Instant::now();
        while !out.lock().contains("PS ") {
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "no prompt: {}",
                out.lock()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(1500));
        Some((pty, out))
    }

    /// Types `command`, which starts a ping, in terminal `id`; returns the ping's pid.
    fn start_ping(pty: &PtyManager, out: &Mutex<String>, id: &str, command: &str) -> u32 {
        let before = pings();
        pty.write(id, format!("{command}\r").as_bytes()).unwrap();
        let start = Instant::now();
        loop {
            if let Some(p) = pings().into_iter().find(|p| !before.contains(p)) {
                return p;
            }
            assert!(
                start.elapsed() < Duration::from_secs(90),
                "ping did not start: {}",
                out.lock()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Types `command`, which starts a ping, then closes the terminal: the ping must die with it.
    fn closing_kills_the_ping_of(id: &'static str, command: &str) {
        let _one = one_shell_at_a_time();
        let Some((pty, out)) = open_pwsh(id) else {
            return;
        };
        let pid = start_ping(&pty, &out, id, command);
        assert!(alive(pid));
        pty.kill(id);
        // The pseudo-console closes on its own time (seconds on a loaded runner): the job does not
        // wait for it.
        let start = Instant::now();
        while alive(pid) {
            if start.elapsed() > Duration::from_secs(20) {
                kill_process(pid);
                panic!("ping {pid} survived its terminal");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[test]
    fn closing_a_terminal_kills_the_programs_it_started() {
        closing_kills_the_ping_of("t1", "ping -n 60 127.0.0.1");
    }

    #[test]
    fn closing_a_terminal_also_kills_programs_started_outside_its_console() {
        // Hidden, in a console of its own: closing the pseudo-console does not reach it.
        closing_kills_the_ping_of(
            "t4",
            "Start-Process ping -ArgumentList '-n','60','127.0.0.1' -WindowStyle Hidden",
        );
    }

    /// Types `command`, which detaches a ping, then `exit`: the ping must run on after the shell
    /// and its terminal are gone.
    fn exit_leaves_the_ping_of(id: &'static str, command: &str) {
        let _one = one_shell_at_a_time();
        let Some((pty, out)) = open_pwsh(id) else {
            return;
        };
        let pid = start_ping(&pty, &out, id, command);
        pty.write(id, b"exit\r").unwrap();
        let start = Instant::now();
        while pty.list().iter().any(|t| t.id == id) {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "the shell did not end: {}",
                out.lock()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        // The shell is gone and its terminal dropped: what it detached must still run.
        std::thread::sleep(Duration::from_secs(2));
        let survived = alive(pid);
        kill_process(pid);
        assert!(survived, "ping {pid} died with the shell that started it");
    }

    #[test]
    fn a_shell_that_ends_by_itself_leaves_the_programs_its_user_detached() {
        // A console of its own, hidden, as `Start-Process` gives a console program. (One sharing the
        // terminal's console, `start /b`, goes with the pseudo-console when its shell ends: that
        // is Windows closing it, no job involved.)
        exit_leaves_the_ping_of(
            "t2",
            "Start-Process ping -ArgumentList '-n','30','127.0.0.1' -WindowStyle Hidden",
        );
    }

    /// Stops the launch command `command`, which starts a ping: the ping must die with it.
    fn stop_kills_its_ping(id: &str, command: &str) {
        let _one = one_shell_at_a_time();
        let pty = PtyManager::default();
        let Some(shell) = detect_shells(&Settings::default())
            .into_iter()
            .find(|s| s.id == "pwsh" || s.id == "powershell")
        else {
            eprintln!("PowerShell not installed: skipped");
            return;
        };
        let info = TermInfo {
            id: id.into(),
            project_id: "p".into(),
            name: "serveur".into(),
            shell: shell.id.clone(),
        };
        let cwd = std::env::temp_dir().to_string_lossy().to_string();
        let before = pings();
        pty.spawn_command(
            info,
            &shell,
            "",
            &cwd,
            (120, 30),
            vec![],
            1,
            command,
            |_| {},
            |_| {},
        )
        .unwrap();
        let start = Instant::now();
        let pid = loop {
            if let Some(p) = pings().into_iter().find(|p| !before.contains(p)) {
                break p;
            }
            assert!(
                start.elapsed() < Duration::from_secs(90),
                "ping did not start"
            );
            std::thread::sleep(Duration::from_millis(50));
        };
        pty.kill(id);
        let start = Instant::now();
        while alive(pid) {
            if start.elapsed() > Duration::from_secs(5) {
                kill_process(pid);
                panic!("ping {pid} survived its launch command");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[test]
    fn stopping_a_launch_command_kills_the_programs_it_started() {
        stop_kills_its_ping("run1", "ping -n 60 127.0.0.1");
    }

    #[test]
    fn stopping_a_launch_command_also_kills_programs_started_outside_its_console() {
        stop_kills_its_ping(
            "run2",
            "Start-Process ping -ArgumentList '-n','60','127.0.0.1' -WindowStyle Hidden; Start-Sleep 60",
        );
    }

    /// Kills the processes it holds when it is dropped: a test that failed leaves them running.
    #[derive(Default)]
    struct Reaper(Vec<u32>);

    impl Drop for Reaper {
        fn drop(&mut self) {
            for pid in &self.0 {
                kill_process(*pid);
            }
        }
    }

    /// What a PowerShell script of the tests does before it waits: a console program only says it
    /// runs (its process id in `pid_file`), a window program opens a form first (and says "none"
    /// when Windows shows it no window: no desktop to show it on).
    fn script_text(pid_file: &Path, window: bool) -> String {
        let pid_file = pid_file.display();
        if window {
            format!(
                "Add-Type -AssemblyName System.Windows.Forms\r\n$f = New-Object Windows.Forms.Form\r\n$f.Show()\r\nSet-Content -Path '{pid_file}' -Value $(if ($f.Visible) {{ $PID }} else {{ 'none' }})\r\nStart-Sleep 60\r\n"
            )
        } else {
            format!("Set-Content -Path '{pid_file}' -Value $PID\r\nStart-Sleep 60\r\n")
        }
    }

    /// Writes the script `name` in `dir`; returns it with the file its process id will be told in.
    fn write_script(dir: &Path, name: &str, window: bool) -> (PathBuf, PathBuf) {
        let (script, pid_file) = (
            dir.join(format!("{name}.ps1")),
            dir.join(format!("{name}.pid")),
        );
        std::fs::write(&script, script_text(&pid_file, window)).unwrap();
        (script, pid_file)
    }

    /// The command that runs `script` in a PowerShell of its own, outside the console of the
    /// terminal: hidden (a console program) or showing its window.
    fn start_command(script: &Path, hidden: bool) -> String {
        let style = if hidden { " -WindowStyle Hidden" } else { "" };
        format!(
            "Start-Process powershell{style} -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','\"{}\"'",
            script.display()
        )
    }

    /// The process id a script told in `pid_file`; None when it could not show its window.
    fn told_pid(pid_file: &Path) -> Option<u32> {
        let start = Instant::now();
        loop {
            let text = std::fs::read_to_string(pid_file).unwrap_or_default();
            if text.trim() == "none" {
                return None;
            }
            if let Ok(pid) = text.trim().parse() {
                return Some(pid);
            }
            assert!(
                start.elapsed() < Duration::from_secs(90),
                "the script told no process id"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Waits for the process `pid` to end; false if it is still there after `secs`.
    fn ends_within(pid: u32, secs: u64) -> bool {
        let start = Instant::now();
        while alive(pid) {
            if start.elapsed() > Duration::from_secs(secs) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        true
    }

    #[test]
    fn closing_a_terminal_ends_its_console_programs_but_spares_the_ones_with_a_window() {
        let _one = one_shell_at_a_time();
        let Some((pty, out)) = open_pwsh("t5") else {
            return;
        };
        let dir = crate::paths::test_dir("pty-close-spares-windows");
        let mut reaper = Reaper::default();
        // A program with no window, in a console of its own; a program showing a window.
        let (console_script, console_file) = write_script(&dir, "console", false);
        let (window_script, window_file) = write_script(&dir, "window", true);
        pty.write(
            "t5",
            format!("{}\r", start_command(&console_script, true)).as_bytes(),
        )
        .unwrap();
        let console = told_pid(&console_file).unwrap();
        reaper.0.push(console);
        pty.write(
            "t5",
            format!("{}\r", start_command(&window_script, false)).as_bytes(),
        )
        .unwrap();
        let Some(window) = told_pid(&window_file) else {
            eprintln!("no window could be shown here: skipped");
            return;
        };
        reaper.0.push(window);
        assert!(alive(console) && alive(window), "{}", out.lock());

        pty.kill("t5");

        assert!(
            ends_within(console, 20),
            "the program without a window survived its terminal"
        );
        // The terminal is closed and gone: the window still stands (given the time a wrongly
        // killed one would take to end).
        assert!(
            !ends_within(window, 3),
            "the program showing a window died with its terminal"
        );
    }

    #[test]
    fn a_terminal_dropped_without_being_closed_kills_nothing() {
        // What the app's crash, its quit or an update's restart does to a terminal: its handles
        // close, nothing asks the processes to end.
        let _one = one_shell_at_a_time();
        let Some((pty, out)) = open_pwsh("t6") else {
            return;
        };
        let dir = crate::paths::test_dir("pty-dropped-kills-nothing");
        let mut reaper = Reaper::default();
        let (script, pid_file) = write_script(&dir, "console", false);
        pty.write(
            "t6",
            format!("{}\r", start_command(&script, true)).as_bytes(),
        )
        .unwrap();
        let program = told_pid(&pid_file).unwrap();
        reaper.0.push(program);
        assert!(alive(program), "{}", out.lock());

        let term = pty.terms.lock().remove("t6");
        drop(term);

        std::thread::sleep(Duration::from_secs(2));
        assert!(alive(program), "the dropped terminal killed its program");
    }

    /// Runs a program (a PowerShell script) in a terminal of its own: it starts a console program
    /// and a program showing a window, outside its console, and ends. Returns whether the console
    /// program and the window one ended with it; None when no window could be shown here.
    fn what_the_end_of_a_program_takes(
        name: &'static str,
        spare_windows: bool,
    ) -> Option<(bool, bool)> {
        let _one = one_shell_at_a_time();
        let Some(shell) = detect_shells(&Settings::default())
            .into_iter()
            .find(|s| s.id == "pwsh" || s.id == "powershell")
        else {
            eprintln!("PowerShell not installed: skipped");
            return None;
        };
        let dir = crate::paths::test_dir(name);
        let mut reaper = Reaper::default();
        let (console_script, console_file) = write_script(&dir, "console", false);
        let (window_script, window_file) = write_script(&dir, "window", true);
        // Both run before it ends: each says so by its file.
        let program = dir.join("program.ps1");
        std::fs::write(
            &program,
            format!(
                "{}\r\n{}\r\nwhile (-not ((Test-Path '{}') -and (Test-Path '{}'))) {{ Start-Sleep -Milliseconds 100 }}\r\nStart-Sleep 1\r\n",
                start_command(&console_script, true),
                start_command(&window_script, false),
                console_file.display(),
                window_file.display(),
            ),
        )
        .unwrap();
        let pty = PtyManager::default();
        let (answer, query) = (pty.clone(), Mutex::new(CursorQuery::default()));
        let info = TermInfo {
            id: name.into(),
            project_id: String::new(),
            name: "connexion".into(),
            shell: "claude".into(),
        };
        let (ended, done) = std::sync::mpsc::channel();
        pty.spawn_program(
            info,
            &crate::claude::Program::Native(PathBuf::from(&shell.path)),
            &[
                "-NoProfile".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                program.display().to_string(),
            ],
            &dir.to_string_lossy(),
            (120, 30),
            vec![],
            spare_windows,
            move |b: Vec<u8>| {
                if query.lock().feed(&b) {
                    let _ = answer.write(name, b"\x1b[1;1R");
                }
            },
            move |_| {
                let _ = ended.send(());
            },
        )
        .unwrap();
        done.recv_timeout(Duration::from_secs(180))
            .expect("the program did not end");
        let console = told_pid(&console_file).unwrap();
        reaper.0.push(console);
        let window = told_pid(&window_file);
        reaper.0.extend(window);
        Some((ends_within(console, 20), ends_within(window?, 3)))
    }

    #[test]
    fn the_end_of_the_sign_in_ends_what_it_started_but_the_programs_with_a_window() {
        let Some((console_ended, window_ended)) =
            what_the_end_of_a_program_takes("pty-sign-in-spares-windows", true)
        else {
            return;
        };
        assert!(console_ended, "the program without a window survived");
        assert!(!window_ended, "the browser of the sign-in died with it");
    }

    #[test]
    fn the_end_of_the_claude_code_view_ends_everything_it_started() {
        let Some((console_ended, window_ended)) =
            what_the_end_of_a_program_takes("pty-view-kills-all", false)
        else {
            return;
        };
        assert!(console_ended, "the program without a window survived");
        assert!(window_ended, "the program with a window survived");
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn touch(p: PathBuf) -> PathBuf {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "").unwrap();
        p
    }

    #[test]
    fn the_login_shell_comes_first_and_fish_is_found_on_the_path() {
        let d = crate::paths::test_dir("shells-unix");
        let fish = touch(d.join("bin").join("fish"));
        let shells = detect_unix_shells(Some(&fish), &[d.join("bin")], &Settings::default());
        assert_eq!(shells[0].id, "fish");
        assert_eq!(shells[0].path, fish.to_string_lossy());
        assert_eq!(shells.iter().filter(|s| s.id == "fish").count(), 1);
        assert!(shells
            .iter()
            .any(|s| s.id == "sh" || s.id == "bash" || s.id == "zsh"));
    }

    /// Runs `command` with zsh or bash as a launch command, away from the machine's ~/.zshrc;
    /// returns its output and exit code.
    fn launch(command: &str) -> (String, Option<u32>) {
        let shell = detect_unix_shells(None, &[], &Settings::default())
            .into_iter()
            .find(|s| s.id == "bash" || s.id == "zsh")
            .expect("a shell");
        let home = crate::paths::test_dir("launch-zdotdir");
        launch_with(&shell, command, zdotdir(&home))
    }

    /// Runs `command` with `shell` as a launch command, `env` added; returns its output and exit
    /// code.
    fn launch_with(
        shell: &ShellInfo,
        command: &str,
        env: Vec<(String, String)>,
    ) -> (String, Option<u32>) {
        let _one = one_shell_at_a_time();
        let pty = PtyManager::default();
        let out = Arc::new(Mutex::new(String::new()));
        let exit = Arc::new(Mutex::new(None));
        let (sink, done) = (out.clone(), exit.clone());
        let info = TermInfo {
            id: format!("run-{command}"),
            project_id: "p".into(),
            name: "run".into(),
            shell: shell.id.clone(),
        };
        let cwd = std::env::temp_dir().to_string_lossy().to_string();
        pty.spawn_command(
            info,
            shell,
            "",
            &cwd,
            (120, 30),
            env,
            1,
            command,
            move |b| sink.lock().push_str(&String::from_utf8_lossy(&b)),
            move |code| *done.lock() = Some(code),
        )
        .unwrap();
        let start = Instant::now();
        while exit.lock().is_none() {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "did not end: {}",
                out.lock()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(100));
        let code = exit.lock().unwrap();
        let text = out.lock().clone();
        (text, code)
    }

    #[test]
    fn a_launch_command_reports_its_output_and_exit_code() {
        let (out, code) = launch("echo hello-escouade; exit 3");
        assert!(out.contains("hello-escouade"), "{out}");
        assert_eq!(code, Some(3));
        assert_eq!(launch("true").1, Some(0));
    }

    /// zsh, when the machine has it (macOS always does).
    fn zsh() -> Option<ShellInfo> {
        let zsh = detect_unix_shells(None, &[], &Settings::default())
            .into_iter()
            .find(|s| s.id == "zsh");
        if zsh.is_none() {
            eprintln!("zsh not installed: skipped");
        }
        zsh
    }

    /// The variables pointing zsh to `dir` for its startup files, with an empty .zshrc there
    /// unless it has one: without any, zsh-newuser-install (Homebrew's zsh, Linux's) would wait
    /// for an answer.
    fn zdotdir(dir: &Path) -> Vec<(String, String)> {
        let zshrc = dir.join(".zshrc");
        if !zshrc.exists() {
            std::fs::write(zshrc, "").unwrap();
        }
        vec![("ZDOTDIR".into(), dir.to_string_lossy().into_owned())]
    }

    #[test]
    fn a_zsh_launch_command_runs_the_ruby_its_zshrc_sets_up_like_a_terminal() {
        use std::os::unix::fs::PermissionsExt;
        let Some(zsh) = zsh() else { return };
        // Version managers (rbenv, nvm, asdf…) set themselves up in ~/.zshrc: without it, a
        // Rails server starts with the system's Ruby, which macOS's /etc/zprofile puts first.
        let home = crate::paths::test_dir("launch-zshrc");
        let ruby = home.join("bin").join("ruby");
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::write(&ruby, "#!/bin/sh\necho ruby-du-zshrc\n").unwrap();
        std::fs::set_permissions(&ruby, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(home.join(".zshrc"), "export PATH=\"$ZDOTDIR/bin:$PATH\"\n").unwrap();
        let (out, code) = launch_with(&zsh, "ruby; echo omz=$DISABLE_AUTO_UPDATE", zdotdir(&home));
        assert!(out.contains("ruby-du-zshrc"), "{out}");
        // Oh My Zsh would ask whether to update itself in the log, which takes no input.
        assert!(out.contains("omz=true"), "{out}");
        assert_eq!(code, Some(0));
    }

    #[test]
    fn stopping_a_zsh_launch_command_kills_the_programs_it_started() {
        let Some(zsh) = zsh() else { return };
        let _one = one_shell_at_a_time();
        let home = crate::paths::test_dir("launch-zsh-stop");
        let pid_file = home.join("pid.txt");
        // Out of the shell's process group (job control), a program ignoring SIGHUP (started
        // with nohup, a server handling it) would outlive the launch command. `; true`: the shell
        // forks the program instead of becoming it.
        let command = format!(
            "sh -c 'trap \"\" HUP; echo $$ > \"{}\"; exec sleep 60'; true",
            pid_file.display()
        );
        let pty = PtyManager::default();
        let info = TermInfo {
            id: "run-zsh-stop".into(),
            project_id: "p".into(),
            name: "serveur".into(),
            shell: zsh.id.clone(),
        };
        pty.spawn_command(
            info,
            &zsh,
            "",
            &home.to_string_lossy(),
            (120, 30),
            zdotdir(&home),
            1,
            &command,
            |_| {},
            |_| {},
        )
        .unwrap();
        let start = Instant::now();
        let pid: i32 = loop {
            if let Some(p) = std::fs::read_to_string(&pid_file)
                .ok()
                .and_then(|s| s.trim().parse().ok())
            {
                break p;
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "the program did not start"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        pty.kill("run-zsh-stop");
        // SAFETY: signal 0 only checks that the process exists.
        let alive = || unsafe { libc::kill(pid, 0) == 0 };
        let start = Instant::now();
        while alive() {
            if start.elapsed() > Duration::from_secs(5) {
                // SAFETY: plain syscall on the process this test started.
                unsafe { libc::kill(pid, libc::SIGKILL) };
                panic!("process {pid} survived its launch command");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn alive(pid: i32) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// Kills the processes it holds when it is dropped: a test that failed leaves them running.
    #[derive(Default)]
    struct Reaper(Vec<i32>);

    impl Drop for Reaper {
        fn drop(&mut self) {
            for pid in &self.0 {
                // SAFETY: plain syscall on a process the test started.
                unsafe { libc::kill(*pid, libc::SIGKILL) };
            }
        }
    }

    /// Writes the shell script `name` in `dir`, executable.
    fn script(dir: &Path, name: &str, text: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{text}")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// The command starting, in the background, a process that tells its id in `pid_file`, then
    /// waits a minute.
    fn background(pid_file: &Path) -> String {
        format!(
            "sh -c 'echo $$ > \"$1\"; exec sleep 60' sh '{}' &",
            pid_file.display()
        )
    }

    /// The process id told in `pid_file`.
    fn told_pid(pid_file: &Path) -> i32 {
        let start = Instant::now();
        loop {
            if let Some(pid) = std::fs::read_to_string(pid_file)
                .ok()
                .and_then(|s| s.trim().parse().ok())
            {
                return pid;
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "no process id in {}",
                pid_file.display()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Waits for the process `pid` to end; false if it is still there after `secs`.
    fn ends_within(pid: i32, secs: u64) -> bool {
        let start = Instant::now();
        while alive(pid) {
            if start.elapsed() > Duration::from_secs(secs) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        true
    }

    /// How a program of a terminal ends in `what_a_program_takes`.
    #[derive(PartialEq)]
    enum End {
        /// It ends by itself.
        Itself,
        /// Its terminal is closed while it runs.
        Closed,
    }

    /// Runs a program standing for Claude Code in a terminal of its own: it starts a process in
    /// the background (a program without a window) and opens a page with `$BROWSER`, as Claude
    /// Code does; the browser (a fake one) starts the process standing for itself and returns, as
    /// `xdg-open` does when no browser runs yet. Then the program ends as `end` says. With
    /// `sign_in`, it runs as the sign-in does: sparing what shows a window, with Escouade's
    /// opener. Returns whether the background process and the browser ended with it.
    fn what_a_program_takes(name: &'static str, sign_in: bool, end: End) -> (bool, bool) {
        let _one = one_shell_at_a_time();
        let dir = crate::paths::test_dir(name);
        let mut reaper = Reaper::default();
        let (console_file, browser_file) = (dir.join("console.pid"), dir.join("browser.pid"));
        let browser = script(&dir, "browser", &background(&browser_file));
        let wait = match end {
            End::Itself => "sleep 1",
            End::Closed => "sleep 60",
        };
        let program = script(
            &dir,
            "claude",
            &format!(
                "{}\n\"$BROWSER\" 'https://claude.ai/oauth/authorize?code=true'\nwhile [ ! -s '{}' ] || [ ! -s '{}' ]; do sleep 0.1; done\n{wait}\n",
                background(&console_file),
                console_file.display(),
                browser_file.display(),
            ),
        );
        let mut env = vec![("BROWSER".to_string(), browser.display().to_string())];
        if sign_in {
            open_browser_apart(&dir.join("open-browser"), &mut env).unwrap();
        }
        let pty = PtyManager::default();
        let info = TermInfo {
            id: name.into(),
            project_id: String::new(),
            name: "connexion".into(),
            shell: "claude".into(),
        };
        let (ended, done) = std::sync::mpsc::channel();
        pty.spawn_program(
            info,
            &crate::claude::Program::Native(PathBuf::from("/bin/sh")),
            &[program.display().to_string()],
            &dir.to_string_lossy(),
            (120, 30),
            env,
            sign_in,
            |_| {},
            move |_| {
                let _ = ended.send(());
            },
        )
        .unwrap();
        let console = told_pid(&console_file);
        reaper.0.push(console);
        let browser = told_pid(&browser_file);
        reaper.0.push(browser);
        if end == End::Closed {
            assert!(alive(console) && alive(browser));
            pty.kill(name);
        }
        done.recv_timeout(Duration::from_secs(60))
            .expect("the program did not end");
        // The browser is given the time a wrongly killed one would take to end.
        (ends_within(console, 10), ends_within(browser, 3))
    }

    #[test]
    fn the_end_of_the_sign_in_ends_what_it_started_but_the_browser_it_opened() {
        let (console_ended, browser_ended) =
            what_a_program_takes("pty-unix-sign-in-end", true, End::Itself);
        assert!(console_ended, "the program without a window survived");
        assert!(!browser_ended, "the browser of the sign-in died with it");
    }

    #[test]
    fn closing_the_sign_in_ends_what_it_started_but_the_browser_it_opened() {
        let (console_ended, browser_ended) =
            what_a_program_takes("pty-unix-sign-in-close", true, End::Closed);
        assert!(console_ended, "the program without a window survived");
        assert!(!browser_ended, "the browser of the sign-in died with it");
    }

    #[test]
    fn the_end_of_the_claude_code_view_ends_everything_it_started() {
        let (console_ended, browser_ended) =
            what_a_program_takes("pty-unix-view-end", false, End::Itself);
        assert!(console_ended, "the program without a window survived");
        assert!(browser_ended, "the browser survived");
    }

    #[test]
    fn the_sign_in_opens_the_users_browser_with_the_opener() {
        let dir = crate::paths::test_dir("pty-unix-opener-env");
        let opener = dir.join("open-browser");
        let mut env = vec![
            ("HTTPS_PROXY".to_string(), "http://proxy:3128".to_string()),
            ("BROWSER".to_string(), "/usr/bin/firefox".to_string()),
        ];
        open_browser_apart(&opener, &mut env).unwrap();
        let var = |k: &str| -> Vec<&str> {
            env.iter()
                .filter(|(n, _)| n == k)
                .map(|(_, v)| v.as_str())
                .collect()
        };
        assert_eq!(var("BROWSER"), vec![opener.to_str().unwrap()]);
        assert_eq!(var(USER_BROWSER_VAR), vec!["/usr/bin/firefox"]);
        assert_eq!(var("HTTPS_PROXY"), vec!["http://proxy:3128"]);
        // The opener itself is never taken for the user's browser: it would open nothing.
        let mut again = env.clone();
        open_browser_apart(&opener, &mut again).unwrap();
        assert_eq!(
            again.iter().find(|(k, _)| k == USER_BROWSER_VAR).unwrap().1,
            "/usr/bin/firefox"
        );
    }

    #[test]
    fn without_a_browser_of_the_user_the_opener_runs_the_systems() {
        let dir = crate::paths::test_dir("pty-unix-opener-system");
        let opener = dir.join("open-browser");
        open_browser_apart(&opener, &mut Vec::new()).unwrap();
        // Stand-ins for the system's openers: each tells what it was asked to open.
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let told = dir.join("told.txt");
        for name in ["xdg-open", "open"] {
            script(
                &bin,
                name,
                &format!("echo \"{name} $1\" > '{}'\n", told.display()),
            );
        }
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let status = std::process::Command::new(&opener)
            .arg("https://claude.ai/login")
            .env("PATH", path)
            .env(USER_BROWSER_VAR, "")
            .env("BROWSER", &opener)
            .status()
            .unwrap();
        assert!(status.success());
        let system = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        assert_eq!(
            std::fs::read_to_string(&told).unwrap().trim(),
            format!("{system} https://claude.ai/login")
        );
    }
}

#[cfg(all(test, windows))]
mod windows_powershell_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn windows_powershell() -> Option<ShellInfo> {
        let roots = ShellRoots {
            path: vec![],
            program_files: None,
            local_app_data: None,
            system_root: std::env::var_os("SystemRoot").map(PathBuf::from),
        };
        detect_shells_in(&roots, &Settings::default())
            .into_iter()
            .find(|s| s.id == "powershell")
    }

    /// Runs `command` as a launch command; returns its output and exit code.
    fn launch(command: &str) -> Option<(String, Option<u32>)> {
        let _one = one_shell_at_a_time();
        let shell = windows_powershell()?;
        let pty = PtyManager::default();
        let out = Arc::new(Mutex::new(String::new()));
        let exit = Arc::new(Mutex::new(None));
        let (sink, done) = (out.clone(), exit.clone());
        let info = TermInfo {
            id: new_test_id(),
            project_id: "p".into(),
            name: "run".into(),
            shell: shell.id.clone(),
        };
        let cwd = std::env::temp_dir().to_string_lossy().to_string();
        pty.spawn_command(
            info,
            &shell,
            "",
            &cwd,
            (120, 30),
            vec![],
            1,
            command,
            move |b| sink.lock().push_str(&String::from_utf8_lossy(&b)),
            move |code| *done.lock() = Some(code),
        )
        .unwrap();
        let start = Instant::now();
        while exit.lock().is_none() {
            assert!(
                start.elapsed() < Duration::from_secs(90),
                "did not end: {}",
                out.lock()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        // Let the reader drain what the pseudo-console still holds.
        std::thread::sleep(Duration::from_millis(300));
        let code = exit.lock().unwrap();
        let text = out.lock().clone();
        Some((text, code))
    }

    fn new_test_id() -> String {
        format!(
            "run-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    #[test]
    fn a_launch_command_runs_to_its_end_and_reports_its_exit_code() {
        let Some((out, code)) = launch("Write-Output ('lancement-' + 'ok'); exit 3") else {
            return;
        };
        assert!(out.contains("lancement-ok"), "{out}");
        assert_eq!(code, Some(3));
    }

    #[test]
    fn a_resized_launch_command_leaves_the_lines_above_its_start_alone() {
        let _one = one_shell_at_a_time();
        let Some(shell) = windows_powershell() else {
            return;
        };
        let pty = PtyManager::default();
        let out = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = out.clone();
        let info = TermInfo {
            id: new_test_id(),
            project_id: "p".into(),
            name: "run".into(),
            shell: shell.id.clone(),
        };
        let id = info.id.clone();
        let cwd = std::env::temp_dir().to_string_lossy().to_string();
        // The log already holds four lines: the command starts on the fifth.
        pty.spawn_command(
            info,
            &shell,
            "",
            &cwd,
            (80, 20),
            vec![],
            5,
            "Write-Output pret; Start-Sleep 20",
            move |b| sink.lock().extend(b),
            |_| {},
        )
        .unwrap();
        let text = |from: usize| String::from_utf8_lossy(&out.lock()[from..]).to_string();
        let start = Instant::now();
        while !text(0).contains("pret") {
            assert!(start.elapsed() < Duration::from_secs(90), "no output");
            std::thread::sleep(Duration::from_millis(50));
        }
        let n = out.lock().len();
        pty.resize(&id, 100, 25).unwrap();
        // The pseudo-console repaints its screen from where the command started, not from the top.
        let start = Instant::now();
        while !text(n).contains("pret") && start.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(50));
        }
        pty.kill(&id);
        let repaint = text(n);
        assert!(
            screen_writes(&repaint).iter().all(|&row| row >= 5),
            "{repaint:?}"
        );
    }

    /// Rows `out` moves the cursor to, or erases from (1 for an erase of the screen).
    fn screen_writes(out: &str) -> Vec<u32> {
        out.split("\x1b[")
            .skip(1)
            .filter_map(|seq| {
                let end = seq.find(|c: char| c.is_ascii_alphabetic())?;
                let first = seq[..end].split(';').next()?.parse().unwrap_or(1);
                match (&seq[end..end + 1], first) {
                    ("H" | "f", row) => Some(row),
                    ("J", 1..=3) => Some(1),
                    _ => None,
                }
            })
            .collect()
    }

    #[test]
    fn a_launch_command_reports_the_exit_code_of_the_program_that_failed() {
        let Some((out, code)) = launch("cmd /c exit 5") else {
            return;
        };
        assert_eq!(code, Some(5), "{out}");
    }

    #[test]
    fn a_launch_command_whose_last_cmdlet_fails_ends_in_error() {
        let Some((out, code)) = launch(r"Get-Item C:\ccm-introuvable-42") else {
            return;
        };
        assert_eq!(code, Some(1), "{out}");
    }

    #[test]
    fn a_launch_command_that_recovers_from_a_failure_ends_with_code_0() {
        let Some((out, code)) = launch("cmd /c exit 5; Write-Output repris") else {
            return;
        };
        assert_eq!(code, Some(0), "{out}");
    }

    #[test]
    fn a_launch_command_that_succeeds_ends_with_code_0() {
        let Some((_, code)) = launch("Write-Output fini") else {
            return;
        };
        assert_eq!(code, Some(0));
    }

    #[test]
    fn a_windows_powershell_terminal_runs_commands() {
        let _one = one_shell_at_a_time();
        let root = std::env::var_os("SystemRoot").map(PathBuf::from);
        let roots = ShellRoots {
            path: vec![],
            program_files: None,
            local_app_data: None,
            system_root: root,
        };
        let Some(shell) = detect_shells_in(&roots, &Settings::default())
            .into_iter()
            .find(|s| s.id == "powershell")
        else {
            eprintln!("Windows PowerShell not found: skipped");
            return;
        };
        let pty = PtyManager::default();
        let out = Arc::new(Mutex::new(String::new()));
        let (sink, answer) = (out.clone(), pty.clone());
        let info = TermInfo {
            id: "wps".into(),
            project_id: "p".into(),
            name: "powershell-1".into(),
            shell: shell.id.clone(),
        };
        let cwd = std::env::temp_dir().to_string_lossy().to_string();
        let query = Mutex::new(CursorQuery::default());
        pty.spawn(
            info,
            &shell,
            "",
            &cwd,
            (120, 30),
            vec![],
            move |b| {
                if query.lock().feed(&b) {
                    let _ = answer.write("wps", b"\x1b[1;1R");
                }
                sink.lock().push_str(&String::from_utf8_lossy(&b));
            },
            |_| {},
        )
        .unwrap();
        let start = Instant::now();
        let mut sent = false;
        while !out.lock().contains("ccm-ok-42") {
            if !sent && out.lock().contains("PS ") {
                pty.write("wps", b"Write-Output ('ccm-ok-' + 42)\r")
                    .unwrap();
                sent = true;
            }
            assert!(
                start.elapsed() < Duration::from_secs(90),
                "no output: {}",
                out.lock()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        pty.kill("wps");
    }
}

#[cfg(test)]
mod cursor_query_tests {
    use super::*;

    /// Whether the query is found in the last of `reads`, which `CursorQuery` has seen in order.
    fn answers(reads: &[&str]) -> Vec<bool> {
        let mut query = CursorQuery::default();
        reads.iter().map(|r| query.feed(r.as_bytes())).collect()
    }

    #[test]
    fn a_query_inside_one_read_is_found() {
        assert_eq!(answers(&["\x1b[6n"]), [true]);
        assert_eq!(answers(&["PS C:\\> \x1b[6n\x1b[?25h"]), [true]);
    }

    #[test]
    fn a_query_cut_between_two_reads_is_found_once_complete() {
        assert_eq!(answers(&["\x1b[", "6n"]), [false, true]);
        assert_eq!(answers(&["\x1b", "[6n"]), [false, true]);
        assert_eq!(answers(&["\x1b[6", "n"]), [false, true]);
        // Cut in three, the middle read holding a single byte.
        assert_eq!(answers(&["ab\x1b", "[", "6n cd"]), [false, false, true]);
    }

    #[test]
    fn a_query_is_found_once_not_again_in_the_reads_after_it() {
        assert_eq!(answers(&["\x1b[6n", "x", "[6n"]), [true, false, false]);
        assert_eq!(answers(&["\x1b[6n", "\x1b[6n"]), [true, true]);
    }

    #[test]
    fn other_sequences_are_not_queries() {
        assert_eq!(answers(&["\x1b[5n", "\x1b[6", "m", "[6n"]), [false; 4]);
        assert_eq!(
            answers(&["", "\x1b[", "", "6n"]),
            [false, false, false, true]
        );
    }
}

#[cfg(test)]
mod run_cwd_tests {
    use super::*;

    #[test]
    fn a_missing_folder_and_a_closed_terminal_read_in_english() {
        use crate::i18n::Lang::En;
        assert_eq!(
            missing_folder(En, "api"),
            "The folder “api” doesn’t exist in the project"
        );
        assert_eq!(closed(En).to_string(), "terminal closed");
    }

    #[test]
    fn a_launch_command_runs_in_the_project_or_one_of_its_folders() {
        let d = crate::paths::test_dir("run-cwd");
        std::fs::create_dir_all(d.join("web")).unwrap();
        let root = d.to_string_lossy().to_string();
        assert_eq!(run_cwd(&root, "").unwrap(), root);
        assert_eq!(run_cwd(&root, "  ").unwrap(), root);
        assert_eq!(
            run_cwd(&root, "web").unwrap(),
            d.join("web").to_string_lossy()
        );
        let err = run_cwd(&root, "api").unwrap_err().to_string();
        assert!(err.contains("api"), "{err}");
    }
}

#[cfg(test)]
mod activity_tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A shell of this machine that runs a launch command the way the app does.
    fn shell() -> Option<ShellInfo> {
        detect_shells(&Settings::default())
            .into_iter()
            .find(|s| matches!(s.id.as_str(), "pwsh" | "powershell" | "bash" | "zsh"))
    }

    fn info(id: &str, shell: &ShellInfo) -> TermInfo {
        TermInfo {
            id: id.into(),
            project_id: "p".into(),
            name: id.into(),
            shell: shell.id.clone(),
        }
    }

    #[test]
    fn a_launch_counts_while_it_runs_and_a_terminal_by_its_last_input_or_output() {
        let _one = one_shell_at_a_time();
        let Some(sh) = shell() else {
            eprintln!("no shell found: skipped");
            return;
        };
        let pty = PtyManager::default();
        let cwd = std::env::temp_dir().to_string_lossy().to_string();
        assert_eq!(pty.activity(), Activity::default());

        let pause = match sh.id.as_str() {
            "pwsh" | "powershell" => "Start-Sleep -Milliseconds 1500",
            _ => "sleep 1.5",
        };
        let ended = Arc::new(Mutex::new(false));
        let done = ended.clone();
        pty.spawn_command(
            info("act-run", &sh),
            &sh,
            "",
            &cwd,
            (80, 24),
            vec![],
            1,
            pause,
            |_| {},
            move |_| *done.lock() = true,
        )
        .unwrap();
        // A launch is no interactive terminal: it counts while it runs, whatever it prints.
        assert_eq!(
            pty.activity(),
            Activity {
                launches: 1,
                shell_io_at: None
            }
        );
        let start = Instant::now();
        while !*ended.lock() {
            assert!(start.elapsed() < Duration::from_secs(60), "did not end");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(pty.activity().launches, 0);

        let opened = now_ms();
        pty.spawn(
            info("act-shell", &sh),
            &sh,
            "",
            &cwd,
            (80, 24),
            vec![],
            |_| {},
            |_| {},
        )
        .unwrap();
        let first = pty.activity().shell_io_at.expect("a terminal open");
        assert!(first >= opened);
        std::thread::sleep(Duration::from_millis(30));
        let typed = now_ms();
        pty.write("act-shell", b" ").unwrap();
        assert!(pty.activity().shell_io_at.unwrap() >= typed);
        assert_eq!(pty.activity().launches, 0);
        pty.kill("act-shell");
        assert_eq!(pty.activity(), Activity::default());
    }
}
