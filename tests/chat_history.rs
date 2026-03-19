mod common;
use common::HOME_LOCK;
use flycrys::session::{self, ChatMessage, WorkspaceConfig};

// ──────────────────────────────────────────────────────────────────────
// Chat history persistence
// ──────────────────────────────────────────────────────────────────────

#[test]
fn chat_history_save_load_roundtrip() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let messages = vec![
        ChatMessage::User {
            text: "Hello agent".to_string(),
        },
        ChatMessage::AssistantText {
            text: "I'll help you with that.".to_string(),
        },
        ChatMessage::ToolCall {
            tool_name: "Read".to_string(),
            tool_input: r#"{"file_path":"/src/main.rs"}"#.to_string(),
            output: String::new(),
            is_error: false,
        },
        ChatMessage::ToolCall {
            tool_name: "Edit".to_string(),
            tool_input: r#"{"file_path":"/src/main.rs","old_string":"old","new_string":"new"}"#
                .to_string(),
            output: "Applied edit".to_string(),
            is_error: false,
        },
        ChatMessage::System {
            text: "✓ Done ($0.0042)".to_string(),
        },
    ];

    session::save_chat_history("ws-test-1", &messages);
    let loaded = session::load_chat_history("ws-test-1");
    assert_eq!(loaded.len(), 5);

    match &loaded[0] {
        ChatMessage::User { text } => assert_eq!(text, "Hello agent"),
        _ => panic!("expected User message"),
    }
    match &loaded[1] {
        ChatMessage::AssistantText { text } => assert_eq!(text, "I'll help you with that."),
        _ => panic!("expected AssistantText"),
    }
    match &loaded[2] {
        ChatMessage::ToolCall {
            tool_name, output, ..
        } => {
            assert_eq!(tool_name, "Read");
            assert!(output.is_empty(), "Read tool has no output");
        }
        _ => panic!("expected ToolCall"),
    }
    match &loaded[3] {
        ChatMessage::ToolCall {
            tool_name,
            output,
            is_error,
            ..
        } => {
            assert_eq!(tool_name, "Edit");
            assert_eq!(output, "Applied edit");
            assert!(!is_error);
        }
        _ => panic!("expected ToolCall"),
    }
    match &loaded[4] {
        ChatMessage::System { text } => assert!(text.contains("Done")),
        _ => panic!("expected System message"),
    }
}

#[test]
fn chat_history_empty_workspace_returns_empty() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let loaded = session::load_chat_history("nonexistent-ws");
    assert!(loaded.is_empty());
}

#[test]
fn chat_history_delete() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let messages = vec![ChatMessage::User {
        text: "test".to_string(),
    }];
    session::save_chat_history("ws-del", &messages);
    assert_eq!(session::load_chat_history("ws-del").len(), 1);

    session::delete_chat_history("ws-del");
    assert!(session::load_chat_history("ws-del").is_empty());
}

#[test]
fn chat_history_overwrite() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let v1 = vec![ChatMessage::User {
        text: "first".to_string(),
    }];
    session::save_chat_history("ws-ow", &v1);

    let v2 = vec![
        ChatMessage::User {
            text: "first".to_string(),
        },
        ChatMessage::AssistantText {
            text: "reply".to_string(),
        },
        ChatMessage::System {
            text: "✓ Done".to_string(),
        },
    ];
    session::save_chat_history("ws-ow", &v2);

    let loaded = session::load_chat_history("ws-ow");
    assert_eq!(loaded.len(), 3);
}

#[test]
fn chat_history_corrupt_json_returns_empty() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let dir = tmp.path().join(".config").join("flycrys").join("sessions");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("corrupt_chat.json"), "NOT VALID JSON").unwrap();

    let loaded = session::load_chat_history("corrupt");
    assert!(loaded.is_empty());
}

#[test]
fn chat_history_tool_call_with_error() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let messages = vec![ChatMessage::ToolCall {
        tool_name: "Bash".to_string(),
        tool_input: r#"{"command":"exit 1"}"#.to_string(),
        output: "command failed".to_string(),
        is_error: true,
    }];
    session::save_chat_history("ws-err", &messages);
    let loaded = session::load_chat_history("ws-err");

    match &loaded[0] {
        ChatMessage::ToolCall {
            is_error, output, ..
        } => {
            assert!(is_error);
            assert_eq!(output, "command failed");
        }
        _ => panic!("expected ToolCall"),
    }
}

#[test]
fn chat_history_save_empty_clears_file() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let messages = vec![ChatMessage::User {
        text: "hello".to_string(),
    }];
    session::save_chat_history("ws-clr", &messages);
    assert_eq!(session::load_chat_history("ws-clr").len(), 1);

    // Save empty vec (simulates Clear button)
    session::save_chat_history("ws-clr", &[]);
    assert!(session::load_chat_history("ws-clr").is_empty());
}

#[test]
fn chat_history_serde_roundtrip_all_variants() {
    // Verify each ChatMessage variant survives JSON round-trip
    let messages = vec![
        ChatMessage::User {
            text: "msg with \"quotes\" & <angle>".to_string(),
        },
        ChatMessage::AssistantText {
            text: "**bold** and `code`".to_string(),
        },
        ChatMessage::ToolCall {
            tool_name: "Grep".to_string(),
            tool_input: r#"{"pattern":"fn main","path":"/src"}"#.to_string(),
            output: "src/main.rs:1:fn main() {}".to_string(),
            is_error: false,
        },
        ChatMessage::System {
            text: "✓ Done ($0.0100)".to_string(),
        },
    ];

    let json = serde_json::to_string(&messages).unwrap();
    let restored: Vec<ChatMessage> = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.len(), 4);

    // Verify special chars survive
    match &restored[0] {
        ChatMessage::User { text } => assert!(text.contains("<angle>")),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn chat_history_with_unicode() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let messages = vec![
        ChatMessage::User {
            text: "Допоможи з кодом 🔧".to_string(),
        },
        ChatMessage::AssistantText {
            text: "我来帮你修改代码 ✨".to_string(),
        },
    ];
    session::save_chat_history("ws-uni", &messages);
    let loaded = session::load_chat_history("ws-uni");

    match &loaded[0] {
        ChatMessage::User { text } => assert!(text.contains("🔧")),
        _ => panic!("expected User"),
    }
    match &loaded[1] {
        ChatMessage::AssistantText { text } => assert!(text.contains("✨")),
        _ => panic!("expected AssistantText"),
    }
}

#[test]
fn chat_history_large_tool_output() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let large_output = "x".repeat(100_000);
    let messages = vec![ChatMessage::ToolCall {
        tool_name: "Bash".to_string(),
        tool_input: r#"{"command":"cat big_file"}"#.to_string(),
        output: large_output.clone(),
        is_error: false,
    }];
    session::save_chat_history("ws-big", &messages);
    let loaded = session::load_chat_history("ws-big");

    match &loaded[0] {
        ChatMessage::ToolCall { output, .. } => assert_eq!(output.len(), 100_000),
        _ => panic!("expected ToolCall"),
    }
}

#[test]
fn chat_history_workspace_lifecycle() {
    // Full lifecycle: create workspace -> chat -> save -> restore -> delete
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let ws = WorkspaceConfig::new("/home/user/project");
    session::save_workspace_config(&ws);

    let history = vec![
        ChatMessage::User { text: "fix the bug".to_string() },
        ChatMessage::AssistantText { text: "I'll look at the code.".to_string() },
        ChatMessage::ToolCall {
            tool_name: "Read".to_string(),
            tool_input: r#"{"file_path":"/home/user/project/src/lib.rs"}"#.to_string(),
            output: String::new(),
            is_error: false,
        },
        ChatMessage::ToolCall {
            tool_name: "Edit".to_string(),
            tool_input: r#"{"file_path":"/home/user/project/src/lib.rs","old_string":"bug","new_string":"fix"}"#.to_string(),
            output: "Applied".to_string(),
            is_error: false,
        },
        ChatMessage::System { text: "✓ Done ($0.0200)".to_string() },
    ];
    session::save_chat_history(&ws.id, &history);

    // Simulate app restart: load workspace + history
    let restored_ws = session::load_workspace_config(&ws.id).unwrap();
    let restored_history = session::load_chat_history(&restored_ws.id);
    assert_eq!(restored_history.len(), 5);

    // Simulate closing the workspace
    session::delete_workspace_config(&ws.id);
    session::delete_chat_history(&ws.id);
    assert!(session::load_workspace_config(&ws.id).is_none());
    assert!(session::load_chat_history(&ws.id).is_empty());
}
