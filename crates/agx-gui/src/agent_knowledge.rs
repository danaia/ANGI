//! The desktop side of ANGI's knowledge graph: the Memory view's worker and model updates.
//!
//! The graph lives in the agent runtime (`agent memory`), not in any AI provider. This module keeps no copy of
//! it: every edit is sent to the runtime, and the view is rebuilt from what the runtime reads back.
use crate::{
    AgentKnowledge, AppWindow, KnowledgeDetail, KnowledgeEvent, KnowledgeLink, KnowledgeNode,
    KnowledgeUse,
};
use serde_json::Value;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{cell::RefCell, process::Command, rc::Rc, sync::mpsc, time::Duration};

type Args = Vec<String>;

/// What the view last asked for, so a change can re-read the same list and memory.
#[derive(Default)]
struct View {
    query: String,
    kind: String,
}

fn request(args: &[String]) -> Result<Value, String> {
    let output = Command::new("/usr/bin/agent")
        .arg("memory")
        .args(args)
        .output()
        .map_err(|_| "Agent Angi’s runtime is not available.".to_string())?;
    let answer: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "Memory could not be read. Try again.".to_string())?;
    match answer["error"].as_str() {
        Some(error) => Err(error.to_string()),
        None => Ok(answer),
    }
}

fn node(value: &Value) -> Option<KnowledgeNode> {
    let text = |key: &str| SharedString::from(value[key].as_str().unwrap_or(""));
    Some(KnowledgeNode {
        id: i32::try_from(value["id"].as_i64()?).ok()?,
        kind: text("kind"),
        name: text("name"),
        summary: text("summary"),
        when: text("when"),
        pinned: value["pinned"].as_bool().unwrap_or(false),
        edited: value["edited"].as_bool().unwrap_or(false),
        source: text("source"),
        links: i32::try_from(value["links"].as_i64().unwrap_or(0)).unwrap_or(0),
    })
}

fn rows<T>(value: &Value, convert: impl Fn(&Value) -> Option<T>) -> Vec<T> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(convert)
        .collect()
}

/// "0 B", "812 B", "4.2 KB", "38 KB", "1.4 MB": the graph's size as a person reads it.
fn size_text(bytes: i64) -> String {
    let bytes = bytes.max(0);
    let (value, unit) = match bytes {
        0..1024 => return format!("{bytes} B"),
        1024..1_048_576 => (bytes as f64 / 1024.0, "KB"),
        _ => (bytes as f64 / 1_048_576.0, "MB"),
    };
    if value < 10.0 {
        format!("{value:.1} {unit}")
    } else {
        format!("{value:.0} {unit}")
    }
}

fn plural(count: i64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// "12 memories: 2 projects, 5 chats". Topics are the bulk of the graph and add noise here, so they stay out.
fn summary_text(counts: &Value) -> String {
    let count = |kind: &str| counts[kind].as_i64().unwrap_or(0);
    let total: i64 = counts
        .as_object()
        .map(|all| all.values().filter_map(Value::as_i64).sum())
        .unwrap_or(0);
    if total == 0 {
        return String::new();
    }
    let parts: Vec<String> = [
        ("project", "project", "projects"),
        ("conversation", "chat", "chats"),
        ("system", "system change", "system changes"),
        ("work", "piece of work", "pieces of work"),
        ("note", "note", "notes"),
    ]
    .iter()
    .filter(|(kind, ..)| count(kind) > 0)
    .map(|(kind, one, many)| plural(count(kind), one, many))
    .collect();
    let lead = plural(total, "memory", "memories");
    if parts.is_empty() {
        lead
    } else {
        format!("{lead}: {}", parts.join(", "))
    }
}

fn show_overview(memory: &AgentKnowledge<'_>, answer: &Value) {
    memory.set_enabled(answer["enabled"].as_bool().unwrap_or(true));
    memory.set_size(size_text(answer["bytes"].as_i64().unwrap_or(0)).into());
    memory.set_total(
        answer["counts"]
            .as_object()
            .map(|all| all.values().filter_map(Value::as_i64).sum::<i64>())
            .and_then(|total| i32::try_from(total).ok())
            .unwrap_or(0),
    );
    memory.set_counts(summary_text(&answer["counts"]).into());
    memory.set_nodes(ModelRc::new(VecModel::from(rows(&answer["nodes"], node))));
    let used = rows(&answer["used"], |row| {
        Some(KnowledgeUse {
            when: row["when"].as_str()?.into(),
            mode: row["mode"].as_str()?.into(),
            names: row["names"].as_str()?.into(),
        })
    });
    memory.set_used(ModelRc::new(VecModel::from(used)));
}

fn show_detail(memory: &AgentKnowledge<'_>, answer: &Value) {
    let Some(opened) = node(&answer["node"]) else {
        return;
    };
    let events = rows(&answer["node"]["events"], |row| {
        Some(KnowledgeEvent {
            when: row["when"].as_str()?.into(),
            text: row["text"].as_str()?.into(),
        })
    });
    let links = rows(&answer["node"]["links"], |row| {
        Some(KnowledgeLink {
            id: i32::try_from(row["id"].as_i64()?).ok()?,
            kind: row["kind"].as_str()?.into(),
            name: row["name"].as_str()?.into(),
            relation: row["relation"].as_str()?.into(),
            outgoing: row["direction"].as_str() == Some("out"),
        })
    });
    memory.set_detail(KnowledgeDetail {
        node: opened,
        created: answer["node"]["created"].as_str().unwrap_or("").into(),
        events: ModelRc::new(VecModel::from(events)),
        links: ModelRc::new(VecModel::from(links)),
    });
}

pub(super) fn install(ui: &AppWindow) -> Timer {
    let (requests, worker) = mpsc::channel::<Args>();
    let (results, updates) = mpsc::channel::<(Args, Result<Value, String>)>();
    std::thread::spawn(move || {
        for args in worker {
            if results.send((args.clone(), request(&args))).is_err() {
                break;
            }
        }
    });
    let view = Rc::new(RefCell::new(View::default()));
    let memory = ui.global::<AgentKnowledge>();
    let send = {
        let weak = ui.as_weak();
        move |args: Args| {
            if let Some(ui) = weak.upgrade() {
                let memory = ui.global::<AgentKnowledge>();
                memory.set_pending(memory.get_pending() + 1);
                let _ = requests.send(args);
            }
        }
    };
    let (list, tracked) = (send.clone(), view.clone());
    memory.on_reload(move |query, kind| {
        *tracked.borrow_mut() = View {
            query: query.to_string(),
            kind: kind.to_string(),
        };
        list(vec!["list".into(), query.into(), kind.into()]);
    });
    let open = send.clone();
    memory.on_choose({
        let weak = ui.as_weak();
        move |id| {
            if let Some(ui) = weak.upgrade() {
                ui.global::<AgentKnowledge>().set_selected(id);
            }
            open(vec!["open".into(), id.to_string()]);
        }
    });
    let edit = send.clone();
    memory.on_edit(move |id, name, summary| {
        edit(vec!["edit".into(), id.to_string(), name.into(), summary.into()]);
    });
    let pin = send.clone();
    memory.on_pin(move |id, on| {
        pin(vec![
            "pin".into(),
            id.to_string(),
            if on { "on" } else { "off" }.into(),
        ]);
    });
    let forget = send.clone();
    memory.on_forget(move |id| forget(vec!["forget".into(), id.to_string()]));
    let unlink = send.clone();
    memory.on_unlink(move |id, other| unlink(vec!["unlink".into(), id.to_string(), other.to_string()]));
    let note = send.clone();
    memory.on_add_note(move |text| note(vec!["note".into(), text.into()]));
    let switch = send.clone();
    memory.on_set_enabled(move |on| {
        switch(vec!["enabled".into(), if on { "on" } else { "off" }.into()]);
    });
    let clear = send.clone();
    memory.on_clear_all(move || clear(vec!["clear".into()]));

    // The header shows the graph's size beside its button, so read it now and then once a minute: agents add
    // to the graph in the background.
    send(vec!["size".into()]);
    let (weak, timer, mut ticks) = (ui.as_weak(), Timer::default(), 0_u32);
    timer.start(TimerMode::Repeated, Duration::from_millis(33), move || {
        ticks = ticks.wrapping_add(1);
        if ticks % 1800 == 0 {
            send(vec!["size".into()]);
        }
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let memory = ui.global::<AgentKnowledge>();
        while let Ok((args, result)) = updates.try_recv() {
            memory.set_pending((memory.get_pending() - 1).max(0));
            let command = args.first().map(String::as_str).unwrap_or("");
            let answer = match result {
                Ok(answer) => {
                    memory.set_error("".into());
                    answer
                }
                Err(error) => {
                    memory.set_error(error.into());
                    if command == "open" {
                        // The memory vanished (forgotten elsewhere): leave its page and show what remains.
                        memory.set_selected(-1);
                        let view = view.borrow();
                        send(vec!["list".into(), view.query.clone(), view.kind.clone()]);
                    }
                    continue;
                }
            };
            match command {
                "list" => show_overview(&memory, &answer),
                "size" => memory.set_size(size_text(answer["bytes"].as_i64().unwrap_or(0)).into()),
                "open" => {
                    if memory.get_selected() == answer["node"]["id"].as_i64().unwrap_or(-2) as i32 {
                        show_detail(&memory, &answer);
                    }
                }
                _ => {
                    // A change was saved: read the list again, and the open memory unless it is gone.
                    if matches!(command, "forget" | "clear") {
                        memory.set_selected(-1);
                    }
                    if command == "note"
                        && let Some(id) = answer["id"].as_i64().and_then(|id| i32::try_from(id).ok())
                    {
                        memory.set_selected(id);
                    }
                    let view = view.borrow();
                    send(vec!["list".into(), view.query.clone(), view.kind.clone()]);
                    if memory.get_selected() >= 0 {
                        send(vec!["open".into(), memory.get_selected().to_string()]);
                    }
                }
            }
        }
    });
    timer
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summary_counts_what_people_recognise_and_skips_topics() {
        let counts = json!({"project": 1, "conversation": 3, "topic": 40, "system": 2});
        assert_eq!(
            summary_text(&counts),
            "46 memories: 1 project, 3 chats, 2 system changes"
        );
        assert_eq!(summary_text(&json!({"topic": 1})), "1 memory");
        assert_eq!(summary_text(&json!({})), "");
    }

    #[test]
    fn sizes_read_like_a_file_manager() {
        let shown: Vec<String> = [0, 812, 1024, 4300, 38_912, 1_468_006, -5]
            .map(size_text)
            .into();
        assert_eq!(shown, ["0 B", "812 B", "1.0 KB", "4.2 KB", "38 KB", "1.4 MB", "0 B"]);
    }

    #[test]
    fn nodes_without_a_usable_id_are_skipped() {
        assert!(node(&json!({"id": "x", "name": "n"})).is_none());
        assert!(node(&json!({"id": 4_000_000_000_i64})).is_none());
        let kept = node(&json!({"id": 7, "kind": "note", "name": "n", "pinned": true})).unwrap();
        assert_eq!((kept.id, kept.pinned, kept.links), (7, true, 0));
    }
}
