//! Integration tests of « Vue Claude Code » (`tui`): the fake `claude` plays Claude Code's
//! interface in a real terminal, calls the hooks of its settings over HTTP and writes its
//! transcript, which the core reads as it comes.

use crate::core::AgentOptions;
use crate::core_tests::{agent_on, harness, plain_text, second_account, Harness};
use crate::model::*;
use std::sync::Arc;

/// An agent on a Claude account of its own (the fake keeps its transcripts in its folder), in the
/// Claude Code view, its interface started.
async fn agent_in_tui(h: &Harness) -> String {
    let (p, _) = h.project(false).await;
    second_account(h);
    let id = agent_on(h, &p, "pro").await.id;
    h.core.set_agent_view(&id, true).await.unwrap();
    let screen = Arc::new(parking_lot::Mutex::new(String::new()));
    let sink = screen.clone();
    let info = h
        .core
        .open_agent_terminal(&id, (100, 30), move |b| {
            sink.lock().push_str(&String::from_utf8_lossy(&b))
        })
        .await
        .unwrap();
    let start = std::time::Instant::now();
    let mut answered = false;
    while !plain_text(&screen.lock()).contains("fake-claude-tui") {
        // As xterm.js does in the window: the pseudo-console (Windows) asks where the cursor is
        // before it runs anything.
        if !answered && screen.lock().contains("\x1b[6n") {
            h.core.pty.write(&info.id, b"\x1b[1;1R").unwrap();
            answered = true;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(60),
            "the interface's first screen not in {:?}",
            screen.lock()
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    id
}

fn texts(h: &Harness, id: &str, kind: &str) -> Vec<String> {
    h.items(id)
        .iter()
        .filter(|i| i["kind"] == kind)
        .map(|i| i["text"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_goes_through_the_interface_and_its_turn_comes_back_from_the_transcript() {
    let h = harness("tui-turn");
    let id = agent_in_tui(&h).await;
    let a = h.agent(&id);
    assert!(a.terminal);
    let session = a
        .session_id
        .clone()
        .expect("a session of its own from the start");
    // No stream's process: the interface has the session.
    assert!(!h.alive(&id));
    assert!(h.core.agent(&id).unwrap().lock().tui.is_some());

    h.core
        .send_message(&id, "bonjour".into(), Vec::new())
        .await
        .unwrap();
    h.wait("the turn's end", |h| {
        h.items(&id).iter().any(|i| i["kind"] == "turn")
    })
    .await;
    let user = h
        .items(&id)
        .into_iter()
        .find(|i| i["kind"] == "user")
        .unwrap();
    assert_eq!(user["text"], "bonjour");
    assert_eq!(user["origin"], "terminal");
    assert_eq!(texts(&h, &id, "text"), ["Fini : bonjour"]);
    let a = h.agent(&id);
    assert_eq!(a.status, AgentStatus::Done);
    assert_eq!(a.prompts, 1);
    assert!(a.cost > 0.0, "counted at list prices");
    assert_eq!(a.session_id.as_deref(), Some(session.as_str()));
    // Where the read stopped is kept, for the next start.
    let (file, offset) = a.transcript_read.clone().unwrap();
    assert!(file.ends_with(&format!("{session}.jsonl")));
    assert_eq!(offset, std::fs::metadata(&file).unwrap().len());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_permission_asked_in_the_interface_is_told_and_answered_there() {
    let h = harness("tui-permission");
    let id = agent_in_tui(&h).await;
    h.core
        .send_message(&id, "permission".into(), Vec::new())
        .await
        .unwrap();
    h.wait("the agent waiting", |h| {
        h.agent(&id).status == AgentStatus::Waiting
    })
    .await;
    assert!(
        h.core.alerts.lock().iter().any(|a| a.contains("echo tui")),
        "{:?}",
        h.core.alerts.lock()
    );
    // A message would go to its dialog: refused.
    let refused = h.core.send_message(&id, "et ça ?".into(), Vec::new()).await;
    assert!(refused.is_err());
    // Answered in its terminal (Enter: « Yes »).
    let term = h
        .core
        .agent(&id)
        .unwrap()
        .lock()
        .view()
        .terminal_id
        .unwrap();
    h.core.pty.write(&term, b"\r").unwrap();
    h.wait("the turn's end", |h| {
        h.agent(&id).status == AgentStatus::Done
    })
    .await;
    let tool = h
        .items(&id)
        .into_iter()
        .find(|i| i["kind"] == "tool")
        .unwrap();
    assert_eq!(
        (tool["name"].as_str(), tool["status"].as_str()),
        (Some("Bash"), Some("ok"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupt_from_the_window_is_escape_in_the_interface() {
    let h = harness("tui-interrupt");
    let id = agent_in_tui(&h).await;
    h.core
        .send_message(&id, "lent".into(), Vec::new())
        .await
        .unwrap();
    h.wait("the turn running", |h| {
        h.agent(&id).status == AgentStatus::Running
    })
    .await;
    h.core.interrupt(&id).await.unwrap();
    h.wait("the turn interrupted", |h| {
        h.items(&id)
            .iter()
            .any(|i| i["kind"] == "turn" && i["interrupted"] == true)
    })
    .await;
}

/// `text` typed in the interface, until its turn comes back from the transcript.
async fn say_in_tui(h: &Harness, id: &str, text: &str) {
    let turns = |h: &Harness| h.items(id).iter().filter(|i| i["kind"] == "turn").count();
    let before = turns(h);
    h.core
        .send_message(id, text.into(), Vec::new())
        .await
        .unwrap();
    h.wait("the turn's end", |h| {
        turns(h) > before && !h.agent(id).status.is_active()
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn in_the_claude_code_view_commands_do_not_name_the_agent_but_the_next_ordinary_message_does()
{
    let h = harness("tui-name-after-commands");
    h.core.settings.write().claude_language = "en".into();
    let id = agent_in_tui(&h).await;
    for command in ["/clear", "/impeccable"] {
        say_in_tui(&h, &id, command).await;
        let a = h.agent(&id);
        assert_eq!((a.named, a.name.as_str()), (false, "agent-1"), "{command}");
        assert!(!h.core.agent(&id).unwrap().lock().naming_asked, "{command}");
    }
    say_in_tui(&h, &id, "Create the hello file").await;
    h.wait("named", |h| h.agent(&id).named).await;
    assert_eq!(h.agent(&id).name, "create-the-fake-en");
}

#[tokio::test(flavor = "multi_thread")]
async fn in_the_claude_code_view_the_automatic_resume_never_names_the_agent() {
    let h = harness("tui-name-not-by-continue");
    h.core.settings.write().claude_language = "en".into();
    let id = agent_in_tui(&h).await;
    say_in_tui(&h, &id, "/impeccable").await;
    // « continue », typed by the app itself after the usage limit: its entry in the transcript is
    // read like any other, and is no task.
    let turns = |h: &Harness| h.items(&id).iter().filter(|i| i["kind"] == "turn").count();
    let before = turns(&h);
    h.core.send_continue(&id).await.unwrap();
    h.wait("the resumed turn's end", |h| {
        turns(h) > before && !h.agent(&id).status.is_active()
    })
    .await;
    assert_eq!(texts(&h, &id, "user"), ["/impeccable", "continue"]);
    let a = h.agent(&id);
    assert_eq!((a.named, a.name.as_str()), (false, "agent-1"));
    assert!(!h.core.agent(&id).unwrap().lock().naming_asked);
    say_in_tui(&h, &id, "Create the hello file").await;
    h.wait("named", |h| h.agent(&id).named).await;
    assert_eq!(h.agent(&id).name, "create-the-fake-en");
}

#[tokio::test(flavor = "multi_thread")]
async fn in_the_claude_code_view_a_skill_with_arguments_names_the_agent_from_them() {
    let h = harness("tui-name-from-skill");
    h.core.settings.write().claude_language = "en".into();
    let id = agent_in_tui(&h).await;
    say_in_tui(&h, &id, "/impeccable propose un nouveau design").await;
    h.wait("named", |h| h.agent(&id).named).await;
    assert_eq!(h.agent(&id).name, "impeccable-propose-fake-en");
}

#[tokio::test(flavor = "multi_thread")]
async fn back_in_escouades_view_the_stream_resumes_the_same_session() {
    let h = harness("tui-back");
    let id = agent_in_tui(&h).await;
    h.core
        .send_message(&id, "bonjour".into(), Vec::new())
        .await
        .unwrap();
    h.wait("the turn's end", |h| {
        h.agent(&id).status == AgentStatus::Done
    })
    .await;
    let session = h.agent(&id).session_id.unwrap();
    let cwd = std::path::PathBuf::from(h.agent(&id).cwd);
    // The warm-up when the agent was made started one already, with no session to resume.
    let streams = |h: &Harness| {
        h.launch_log(&cwd)
            .into_iter()
            .filter(|l| l["argv"].to_string().contains("stream-json"))
            .collect::<Vec<_>>()
    };
    let before = streams(&h).len();
    h.core.set_agent_view(&id, false).await.unwrap();
    assert!(!h.agent(&id).terminal);
    assert!(h.core.agent(&id).unwrap().lock().tui.is_none());
    h.wait("the stream's process", |h| streams(h).len() > before)
        .await;
    let last = streams(&h).pop().unwrap();
    let args: Vec<&str> = last["argv"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a.as_str())
        .collect();
    assert!(
        args.contains(&format!("--resume={session}").as_str()),
        "{args:?}"
    );
    assert!(args.contains(&"stream-json"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tickets_agent_never_goes_to_the_claude_code_view() {
    let h = harness("tui-ticket");
    let (p, _) = h.project(false).await;
    let a = h
        .core
        .create_agent_with(
            &p.id,
            AgentOptions {
                ticket_id: Some("t1".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .meta;
    assert!(h.core.set_agent_view(&a.id, true).await.is_err());
    assert!(!h.agent(&a.id).terminal);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hook_without_the_token_of_an_interface_is_refused() {
    let h = harness("tui-token");
    let _id = agent_in_tui(&h).await;
    let port = h.core.hooks_port().unwrap();
    let call = |auth: Option<&str>| {
        let mut req = format!(
            "POST /hook HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n"
        );
        if let Some(a) = auth {
            req.push_str(&format!("Authorization: Bearer {a}\r\n"));
        }
        req.push_str("\r\n{}");
        use std::io::{Read, Write};
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        s.write_all(req.as_bytes()).unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        out.lines().next().unwrap_or_default().to_string()
    };
    assert!(call(None).contains("401"));
    assert!(call(Some("not-a-token")).contains("401"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_image_pasted_in_the_interface_is_written_to_a_file_whose_path_is_typed_there() {
    use base64::Engine;
    let h = harness("tui-paste");
    let id = agent_in_tui(&h).await;
    let pastes = h.dir.join("data").join("tui").join("pastes");
    // Kept a week, for Claude Code reads it again when the image's message is resumed or the
    // model looks at its source: older ones go as the next one comes.
    std::fs::create_dir_all(&pastes).unwrap();
    let aged = |name: &str, days: u64| {
        let f = pastes.join(name);
        std::fs::write(&f, b"old").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&f)
            .unwrap()
            .set_modified(
                std::time::SystemTime::now() - std::time::Duration::from_secs(days * 24 * 3600),
            )
            .unwrap();
        f
    };
    let (old, recent) = (aged("old.png", 8), aged("recent.png", 6));

    let png = [137u8, 80, 78, 71, 13, 10, 26, 10];
    let data = base64::engine::general_purpose::STANDARD.encode(png);
    let path = h.core.paste_image(&id, "image/png", &data).unwrap();
    let file = std::path::Path::new(&path);
    assert_eq!(file.parent(), Some(pastes.as_path()));
    assert_eq!(file.extension().unwrap(), "png");
    assert_eq!(std::fs::read(file).unwrap(), png);
    assert!(!old.exists());
    assert!(recent.exists());

    // Each paste its own file, named after its type as Claude Code reads it.
    let jpeg = h.core.paste_image(&id, "image/jpeg", &data).unwrap();
    assert!(jpeg.ends_with(".jpg"), "{jpeg}");
    assert_ne!(jpeg, path);
    for refused in ["application/pdf", "image/svg+xml"] {
        assert!(
            h.core.paste_image(&id, refused, &data).is_err(),
            "{refused}"
        );
    }
    assert!(h
        .core
        .paste_image(&id, "image/png", "pas du base64")
        .is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_image_is_pasted_only_where_claude_codes_interface_runs() {
    use base64::Engine;
    let h = harness("tui-paste-none");
    let (p, _) = h.project(false).await;
    let id = agent_on(&h, &p, "principal").await.id;
    let data = base64::engine::general_purpose::STANDARD.encode([1u8]);
    let e = h.core.paste_image(&id, "image/png", &data).unwrap_err();
    assert!(e.to_string().contains("vue Claude Code"), "{e}");
}
