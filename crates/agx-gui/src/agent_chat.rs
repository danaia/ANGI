//! Run Chat turns off the UI thread and keep the active conversation tied to its session ID.
use crate::{AgentChat, AgentHistory, AgentScope, AppWindow, ChatMessage};
use serde_json::Value;
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use std::{process::{Command, Stdio}, sync::mpsc, time::Duration};

struct Reply {
    session_id: Option<String>,
    message: Result<String, String>,
}

fn parse_reply(stdout: &[u8]) -> Result<Reply, String> {
    let unavailable = || "Angi's runtime did not answer.".to_string();
    let answer: Value = serde_json::from_slice(stdout).map_err(|_| unavailable())?;
    let message = match (answer["text"].as_str(), answer["error"].as_str()) {
        (Some(text), _) => Ok(text.to_string()),
        (None, Some(error)) => Err(error.to_string()),
        _ => return Err(unavailable()),
    };
    Ok(Reply {
        session_id: answer["session_id"].as_str().map(str::to_string),
        message,
    })
}

fn ask(message: &str, session_id: &str, folder: &str, project: &str) -> Result<Reply, String> {
    let mut command = Command::new("/usr/bin/agent");
    command.arg("chat-turn");
    if session_id.is_empty() {
        command.arg("--new");
    } else {
        command.args(["--session", session_id]);
    }
    if !folder.is_empty() {
        command.args(["--folder", folder]);
    }
    let output = command
        .arg(message)
        .env("AGX_CHAT_PREVIEW_PROJECT", project)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "Agent Angi's runtime is not available.".to_string())?;
    parse_reply(&output.stdout)
}

pub(super) fn bubble(mine: bool, failed: bool, text: &str) -> ChatMessage {
    ChatMessage { mine, failed, text: text.into() }
}

pub(super) fn set_messages(chat: &AgentChat<'_>, messages: Vec<ChatMessage>) {
    chat.set_messages(ModelRc::new(VecModel::from(messages)));
}

fn append(chat: &AgentChat<'_>, message: ChatMessage) {
    let messages = chat.get_messages();
    if let Some(rows) = messages.as_any().downcast_ref::<VecModel<ChatMessage>>() {
        rows.push(message);
    }
}

pub(super) fn install(ui: &AppWindow) -> Timer {
    set_messages(&ui.global::<AgentChat>(), Vec::new());
    let (requests, worker) = mpsc::channel::<(String, String, String, String)>();
    let (replies, answers) = mpsc::channel();
    std::thread::spawn(move || {
        for (message, session_id, folder, project) in worker {
            if replies.send(ask(&message, &session_id, &folder, &project)).is_err() {
                break;
            }
        }
    });
    let weak = ui.as_weak();
    ui.global::<AgentChat>().on_send(move |text| {
        let Some(ui) = weak.upgrade() else { return; };
        let (chat, text) = (ui.global::<AgentChat>(), text.trim());
        if chat.get_busy() || ui.global::<AgentHistory>().get_loading() || text.is_empty() {
            return;
        }
        append(&chat, bubble(true, false, text));
        chat.set_busy(true);
        let scope = ui.global::<AgentScope>();
        let folder = scope.get_chat_folder().to_string();
        let project = scope.get_project().to_string();
        let session_id = chat.get_session_id().to_string();
        let _ = requests.send((text.to_string(), session_id, folder, project));
    });
    let weak = ui.as_weak();
    ui.global::<AgentChat>().on_new_chat(move || {
        let Some(ui) = weak.upgrade() else { return; };
        let chat = ui.global::<AgentChat>();
        if !chat.get_busy() && !ui.global::<AgentHistory>().get_loading() {
            set_messages(&chat, Vec::new());
            chat.set_session_id("".into());
        }
    });
    let (weak, timer) = (ui.as_weak(), Timer::default());
    timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        let Some(ui) = weak.upgrade() else { return; };
        let chat = ui.global::<AgentChat>();
        while let Ok(answer) = answers.try_recv() {
            chat.set_busy(false);
            let message = answer.and_then(|reply| {
                if let Some(id) = reply.session_id {
                    chat.set_session_id(id.into());
                }
                reply.message
            });
            match message {
                Ok(text) => append(&chat, bubble(false, false, &text)),
                Err(reason) => append(&chat, bubble(false, true, &reason)),
            }
            let history = ui.global::<AgentHistory>();
            if history.get_open() {
                history.invoke_reload();
            }
        }
    });
    timer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_preserve_a_session_id_even_on_an_error() {
        let said = parse_reply(br#"{"text":"Hey there!","session_id":"ses_one"}"#).unwrap();
        assert_eq!(said.message, Ok("Hey there!".into()));
        assert_eq!(said.session_id.as_deref(), Some("ses_one"));
        let failed = parse_reply(br#"{"error":"No credits","session_id":"ses_two"}"#).unwrap();
        assert_eq!(failed.message, Err("No credits".into()));
        assert_eq!(failed.session_id.as_deref(), Some("ses_two"));
        assert!(parse_reply(b"").is_err());
        assert!(parse_reply(b"{}").is_err());
    }

    #[test]
    fn a_failed_turn_is_marked_so_it_is_never_taken_for_angi() {
        let failure = bubble(false, true, "No credits");
        assert!(failure.failed && !failure.mine);
        let mine = bubble(true, false, "hi");
        assert!(mine.mine && !mine.failed);
    }
}
