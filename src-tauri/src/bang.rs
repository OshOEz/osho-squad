//! `!command` typed in the composer: run by the user in the agent's folder, shown in its
//! conversation as a Bash call, and handed to Claude before the next message as Claude Code does
//! (`<bash-input>`, `<bash-stdout>`, `<bash-stderr>`), without starting a turn.

use crate::core::Core;
use crate::model::{new_id, now_ms, ConvOp};
use crate::{job, proc, testlaunch};
use anyhow::Result;
use parking_lot::Mutex;
use serde_json::json;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

/// How long a command may run before it is stopped with what it started.
const LIMIT: Duration = Duration::from_secs(10 * 60);
/// What Claude is given of each output at most: its end.
const MAX_OUTPUT: usize = 30_000;
/// What the conversation shows of the outputs at most (as for Claude's own Bash calls).
const MAX_SHOWN: usize = 8000;

/// The commands each agent ran since its last message, ready for Claude. In memory only: lost
/// when the app quits.
static PENDING: LazyLock<Mutex<HashMap<String, Vec<String>>>> = LazyLock::new(Default::default);

/// What a command gave.
#[derive(Debug)]
struct Ran {
    stdout: String,
    stderr: String,
    failed: bool,
}

#[tauri::command(async)]
pub fn run_bang(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
    command: String,
) -> Result<(), String> {
    start(core.inner(), &id, command).map_err(|e| format!("{e:#}"))
}

/// Shows `command` running in the agent's conversation and runs it in the background.
fn start(core: &Arc<Core>, id: &str, command: String) -> Result<()> {
    let item_id = new_id();
    let (cwd, env) = core.with_agent(id, |rt, fx| {
        let item = json!({
            "kind": "tool", "id": item_id, "name": "Bash", "byUser": true, "status": "running",
            "input": { "command": command, "description": tr!("Lancée par toi", "Run by you") },
            "parent": null, "ts": now_ms(),
        });
        let op = ConvOp::Append { item };
        rt.conv.apply(&op);
        fx.ops.push(op);
        Ok((rt.meta.cwd.clone(), testlaunch::port_env(rt.meta.port_base)))
    })?;
    let (core, id) = (core.clone(), id.to_string());
    tauri::async_runtime::spawn(async move {
        let ran = exec(&command, &cwd, &env, LIMIT).await;
        let shown = keep_end(format!("{}\n{}", ran.stdout, ran.stderr).trim(), MAX_SHOWN);
        let patch = json!({
            "status": if ran.failed { "error" } else { "ok" },
            "result": { "isError": ran.failed, "text": shown },
        });
        let _ = core.with_agent(&id, |rt, fx| {
            let op = ConvOp::Patch { id: item_id, patch };
            rt.conv.apply(&op);
            fx.ops.push(op);
            Ok(())
        });
        PENDING
            .lock()
            .entry(id)
            .or_default()
            .push(context(&command, &ran));
    });
    Ok(())
}

/// The commands the agent `id` was not told of yet, as the head of its next message, and how
/// many they are (for `delivered`).
pub fn pending(id: &str) -> (String, usize) {
    let all = PENDING.lock();
    let ran = all.get(id).map(Vec::as_slice).unwrap_or_default();
    (ran.iter().map(|c| format!("{c}\n\n")).collect(), ran.len())
}

/// The first `n` commands of `pending` reached Claude: not given again.
pub fn delivered(id: &str, n: usize) {
    if let Some(ran) = PENDING.lock().get_mut(id) {
        ran.drain(..n.min(ran.len()));
    }
}

/// A command as Claude Code hands it to Claude.
fn context(command: &str, ran: &Ran) -> String {
    format!(
        "<bash-input>{command}</bash-input>\n<bash-stdout>{}</bash-stdout><bash-stderr>{}</bash-stderr>",
        ran.stdout, ran.stderr
    )
}

/// The user's shell running `command` (zsh without one; cmd under Windows).
fn shell(command: &str) -> tokio::process::Command {
    #[cfg(windows)]
    {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.raw_arg(format!("/C {command}"))
            .creation_flags(crate::claude::CREATE_NO_WINDOW);
        cmd
    }
    #[cfg(not(windows))]
    {
        let sh = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/zsh".into());
        let mut cmd = tokio::process::Command::new(sh);
        cmd.arg("-c").arg(command);
        cmd
    }
}

/// Runs `command` in `cwd` with `env` added; past `limit`, stopped with what it started.
async fn exec(command: &str, cwd: &str, env: &[(String, String)], limit: Duration) -> Ran {
    let mut cmd = shell(command);
    cmd.current_dir(cwd)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    proc::no_inherited_fds(&mut cmd);
    job::isolate(&mut cmd);
    let failed = |stderr: String| Ran {
        stdout: String::new(),
        stderr,
        failed: true,
    };
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return failed(e.to_string()),
    };
    // Dropped at the end: what the command left running goes with it.
    let _job = job::Job::for_child(&child);
    let out = match tokio::time::timeout(limit, proc::wait_output(child)).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return failed(e.to_string()),
        Err(_) => {
            let minutes = (limit.as_secs() / 60).max(1);
            return failed(tr!(
                "Arrêtée au bout de {minutes} min.",
                "Stopped after {minutes} min."
            ));
        }
    };
    let mut stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        let code = out.status.code().map_or("?".into(), |c| c.to_string());
        stderr = format!(
            "{stderr}{}",
            tr!("Code de sortie {code}", "Exit code {code}")
        );
    }
    Ran {
        stdout: keep_end(&String::from_utf8_lossy(&out.stdout), MAX_OUTPUT),
        stderr: keep_end(&stderr, MAX_OUTPUT),
        failed: !out.status.success(),
    }
}

/// The last `max` characters of `text`, headed by a note when its start is cut.
fn keep_end(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max {
        return text.to_string();
    }
    let tail: String = text.chars().skip(n - max).collect();
    format!("{}\n{tail}", tr!("… (début tronqué)", "… (start cut)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ccm-bang-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    #[tokio::test]
    async fn runs_in_the_folder_given_with_the_env_added() {
        let d = temp_dir("cwd");
        let env = [("BANG_TEST".to_string(), "42".to_string())];
        let ran = exec("pwd; echo $BANG_TEST", d.to_str().unwrap(), &env, LIMIT).await;
        assert_eq!(ran.stdout, format!("{}\n42\n", d.display()));
        assert_eq!(ran.stderr, "");
        assert!(!ran.failed);
    }

    #[tokio::test]
    async fn keeps_stderr_and_the_exit_code_of_a_failure() {
        let d = temp_dir("fail");
        let ran = exec(
            "echo out; echo oops >&2; exit 3",
            d.to_str().unwrap(),
            &[],
            LIMIT,
        )
        .await;
        assert_eq!(ran.stdout, "out\n");
        assert!(ran.stderr.starts_with("oops\n"), "{}", ran.stderr);
        assert!(ran.stderr.ends_with(" 3"), "{}", ran.stderr);
        assert!(ran.failed);
    }

    #[tokio::test]
    async fn stops_a_command_past_its_limit() {
        let d = temp_dir("slow");
        let begun = std::time::Instant::now();
        let ran = exec(
            "sleep 30",
            d.to_str().unwrap(),
            &[],
            Duration::from_millis(300),
        )
        .await;
        assert!(ran.failed);
        assert!(!ran.stderr.is_empty());
        assert!(begun.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_long_output_keeps_its_end() {
        assert_eq!(keep_end("abc", 3), "abc");
        let cut = keep_end("é123456789", 4);
        assert!(cut.starts_with('…'), "{cut}");
        assert!(cut.ends_with("\n6789"), "{cut}");
    }

    #[test]
    fn hands_the_commands_over_as_claude_code_does_then_forgets_them() {
        let ran = Ran {
            stdout: "a\n".into(),
            stderr: "b".into(),
            failed: false,
        };
        assert_eq!(
            context("ls", &ran),
            "<bash-input>ls</bash-input>\n<bash-stdout>a\n</bash-stdout><bash-stderr>b</bash-stderr>"
        );
        let id = "bang-test-agent";
        PENDING
            .lock()
            .insert(id.into(), vec!["one".into(), "two".into()]);
        assert_eq!(pending(id), ("one\n\ntwo\n\n".into(), 2));
        // One more ran while the message was being sent: kept for the next.
        PENDING.lock().get_mut(id).unwrap().push("three".into());
        delivered(id, 2);
        assert_eq!(pending(id), ("three\n\n".into(), 1));
        assert_eq!(pending("nobody"), (String::new(), 0));
    }
}
